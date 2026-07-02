//! Integration tests that spawn the real `json-tools` binary and exercise the
//! stdin -> process -> stdout (and exit-code) trigger path. Unit tests cover
//! library logic; these cover argument marshaling, exit codes, and the actual
//! preserved key order on the wire.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

fn cmd() -> Command {
    Command::cargo_bin("json-tools").unwrap()
}

/// Write `content` to a temp file and return its path (kept alive by the
/// returned `NamedTempFile`).
fn tmp_json(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn fmt_compact_from_stdin() {
    cmd()
        .arg("fmt")
        .arg("--compact")
        .write_stdin("{ \"a\" : 1 , \"b\" : 2 }")
        .assert()
        .success()
        .stdout("{\"a\":1,\"b\":2}\n");
}

#[test]
fn fmt_pretty_from_stdin_has_newlines() {
    cmd()
        .arg("fmt")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("\n"))
        .stdout(predicate::str::contains("  \"a\": 1"));
}

#[test]
fn fmt_preserves_input_key_order_on_the_wire() {
    // The whole reason for the preserve_order feature. Out-of-alpha input must
    // come out in input order through the real binary, not sorted.
    cmd()
        .arg("fmt")
        .arg("--compact")
        .write_stdin(r#"{"zebra":1,"apple":2,"mango":3}"#)
        .assert()
        .success()
        .stdout("{\"zebra\":1,\"apple\":2,\"mango\":3}\n");
}

#[test]
fn fmt_sort_keys() {
    cmd()
        .arg("fmt")
        .arg("--compact")
        .arg("--sort-keys")
        .write_stdin(r#"{"zebra":1,"apple":2,"mango":3}"#)
        .assert()
        .success()
        .stdout("{\"apple\":2,\"mango\":3,\"zebra\":1}\n");
}

#[test]
fn validate_accepts_good_json_exit_zero() {
    cmd()
        .arg("validate")
        .write_stdin(r#"{"ok":true}"#)
        .assert()
        .success();
}

#[test]
fn validate_rejects_bad_json_nonzero_with_location() {
    cmd()
        .arg("validate")
        .write_stdin(r#"{"oops": }"#)
        .assert()
        .failure()
        .stderr(predicate::str::contains("line"))
        .stderr(predicate::str::contains("column"));
}

#[test]
fn validate_bad_json_from_file_nonzero() {
    let f = tmp_json(r#"{not json"#);
    cmd().arg("validate").arg(f.path()).assert().failure();
}

#[test]
fn get_by_pointer_stdout() {
    cmd()
        .arg("get")
        .arg("/a/b/0")
        .write_stdin(r#"{"a":{"b":[42,43]}}"#)
        .assert()
        .success()
        .stdout("42\n");
}

#[test]
fn get_by_dot_path_stdout() {
    cmd()
        .arg("get")
        .arg("a.b.1")
        .write_stdin(r#"{"a":{"b":[42,43]}}"#)
        .assert()
        .success()
        .stdout("43\n");
}

#[test]
fn get_missing_path_fails() {
    cmd()
        .arg("get")
        .arg("/no/such")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .failure();
}

#[test]
fn get_object_preserves_order() {
    cmd()
        .arg("get")
        .arg("/obj")
        .arg("--compact")
        .write_stdin(r#"{"obj":{"zebra":1,"apple":2}}"#)
        .assert()
        .success()
        .stdout("{\"zebra\":1,\"apple\":2}\n");
}

#[test]
fn keys_top_level_input_order() {
    cmd()
        .arg("keys")
        .write_stdin(r#"{"zebra":1,"apple":2,"mango":3}"#)
        .assert()
        .success()
        .stdout("zebra\napple\nmango\n");
}

#[test]
fn keys_at_path() {
    cmd()
        .arg("keys")
        .arg("--path")
        .arg("nested")
        .write_stdin(r#"{"nested":{"k1":1,"k2":2}}"#)
        .assert()
        .success()
        .stdout("k1\nk2\n");
}

#[test]
fn flatten_then_unflatten_round_trips_via_cli() {
    let nested = r#"{"a":{"b":[{"c":1},{"c":2}]},"name":"x","tags":["p","q"]}"#;

    // flatten -> capture stdout
    let flat_out = cmd().arg("flatten").write_stdin(nested).assert().success();
    let flat_stdout = String::from_utf8(flat_out.get_output().stdout.clone()).unwrap();

    // unflatten the flattened output, compare to the original value.
    let restored = cmd()
        .arg("unflatten")
        .write_stdin(flat_stdout)
        .assert()
        .success();
    let restored_stdout = String::from_utf8(restored.get_output().stdout.clone()).unwrap();

    let restored_val: serde_json::Value = serde_json::from_str(&restored_stdout).unwrap();
    let original_val: serde_json::Value = serde_json::from_str(nested).unwrap();
    assert_eq!(restored_val, original_val);
}

#[test]
fn diff_detects_changes_text() {
    let a = tmp_json(r#"{"keep":1,"drop":2,"mod":3}"#);
    let b = tmp_json(r#"{"keep":1,"mod":4,"new":5}"#);
    cmd()
        .arg("diff")
        .arg(a.path())
        .arg(b.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("- /drop"))
        .stdout(predicate::str::contains("~ /mod"))
        .stdout(predicate::str::contains("+ /new"));
}

#[test]
fn diff_json_output() {
    let a = tmp_json(r#"{"x":1}"#);
    let b = tmp_json(r#"{"x":2}"#);
    let out = cmd()
        .arg("diff")
        .arg("--json")
        .arg(a.path())
        .arg(b.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(r#"[{"op":"change","path":"/x","from":1,"to":2}]"#).unwrap();
    assert_eq!(parsed, expected);
}

#[test]
fn merge_deep_later_wins_replace_array() {
    let a = tmp_json(r#"{"v":1,"nest":{"x":1},"list":[1,2,3]}"#);
    let b = tmp_json(r#"{"v":2,"nest":{"y":2},"list":[9]}"#);
    let out = cmd()
        .arg("merge")
        .arg("--compact")
        .arg(a.path())
        .arg(b.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let expected = serde_json::json!({"v":2,"nest":{"x":1,"y":2},"list":[9]});
    assert_eq!(parsed, expected);
}

#[test]
fn merge_array_concat() {
    let a = tmp_json(r#"{"list":[1,2]}"#);
    let b = tmp_json(r#"{"list":[3,4]}"#);
    let out = cmd()
        .arg("merge")
        .arg("--array")
        .arg("concat")
        .arg("--compact")
        .arg(a.path())
        .arg(b.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed, serde_json::json!({"list":[1,2,3,4]}));
}

#[test]
fn merge_three_files() {
    let a = tmp_json(r#"{"v":1,"a":"x"}"#);
    let b = tmp_json(r#"{"v":2,"b":"y"}"#);
    let c = tmp_json(r#"{"v":3,"c":"z"}"#);
    let out = cmd()
        .arg("merge")
        .arg("--compact")
        .arg(a.path())
        .arg(b.path())
        .arg(c.path())
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed, serde_json::json!({"v":3,"a":"x","b":"y","c":"z"}));
}

#[test]
fn to_csv_array_of_objects_preserves_key_order() {
    cmd()
        .arg("to-csv")
        .write_stdin(r#"[{"zebra":1,"apple":2},{"zebra":3,"apple":4}]"#)
        .assert()
        .success()
        .stdout("zebra,apple\n1,2\n3,4\n");
}

#[test]
fn to_csv_union_of_keys_and_escaping() {
    cmd()
        .arg("to-csv")
        .write_stdin(r#"[{"a":1,"b":"x,y"},{"a":2,"c":"he \"said\""}]"#)
        .assert()
        .success()
        .stdout("a,b,c\n1,\"x,y\",\n2,,\"he \"\"said\"\"\"\n");
}

#[test]
fn to_csv_escape_formulas_flag() {
    cmd()
        .arg("to-csv")
        .arg("--escape-formulas")
        .write_stdin(r#"[{"a":"=1+2","b":"safe"}]"#)
        .assert()
        .success()
        .stdout("a,b\n'=1+2,safe\n");
}

#[test]
fn to_csv_escape_formulas_off_by_default() {
    cmd()
        .arg("to-csv")
        .write_stdin(r#"[{"a":"=1+2","b":"safe"}]"#)
        .assert()
        .success()
        .stdout("a,b\n=1+2,safe\n");
}

#[test]
fn to_csv_rejects_non_object_array() {
    cmd()
        .arg("to-csv")
        .write_stdin(r#"[1,2,3]"#)
        .assert()
        .failure();
}

#[test]
fn output_to_file_flag() {
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("out.json");
    cmd()
        .arg("fmt")
        .arg("--compact")
        .arg("-o")
        .arg(&out_path)
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success();
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(written, "{\"a\":1}\n");
}
