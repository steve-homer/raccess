# Prior art and licensing

## The headline finding: `jetdb` already exists

[`jetdb`](https://github.com/dominion525/jetdb) is a pure-Rust, read-only
Access reader that covers **more of this project's stated goal than Jackcess
does**. Verified by cloning, building and running it against our corpus on
2026-10-24:

| | |
| --- | --- |
| License | **MIT OR Apache-2.0** (both `LICENSE-MIT` and `LICENSE-APACHE` present) |
| Version | 0.3.3, first published 2026-02-16, last pushed 2026-10-24 |
| Reach | 13,468 downloads; 12 stars; 4 forks; 1 open issue |
| Size | ~39k lines of Rust, MSRV 1.85, no FFI |
| Crates | `jetdb`, `jetdb-cli`, `jetdb-wasm`, `complexity-filter` |

Its modules line up almost exactly with our phase list: `file.rs`, `format.rs`,
`data.rs`, `table.rs`, `catalog.rs`, `map.rs`, `complex.rs`, `crypto.rs`,
`query.rs`, `prop.rs`, `relationship.rs`, **`vba.rs`**, **`storage.rs`**,
**`form.rs` + `form2000.rs`**, **`macro_def.rs` / `macro_action.rs` /
`macro_text.rs`**, `csv.rs`, `save_as_text.rs`, `ddl/`.

### What it actually produced

Not claims from a README — observed output:

- **Jet 3 → Access 2019** all read, including `DateTimeExtended` (the 2016+
  type) and calculated columns, correctly typed in `jetdb schema`.
- **CSV export** of table data.
- **Queries** enumerated across all nine types in the Jackcess `queryTest`
  fixtures.
- **VBA** source extracted verbatim — `Attribute VB_Name`, comments,
  line-continuations and all — including from *encrypted* and Japanese-encoded
  databases.
- **Forms** as a control tree with names, types and ordering.
- **Macros** as their underlying AXL XML.

`jetdb-cli` subcommands: `ver`, `tables`, `schema`, `queries`, `prop`, `vba`,
`form`, `macro`, `export`.

### The gap

`jetdb` is **query-oriented, not extraction-oriented**. You ask it for one
object at a time and get text or TSV. It has no "walk the whole database and
write a directory" command, and no JSON object descriptions or manifest.

That gap is precisely the thing this project set out to build — which makes
`raccess` a thin orchestration layer over `jetdb` rather than a parser from
scratch. See [`development-plan.md`](development-plan.md).

### Risks of depending on it

Single author, 12 stars, v0.3.x — real bus-factor exposure. Mitigated by the
permissive license: we can vendor or fork at any point. Its declared
limitations — read-only, full table scan with no index lookups — are things we
wanted anyway.

## Jackcess

[github.com/jahlborn/jackcess](https://github.com/jahlborn/jackcess) — the
long-standing Java reference implementation. ~56k lines across 194 files,
developed since 2005. Current release 5.0.4.

**License: Apache-2.0**, verified from `LICENSE.txt` (verbatim Apache 2.0),
`pom.xml`, the GitHub API, and per-file headers (`Copyright (c) 2005 Health
Market Science, Inc.`).

⚠️ **The README states Jackcess is Apache-licensed "as of version 2.1.0"** —
earlier releases were not. Any porting must work from a 2.1.0-or-later tree.
Current `master` is fine.

Jackcess core has **zero required runtime dependencies**; its only optional one
is Apache POI (Apache-2.0) for compound-storage OLE blobs.

### What Jackcess covers

- `impl/JetFormat` — the entire per-version constant matrix in one file (~110
  fields via `defineXxx()` overrides). The license-safe normative spec.
- `impl/PageChannel` — page reads, plus the header mask.
- `impl/UsageMap` — which pages a table owns (inline and reference forms).
- `impl/DatabaseImpl` — the catalog, bootstrapped from `PAGE_SYSTEM_CATALOG = 2`.
- `impl/TableImpl` (3739 lines), `impl/ColumnImpl` (2933), `impl/NullMask`.
- `impl/LongValueColumnImpl` — LVAL memo/OLE values; the length field's top two
  bits select inline / single-page / chained.
- `impl/PropertyMaps` — the `MSysObjects.LvProp` blob (`KKD\0` Jet3, `MR2\0` Jet4+).
- `query/` + `impl/query/` — eleven query classes with SQL reconstruction.
- `impl/IndexData` (3049 lines) + six collation resource files — the expensive,
  skippable part.
- `impl/expr/` (25 files) — a full Access expression evaluator. Not needed for
  extraction.

System tables read: `MSysObjects`, `MSysQueries`, `MSysRelationships`,
`MSysACEs`, `MSysComplexColumns`, `MSysComplexTypeVH`, `MSysDb`.

### What Jackcess does not cover

**Forms, reports, macros and VBA — nothing at all.** A grep of the full tree for
form/report/module/VBA/macro/compound-file handling returns only false positives
(`JetFormat`, `FormatUtil`, `QueryFormat`, `ColumnFormatter`). There is no
`MSysAccessStorage` or `MSysAccessObjects` handling anywhere. Jackcess is a
data + schema + query library, and never claims otherwise.

This is the whole reason `jetdb` matters: for roughly a third of our feature
list, Jackcess is not a reference implementation at any price.

## Where the undocumented objects live

Established from `jetdb`'s implementation and docs (MIT/Apache-2.0, so
license-safe to read — unlike mdbtools):

- **`MSysAccessStorage`** (Jet 4 / ACE): a tree table of `Id`, `ParentId`,
  `Name`, `Type` plus binary data columns, reassembled into a **Compound File
  Binary (CFB)** tree holding form and report designs, macros and the VBA
  project.
- **`MSysAccessObjects`** (Jet 3): the same tree as one raw CFB blob.
- VBA inside that CFB follows **MS-OVBA** — `VBA` storage, `_VBA_PROJECT`
  stream, `dir` stream, per-module compressed streams.
- Access 2010+ macros are **AXL XML**, confirmed by observing the extracted
  `_AXL:` payload.
- ⚠️ Form and report `RecordSource`/`RowSource` SQL is **not stored with the
  form** — Access writes it into `MSysQueries` under synthetic `~sq_` names
  (`~sq_f` form, `~sq_r` report, `~sq_c` control, `~sq_d` report control). These
  usually lack the `Attribute=1` type row, so a strict query parser must handle
  or filter them. In one real database: 227 query objects, only 43 user-defined.

## Format version bytes

From `JetFormat`, confirmed against all 85 fixtures:

| Byte | Version | Page size | Validated |
| --- | --- | --- | --- |
| `0x00` | Jet 3 (Access 97) | 2048 | ✅ 9 files |
| `0x01` | Jet 4 (Access 2000–2003) | 4096 | ✅ 33 files |
| `0x01` + `MSISAM Database` engine | MS Money | 4096 | ❌ no fixture |
| `0x02` | Access 2007 | 4096 | ✅ 21 files |
| `0x03` | Access 2010 | 4096 | ✅ 19 files |
| `0x04` | Access 2013 — **never written**; 2013 emits 2010's format | — | n/a |
| `0x05` | Access 2016 | 4096 | ❌ no fixture |
| `0x06` | Access 2019 / 2021 | 4096 | ✅ 1 file |

There is **no "ACE 17"** — Jackcess's `VERSION_17` is a deprecated alias for
`VERSION_2019`, commented "Access 17 does not exist".

The engine name is at offset `0x04` (length `0x0F`) and the version byte at
`0x14`, both outside the masked region, which is why detection needs no
de-obfuscation.

### What varies across versions

Only two real base implementations exist — `Jet3Format` and `Jet4Format` — with
2007 → 2010 → 2016 → 2019 each overriding a handful of methods. **The 97→2000
jump changes page size and text encoding; 2007+ differs from 2000 almost
entirely in encryption and new column types, not page layout.** A favourable
shape for a port.

Jet 3 vs Jet 4: page size 2048/4096; charset DB-codepage/UTF-16LE; text field
unit 1/2 bytes; header mask 126/128 bytes; password size 20/40 at offset 66;
TDEF header 43/63 bytes; property magic `KKD\0`/`MR2\0`. Jackcess treats Jet 3
as **read-only**.

## Encryption and obfuscation

Three distinct things, and only the first is mandatory:

1. **Header XOR mask — always, on every file, including unprotected ones.**
   `PageChannel.applyHeaderMask()` XORs a fixed 128-byte constant
   (`JetFormat.BASE_HEADER_MASK`) over the header from `OFFSET_MASKED_HEADER =
   24`. Jet 3 uses the first 126 bytes. Page 0 only. **Required to read
   anything.**
2. **Embedded-key page encryption.** A 4-byte encoding key at
   `OFFSET_ENCODING_KEY = 62`. **If it is zero — the ordinary case — no
   decryption is needed at all**, which is why Jackcess core ships without any
   crypto dependency. If non-zero, pages are RC4'd with a key derived from that
   file-embedded value XOR the page number — no user password involved. The
   Access ≤2003 "database password" is likewise only software-enforced; the file
   is readable regardless.
3. **Real encryption** (`jackcess-encrypt`, also Apache-2.0, needs
   BouncyCastle): RC4 CryptoAPI, ECMA standard, and Office **agile** encryption
   per **MS-OFFCRYPTO** — i.e. accdb encryption is standard Office crypto, not
   an Access-specific scheme. `jetdb` implements all three in `crypto.rs`.

## mdbtools — do not read

[mdbtools](https://github.com/mdbtools/mdbtools) ships two license files:
`COPYING` is **GPL-2+** (the `src/util/` command-line tools) and `COPYING.LIB`
is **LGPL-2+** (`src/libmdb/`). Neither can source a permissively-licensed Rust
crate, and a line-by-line translation is a derivative work — which is exactly
why `mdbtools-pure-rs` is LGPL.

Its `HACKING.md` (~90KB) is nonetheless the best written format spec in
existence: pages, the database definition page, data pages, LVAL pages, TDEF
pages, usage maps, indices, the catalog, complex columns, properties, name maps,
text types, with per-version byte offsets. Treat it as **reference reading only**
— never transcribe it, and prefer `JetFormat` and `jetdb`'s docs, which are
permissively licensed.

## Other Rust prior art

| Crate | License | Verdict |
| --- | --- | --- |
| `jetdb` | MIT OR Apache-2.0 | ✅ the one that matters — see above |
| `accdt` | MIT OR Apache-2.0 | Reads `.accdt` template packages; useful reference for object blobs |
| `mdbtools-pure-rs` | **LGPL-2+** | ⚠️ avoid — carries libmdb's copyleft forward |
| `mdbsql` | Apache-2.0 crate, links libmdb | ⚠️ avoid — GPL/LGPL C at runtime |
| `mdbtools_rs` | MIT | Shells out to GPL binaries; abandoned 2020 |
| `pfisherogden/jackcess-rs` | **none** | ⚠️ unusable — an unlicensed derivative of an Apache-2.0 project |
| `oglassdev/jet3-rs` | none declared | Jet 3 only, very early |

Permissive building blocks: `cfb` (MIT) for CFB containers, `ovba` for MS-OVBA
(crates.io flags its license "non-standard" — **verify the LICENSE text before
depending on it**), `encoding_rs` + `codepage` for Jet 3 codepages,
`miniz_oxide` for VBA stream deflate.

## Public specifications

Microsoft has **never published a spec for the .mdb or .accdb format** —
confirmed via the Library of Congress format registry entries for
[MDB](https://www.loc.gov/preservation/digital/formats/fdd/fdd000462.shtml) and
[ACCDB](https://www.loc.gov/preservation/digital/formats/fdd/fdd000463.shtml),
both of which state no public specification exists and point at mdbtools.

Adjacent formats we need *are* specified, which is why VBA extraction is lower
risk than form extraction:

- [MS-OVBA](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/) — VBA project storage and source compression.
- [MS-CFB](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/) — the Compound File Binary container.
- [MS-OFFCRYPTO](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/) — Office document encryption, which accdb uses.

And `jetdb`'s own `docs/form-design.md` (47KB) documents the form/report design
blob layout — the thing nobody else documents — under a permissive license.
