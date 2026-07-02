# json-tools

A JSON swiss-army knife on the command line. Reads from a file argument or
stdin, writes to `-o <file>` or stdout.

Object key order from the input is **preserved** throughout (serde_json is
built with the `preserve_order` feature) — `fmt` only reorders keys when you
ask it to with `--sort-keys`.

## Install / build

```sh
cargo build --release
# binary at target/release/json-tools
```

## Subcommands

### `fmt` — pretty-print or compact, optional key sort
```sh
json-tools fmt data.json                 # pretty (default)
json-tools fmt --compact data.json       # single line
json-tools fmt --sort-keys data.json     # sort keys recursively
cat data.json | json-tools fmt           # from stdin
```

### `validate` — exit 0 if valid, non-zero + line/col error if not
```sh
json-tools validate data.json && echo OK
echo '{"oops": }' | json-tools validate   # exits non-zero, prints line/column
```

### `flatten` / `unflatten` — nested <-> flat dotted/bracket keys
```sh
json-tools flatten nested.json
# {
#   "a.b[0].c": 1,
#   "tags[0]": "x"
# }
json-tools flatten nested.json | json-tools unflatten   # round-trips
```
Object members join with `.`, array elements use `[i]` (`a.b[0].c`). Empty
objects/arrays are kept as leaves so the round-trip is lossless. Object keys
that themselves contain `.`/`[`/`]` are emitted in the quoted form
`['weird.key']`.

### `get <path>` — extract by JSON Pointer or dot path
```sh
json-tools get /a/b/0 data.json     # RFC 6901 JSON Pointer
json-tools get a.b.0 data.json      # simple dot path (numeric -> array index)
```

### `keys` — list top-level keys (or keys at `--path`)
```sh
json-tools keys data.json
json-tools keys --path a.nested data.json
```

### `diff a.json b.json` — structural diff
```sh
json-tools diff old.json new.json          # text: '+' added, '-' removed, '~' changed
json-tools diff --json old.json new.json   # machine-readable JSON Patch-ish array
```
Paths are JSON Pointers. Arrays diff index-by-index; length changes appear as
added/removed trailing indices.

### `merge a.json b.json [...]` — deep-merge, later wins
```sh
json-tools merge base.json override.json
json-tools merge --array concat a.json b.json   # arrays concatenated
json-tools merge --array replace a.json b.json  # arrays replaced (default)
```
Objects merge recursively; existing key order is preserved and new keys from
later files append in order. On any type mismatch, the later value wins.

### `to-csv` — flatten an array of objects into CSV
```sh
json-tools to-csv users.json                    # header + one row per object
cat users.json | json-tools to-csv
json-tools to-csv --escape-formulas users.json  # neutralize spreadsheet formulas
```
Columns are the union of every object's keys in **first-seen order** (input key
order is preserved, not alphabetized). A lone object is treated as a single
row. Missing keys become empty fields; `null` is empty, nested objects/arrays
are emitted as compact JSON.

Fields containing a comma, double quote, CR, or LF are wrapped in double quotes
with embedded quotes doubled, following RFC 4180's field-quoting rules. Records,
however, are separated by a bare `\n` (LF), not RFC 4180's CRLF — matching the
Unix-newline output of the other subcommands.

Integers outside the `i64`/`u64` range are parsed as `f64`, so very large
values may render in scientific notation (e.g. `1e30`) rather than as exact
integers.

`--escape-formulas` (off by default) prefixes any cell — header or data —
beginning with `=`, `+`, `-`, or `@` with a single quote so spreadsheet apps
treat it as text. This defends against CSV/formula injection when the output is
opened in Excel, Google Sheets, or LibreOffice.

## Library

Every operation is a pure function over `serde_json::Value` in the
`json_tools` library crate (`fmt`, `validate` via `parse`, `flatten`,
`unflatten`, `get`, `keys`, `diff`, `merge`, `to_csv`). The binary is a thin
wrapper.

## License

MIT — see [LICENSE](LICENSE).
