# Test fixtures

Neither corpus is committed (~211MB together). Populate both with:

```sh
./scripts/fetch-fixtures.sh
```

Tests that need them skip themselves when absent, so a fresh clone still
passes. Both upstreams are permissively licensed, so redistribution is fine;
their LICENSE files and a `PROVENANCE-commit.txt` are copied alongside the data.

## `jackcess/` — 85 databases, broad version coverage

The [Jackcess](https://github.com/jahlborn/jackcess) test suite (Apache-2.0).

| Directory | Format | Files |
| --- | --- | --- |
| `V1997/` | Jet 3 | 9 |
| `V2000/` | Jet 4 | 16 |
| `V2003/` | Jet 4 | 17 |
| `V2007/` | ACE 12 | 21 |
| `V2010/` | ACE 14 | 19 |
| `V2019/` | Access 2019 | 1 |

Filenames say what each exercises: `queryTest*`, `indexTest*`, `compIndexTest*`
(composite indexes), `overflowTest*` (rows spanning pages), `delTest*` /
`delColTest*` (deleted rows and columns), `testOle*` (OLE columns),
`calcFieldTest*` (calculated columns), `extDateTestV2019` (the 2016+ extended
date/time type), `unsupportedFieldsTest*`.

**Contains no forms, reports, macros or VBA** — Jackcess cannot read those, so
its fixtures have none to read. Use the jetdb corpus for them.

V2000 and V2003 share the Jet 4 header version byte; telling them apart needs
the `AccessVersion` property, so both report `Jet4`.

## `jetdb/` — 117 databases, the only source of forms/VBA/macros

[jetdb](https://github.com/dominion525/jetdb)'s own test data (MIT OR
Apache-2.0). Includes a `V2016/` directory, which is the **only** Access 2016
(version byte `0x05`) fixture available anywhere — it closes a coverage gap the
Jackcess corpus leaves open.

Most useful for manual testing:

| File | What it exercises |
| --- | --- |
| `jetdb-sample.accdb` | A realistic small database: 3 tables with data, 1 query, 1 form, 1 macro, 2 VBA modules |
| `formPropTest.accdb` | 4 forms, 1 macro, 3 modules, 2 queries — the widest object mix |
| `controlPropertyTest*` | Form control property torture tests (~12MB) |
| `vbaJapaneseV2007.accdb` | Non-ASCII object names and source, CP932 |
| `db2007-enc.accdb` | Office-encrypted; password `Test123` |
| `enc_vbaV2003.mdb` | Jet RC4 encryption, handled without a password |
| `V2016/bigIntTestV2016.accdb` | Access 2016 and the BigInt type |
| `linkeeTest.accdb` | Linked tables |

`SOURCES.md` in that directory documents each file's provenance, and
`*-generators/` holds the VBA used to create them.

## `private/`

Gitignored. Drop real-world databases here for manual testing; nothing in this
directory is redistributable.
