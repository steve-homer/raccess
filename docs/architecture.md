# Architecture

One binary crate. The file format parser is [`jetdb`](https://github.com/dominion525/jetdb);
`raccess` is the extraction layer on top of it.

```text
         raccess
  ┌────────────────────┐
  │ main.rs            │  CLI: info, extract
  │ extract.rs         │  walk objects → write the output tree
  │ model.rs           │  serde shapes for the JSON we emit
  └─────────┬──────────┘
            │ jetdb's API
  ┌─────────▼──────────┐
  │ jetdb              │  pages, catalog, rows, queries,
  │  (pinned revision) │  forms, macros, VBA, encryption
  └────────────────────┘
```

## The three modules

**`main.rs`** parses arguments, opens the database, and maps the result to an
exit code: `0` all good, `1` something partial, `2` an object failed, `3` the
run could not start. Scripts can gate on that without reading the manifest.

**`extract.rs`** is the orchestration. One function per output kind
(`write_schema`, `write_tables`, `write_queries`, `write_forms`, `write_macros`,
`write_vba`), each appending to a shared `Vec<ObjectRecord>` that becomes
`manifest.json`.

**`model.rs`** exists because `jetdb` has no serde support. Every type in it
mirrors a `jetdb` struct to give it a stable JSON shape — which is the right
place for that coupling, since the output contract is ours to keep stable even
if `jetdb`'s types move.

## Design rules

- **Read-only.** No code path opens a database for writing.
- **Partial success over total failure.** One unreadable form must not abandon
  the other forty objects. Every failure is caught at the object boundary,
  recorded in the manifest with the reason, and the walk continues. This is why
  `write_forms`, `write_macros` and `write_vba` return `()` rather than
  `Result` — there is nothing for a caller to do about a single bad object.
- **An absent object kind is not an error.** Most databases have no forms and no
  VBA. `list_forms` failing means "no form storage here", so those directories
  are simply not created.
- **Deterministic output.** Keys serialise in declaration order and columns sort
  by display index, so two runs over the same file diff clean.
- **Names are not paths.** Access object names may contain `/`, `:`, `*` and
  control characters. `sanitize` percent-encodes those while keeping non-ASCII
  intact (Japanese form names are common), and `Namer` resolves collisions
  case-insensitively so exports are correct on Windows and macOS. The manifest
  always records the original name next to the file.

## Why no parser layer

An earlier draft of this document specified a six-layer parser — pager, page
views, catalog, row decoding, objects — as a port of Jackcess's 56k lines.
`jetdb` already implements all of it, permissively licensed, so none of it is
here. [`prior-art.md`](prior-art.md) records that decision and the evidence for
it.

The practical consequence: when extraction is wrong, the bug is usually
upstream. Reproduce it with `jetdb`'s own CLI first, and report it there rather
than working around it here.
