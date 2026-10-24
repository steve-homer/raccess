# raccess

Extract everything out of a Microsoft Access database — `.mdb` or `.accdb`,
Access 97 through 2019 — into a directory of CSV and JSON. A single binary,
read-only, with no Access, no ODBC and no JVM.

```sh
raccess extract sales.accdb -o ./sales-export
```

```text
sales-export/
  manifest.json      what was extracted, and what failed
  schema.json        columns, types, indexes, relationships
  tables/*.csv       one file per table
  queries/*.json     + *.sql, reconstructed
  forms/*.json       control tree with named properties
  reports/*.json
  macros/*.json      statement tree
  modules/*.bas      VBA source
```

The full contract is in [`docs/output-layout.md`](docs/output-layout.md).

## Status

Working. Verified against the 85-database Jackcess corpus: all 85 extract with
no failures, yielding 302 tables and 539,822 rows. Forms, macros and VBA are
verified against jetdb's fixtures, since the Jackcess corpus contains none.

Remaining gaps — embedded macros, object properties, blob thresholds — are
listed in [`docs/development-plan.md`](docs/development-plan.md).

## Install

A Debian package for Ubuntu/Debian (including WSL):

```sh
cargo install cargo-deb      # once
cargo deb                    # -> target/debian/raccess_<version>_amd64.deb
sudo dpkg -i target/debian/raccess_0.1.0-1_amd64.deb
```

~1.1MB packaged, 3.4MB installed, binary at `/usr/bin/raccess`. The only
runtime dependencies are `libc6 (>= 2.34)` and `libgcc-s1`, both already
present on Ubuntu 22.04 and later. Docs land in
`/usr/share/doc/raccess/`. Remove with `sudo dpkg -r raccess`.

## Build

```sh
cargo build --release
./target/release/raccess extract path/to/db.accdb -o ./out
```

### Options

| Flag | Effect |
| --- | --- |
| `-o, --output <DIR>` | Output directory; created if absent |
| `--password <PW>` | For Office-encrypted `.accdb`. Jet-encrypted `.mdb` needs no password |
| `--include-system` | Also export `MSys*` tables and hidden `~sq_` queries |
| `--null-as <TEXT>` | Text written for NULL (default: empty) |

`raccess info <file>` reports the detected format without extracting anything.

Exit codes: `0` all good, `1` something partial, `2` an object failed, `3` the
run could not start.

## How it works

`raccess` is the extraction layer over
[`jetdb`](https://github.com/dominion525/jetdb), which does the file format
parsing. That choice, and the evidence for it, is recorded in
[`docs/prior-art.md`](docs/prior-art.md) — in short, jetdb already reads
everything we need under a compatible license, including the forms, macros and
VBA that Jackcess cannot read at all.

jetdb is pinned to an exact git revision: the published 0.3.3 has no CSV writer
and no macro support.

## Tests

```sh
./scripts/fetch-fixtures.sh            # once; vendors 202 databases (~211MB)
cargo test                             # fast: ~2s
cargo test -- --ignored --nocapture    # full sweep of all 202: ~25s
```

The fast suite checks one database per format version (Jet 3 → Access 2019),
that forms/macros/VBA actually come out of the fixtures that contain them, that
VBA source is real source, that non-ASCII object names survive to disk, and
that encrypted databases work with `--password` and fail without it.

The full sweep extracts every database in both corpora — 197 of them, plus 5
skipped as password-protected — and asserts no object kind is missing
entirely. Last run: 665 tables, 289 forms, 139 reports, 657 macros, 94 queries,
45 VBA modules.

Two corpora are vendored, neither committed: the
[Jackcess](https://github.com/jahlborn/jackcess) suite (85 databases, every
format version) and [jetdb](https://github.com/dominion525/jetdb)'s own (117,
and the only source of forms, macros, VBA and an Access 2016 file). Tests that
need them skip themselves when absent, so a fresh clone passes without the
download. See [`fixtures/README.md`](fixtures/README.md) for which file
exercises what.

## License

Apache-2.0. See [`NOTICE`](NOTICE) for attribution and
[`docs/prior-art.md`](docs/prior-art.md) for the licensing analysis of every
upstream project involved.
