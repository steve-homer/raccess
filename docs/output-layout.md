# Output layout

`raccess extract db.accdb -o ./export` writes:

```text
export/
  manifest.json           what was extracted, what failed, and from what
  schema.json             tables, columns, types, indexes, relationships
  tables/
    tblOrders.csv
  queries/
    qryMonthlySales.json  name, type, reconstructed SQL
    qryMonthlySales.sql   the same SQL on its own, for reading
  forms/
    frmOrders.json        control tree with named properties
  reports/
    rptInvoice.json
  macros/
    mcrOpenOrders.json    statement tree
  modules/
    modBilling.bas        VBA source
    Form_frmOrders.cls
  blobs/
    attachment.pdf
```

A directory appears only when the database holds objects of that kind — and
most databases have no forms and no VBA, so most exports are smaller than this.

## Filenames

Access object names may contain characters a path cannot. `/ \ : * ? " < > | %`
and control characters are percent-encoded; everything else, including spaces
and non-ASCII (Japanese form names are common), is kept as-is. Names that
collide once encoded — or that differ only in case, which matters on Windows
and macOS — get a `~2`, `~3` suffix.

`manifest.json` always records the original Access name next to the file, so
the mapping never has to be guessed.

## CSV

UTF-8, `,`-delimited, with a header row of column names. One file per table.
System and hidden tables are excluded unless `--include-system` is passed.

| Access type | Rendering |
| --- | --- |
| Text, Memo | verbatim; embedded newlines are quoted, not stripped |
| Yes/No | `TRUE` / `FALSE` |
| Date/Time | `YYYY-MM-DD` when the time is midnight, `YYYY-MM-DD HH:MM:SS` otherwise |
| Currency, Decimal | plain decimal at full stored precision, never scientific |
| Single, Double | shortest representation that round-trips |
| Replication ID | `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` |
| Binary, OLE | lowercase hex |
| Attachment | path to a `blobs/` sidecar, relative to the output root |
| Null | empty field, or `--null-as TEXT` |

Dates carry no timezone in Access and none is applied: values are emitted as
stored.

Access Text columns can legally hold C0 control characters, including NUL.
These are written through verbatim rather than silently altered, but a CSV
containing them will be rejected by strict readers (Python's `csv`, Postgres
`COPY`). When it happens the table is marked `partial` in the manifest with the
offending byte named, so it is never a silent surprise.

## JSON

UTF-8, pretty-printed, newline-terminated. Keys serialise in a fixed order and
table columns are sorted by display index, so two runs over the same database
produce byte-identical output and diff cleanly.

### Forms and reports

Each control carries its name, Access's type code, the resolved type name
(`TextBox`, `CommandButton`, …) and its properties. Properties are reported with
Access's own name where the id is a known one:

```json
{ "name": "Width", "id": 61, "value": 2400 }
```

An unrecognised id appears with `id` and `value` but no `name` — it is kept
rather than dropped, since the property tables are reverse-engineered and
incomplete.

### Macros

A statement tree, not flat text: actions with arguments, comments, `If`
branches, groups and submacros nest the way the macro does.

### Queries

`name`, `type`, the reconstructed `sql`, and `incomplete` — which is `true` when
some `MSysQueries` rows could not be read and the SQL may therefore be missing
clauses. The clause-level detail jetdb parses is not exposed by its API, so the
JSON records the reconstructed SQL rather than a clause tree.

Access stores form and report `RecordSource` SQL as hidden queries named
`~sq_…`; these typically outnumber real queries several times over, so they are
excluded unless `--include-system` is passed.

## `manifest.json`

The record of what happened, and the reason a partial extraction is still
useful:

```json
{
  "raccess_version": "0.1.0",
  "jetdb_version": "0.3",
  "extracted_at": "2026-10-24T22:17:01Z",
  "source": {
    "path": "sales.accdb",
    "format": "ACE12 (Access 2007)",
    "bytes": 479232,
    "sha256": "1566e947dcc41cba…",
    "code_page": 932,
    "lang_id": 1041
  },
  "objects": [
    { "kind": "table", "name": "tblOrders", "file": "tables/tblOrders.csv",
      "status": "ok", "rows": 5 },
    { "kind": "form", "name": "Odd Form", "status": "failed",
      "warnings": ["reading the design stream: unexpected end of stream"] }
  ]
}
```

Every object gets an entry. `status` is `ok`, `partial` or `failed`; `rows` is
the count the table definition declares, and `skipped_rows` appears when rows
could not be decoded and are therefore absent from the CSV.

A failed object never aborts the run. The exit code reflects the worst outcome
so a script can gate on it without parsing the manifest:

| Code | Meaning |
| --- | --- |
| 0 | everything extracted |
| 1 | at least one object is partial |
| 2 | at least one object failed |
| 3 | the run could not start — unreadable file, wrong password |
