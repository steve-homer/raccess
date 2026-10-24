# Development plan

## Goal

A single static binary that turns an Access database into a directory of plain,
diffable, tool-friendly files:

```sh
raccess extract sales.accdb -o ./sales-export
```

Read-only. No Access, no ODBC, no JVM, no mdbtools. Jet 3 (Access 97) through
Access 2019, both `.mdb` and `.accdb`.

## The decision that shaped this plan

The original plan was a seven-phase port of
[Jackcess](https://github.com/jahlborn/jackcess) — months of work, with forms,
reports, macros and VBA as genuine reverse engineering because **Jackcess does
not read them at all**.

Research found [`jetdb`](https://github.com/dominion525/jetdb): pure Rust,
MIT OR Apache-2.0, Jet 3 → Access 2019, and it already does tables, schema,
queries, forms, macros, VBA *and* all three encryption families. Verified by
building it and running it against real databases — see
[`prior-art.md`](prior-art.md).

What `jetdb` does not have is an extraction layer: it answers one query at a
time and prints text. **That gap is exactly this project.** So `raccess` wraps
`jetdb` rather than reimplementing it, and the parser work disappears from this
plan entirely.

## Status: the core is working

`raccess extract` produces the full output tree today —
[`output-layout.md`](output-layout.md) is the contract, and it is implemented,
not aspirational.

Verified against the 85-database Jackcess corpus: **all 85 extract with no
failures and no crashes**, yielding 302 tables, 539,822 rows and 45 queries.
Forms, macros and VBA were verified separately against `jetdb`'s own fixtures,
because the Jackcess corpus contains none (Jackcess cannot read them, so its
test databases have none to read). Office-encrypted `.accdb` extraction works
with `--password`; Jet RC4 `.mdb` encryption is handled transparently.

| Output | Status |
| --- | --- |
| `schema.json` — columns, types, indexes, relationships | ✅ |
| `tables/*.csv` | ✅ |
| `queries/*.json` + `*.sql` | ✅ |
| `forms/*.json`, `reports/*.json` — control tree and properties | ✅ |
| `macros/*.json` — statement tree | ✅ named macros only |
| `modules/*.bas`, `*.cls` — VBA source | ✅ |
| `manifest.json` — per-object status and warnings | ✅ |
| `blobs/` — attachment sidecars | ✅ |

## Next increments

Roughly in value order. Each is small now that the parser is not our problem.

### 1. Embedded and data macros

Only named macro objects are exported. Access also stores:

- **Embedded macros** — macros bound to a form or report event, via
  `read_embedded_macros`.
- **Data macros** — table event macros, via `read_data_macros`.

Both are one call per object and reuse the existing `Macro` serialisation. This
is the largest remaining fidelity gap.

### 2. Object properties

`jetdb`'s `prop` module reads the `MSysObjects.LvProp` blob: column
descriptions, formats, input masks, captions, validation rules. None of it is
in `schema.json` yet, and it is what makes an export useful for documenting a
database rather than just moving its rows.

### 3. Form and report RecordSource

Access stores a form's `RecordSource` SQL not with the form but in
`MSysQueries` under a synthetic `~sq_f<FormName>` name. We currently filter
`~sq_` queries out unless `--include-system` is passed. Resolving them and
attaching the SQL to the owning form's JSON would make form exports
self-contained.

### 4. Blob policy

Binary columns are hex-encoded inline in the CSV; only true attachments spill
to `blobs/`. `output-layout.md` specifies a `--blob-threshold` that is not yet
implemented — large OLE columns should become sidecar files with a path in the
cell.

### 5. Dependency hygiene

`jetdb` is pinned to a **git revision**, because the published 0.3.3 has no CSV
writer and no macro support at all. Move to a released version once upstream
publishes one. Until then the pin is exact and reproducible.

### 6. Coverage gaps

- No MSISAM (MS Money) fixture exists in either corpus.
- `--include-system` is implemented but not covered by a test.
- `--null-as` and the blob sidecar path are likewise untested.

Two gaps have closed. Access 2016 (version byte `0x05`) is now covered by
jetdb's `V2016/bigIntTestV2016.accdb`, which round-trips `9007199254740993`
exactly — past double precision, so BigInt is not going through a float. And
the test sweep now covers both corpora rather than only Jackcess, so the
forms/reports/macros/VBA paths are under automated test instead of manual
verification.

### 7. Scale

Everything is read into memory: a table's CSV is built in a `Vec<u8>` before
being written, and the source file is read whole to checksum it. Fine for the
corpus; wrong for a 2GB database. Stream CSV straight to the file and hash
incrementally.

## Risks

| Risk | Mitigation |
| --- | --- |
| `jetdb` bus factor — one author, 12 stars, v0.3.x | Permissively licensed, so we can vendor or fork at any point. Our layer is thin and the output contract is ours. |
| Pinned to an unreleased git revision | Exact rev, so builds are reproducible. Revisit on each upstream release. |
| `jetdb` bugs become our bugs | The corpus is the regression net; report upstream rather than patching around. |
| Form/macro formats are reverse-engineered upstream | Partial extraction is recorded per object in `manifest.json` rather than failing the run. |
| Scope creep into a writer | Explicitly read-only. |

## Non-goals

Writing or modifying databases, executing queries, evaluating VBA or Access
expressions, and reproducing form layout visually. Reimplementing the file
format parser — that is `jetdb`'s job.
