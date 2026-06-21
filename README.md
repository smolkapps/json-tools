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

## Library

Every operation is a pure function over `serde_json::Value` in the
`json_tools` library crate (`fmt`, `validate` via `parse`, `flatten`,
`unflatten`, `get`, `keys`, `diff`, `merge`). The binary is a thin wrapper.

## License

MIT — see [LICENSE](LICENSE).
