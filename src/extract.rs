//! Walks every object in a database and writes the output directory.
//!
//! One object failing never aborts the run: each failure is recorded in the
//! manifest and the walk continues.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jetdb::{
    BinaryFormat, CatalogEntry, CsvOptions, FormObjectType, ObjectType, PageReader, VbaModuleType,
    list_forms, list_macros, read_catalog, read_form_properties, read_macro, read_queries_in,
    read_relationships_in, read_table_def, read_vba_project, write_csv,
};

use crate::model::{
    Form, Macro, Manifest, ObjectRecord, Query, Schema, SchemaRelationship, SchemaTable, Source,
    Status,
};

pub struct Options {
    pub include_system: bool,
    pub null_as: String,
}

pub fn run(
    reader: &mut PageReader,
    database: &Path,
    out: &Path,
    opts: &Options,
) -> Result<Manifest> {
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    let catalog = read_catalog(reader).context("reading the system catalog (MSysObjects)")?;
    let mut objects = Vec::new();

    write_schema(reader, out, &catalog, opts, &mut objects)?;
    write_tables(reader, out, &catalog, opts, &mut objects)?;
    write_queries(reader, out, &catalog, opts, &mut objects)?;
    write_forms(reader, out, &mut objects);
    write_macros(reader, out, &mut objects);
    write_vba(reader, out, &mut objects);

    let manifest = Manifest {
        raccess_version: env!("CARGO_PKG_VERSION"),
        jetdb_version: jetdb_version(),
        extracted_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        source: source_info(reader, database)?,
        objects,
    };
    write_json(&out.join("manifest.json"), &manifest)?;
    Ok(manifest)
}

fn jetdb_version() -> &'static str {
    // Recorded in the manifest so an export can be traced to the reader that
    // produced it; bump alongside the Cargo.toml dependency.
    "0.3"
}

fn source_info(reader: &PageReader, database: &Path) -> Result<Source> {
    use sha2::{Digest, Sha256};

    let bytes = fs::read(database).with_context(|| format!("reading {}", database.display()))?;
    let header = reader.header();
    Ok(Source {
        path: database.display().to_string(),
        format: header.version.to_string(),
        bytes: bytes.len() as u64,
        sha256: crate::model::hex(&Sha256::digest(&bytes)),
        code_page: header.code_page,
        lang_id: header.lang_id,
    })
}

// ---------------------------------------------------------------------------
// schema.json
// ---------------------------------------------------------------------------

fn write_schema(
    reader: &mut PageReader,
    out: &Path,
    catalog: &[CatalogEntry],
    opts: &Options,
    objects: &mut Vec<ObjectRecord>,
) -> Result<()> {
    let mut tables = Vec::new();
    for entry in tables_to_export(catalog, opts) {
        match read_table_def(reader, &entry.name, entry.table_page) {
            Ok(def) => tables.push(SchemaTable::new(&def, entry.is_system())),
            Err(err) => objects.push(ObjectRecord::failed(
                "table",
                &entry.name,
                format!("reading the table definition: {err}"),
            )),
        }
    }

    let relationships = match read_relationships_in(reader, catalog) {
        Ok(rels) => rels.iter().map(SchemaRelationship::new).collect(),
        Err(err) => {
            objects.push(ObjectRecord::failed(
                "relationships",
                "relationships",
                format!("reading MSysRelationships: {err}"),
            ));
            Vec::new()
        }
    };

    let schema = Schema {
        tables,
        relationships,
    };
    write_json(&out.join("schema.json"), &schema)?;
    objects.push(ObjectRecord::ok("schema", "schema", "schema.json"));
    Ok(())
}

// ---------------------------------------------------------------------------
// tables/
// ---------------------------------------------------------------------------

fn tables_to_export<'a>(catalog: &'a [CatalogEntry], opts: &Options) -> Vec<&'a CatalogEntry> {
    catalog
        .iter()
        .filter(|e| e.object_type == ObjectType::Table)
        .filter(|e| opts.include_system || !e.is_system_or_hidden())
        .collect()
}

fn write_tables(
    reader: &mut PageReader,
    out: &Path,
    catalog: &[CatalogEntry],
    opts: &Options,
    objects: &mut Vec<ObjectRecord>,
) -> Result<()> {
    let entries = tables_to_export(catalog, opts);
    if entries.is_empty() {
        return Ok(());
    }

    let dir = out.join("tables");
    fs::create_dir_all(&dir)?;
    let blobs = out.join("blobs");
    let mut namer = Namer::default();

    let csv_options = CsvOptions {
        binary: BinaryFormat::Hex,
        null: opts.null_as.clone(),
        boolean_words: true,
        system_columns: opts.include_system,
        ..CsvOptions::default()
    };

    for entry in entries {
        let def = match read_table_def(reader, &entry.name, entry.table_page) {
            Ok(def) => def,
            // Already recorded by write_schema.
            Err(_) => continue,
        };

        let file = namer.file(&entry.name, "csv");
        let path = dir.join(&file);
        let mut sink = Vec::new();

        let blobs_dir = blobs.clone();
        let mut written: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        let mut attachments = |_row: i32, name: &str, data: &[u8]| -> io::Result<String> {
            let blob_name = sanitize(name);
            written.push((blobs_dir.join(&blob_name), data.to_vec()));
            Ok(format!("blobs/{blob_name}"))
        };

        let result = write_csv(
            reader,
            catalog,
            &def,
            &csv_options,
            &mut sink,
            Some(&mut attachments),
        );

        match result {
            // write_csv returns the number of rows it had to skip, not the
            // number it wrote.
            Ok(skipped) => {
                if !written.is_empty() {
                    fs::create_dir_all(&blobs)?;
                    for (path, data) in &written {
                        fs::write(path, data)?;
                    }
                }
                fs::write(&path, &sink).with_context(|| format!("writing {}", path.display()))?;
                let mut record = ObjectRecord::ok("table", &entry.name, format!("tables/{file}"))
                    .with_rows(def.num_rows as usize, skipped);
                if let Some(byte) = stray_control_byte(&sink) {
                    record = record.warn(format!(
                        "a text value contains control byte {byte:#04x}, written through verbatim; \
                         strict CSV readers will reject this file"
                    ));
                }
                objects.push(record);
            }
            Err(err) => objects.push(ObjectRecord::failed(
                "table",
                &entry.name,
                format!("exporting rows: {err}"),
            )),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// queries/
// ---------------------------------------------------------------------------

fn write_queries(
    reader: &mut PageReader,
    out: &Path,
    catalog: &[CatalogEntry],
    opts: &Options,
    objects: &mut Vec<ObjectRecord>,
) -> Result<()> {
    let queries = match read_queries_in(reader, catalog) {
        Ok(queries) => queries,
        Err(err) => {
            objects.push(ObjectRecord::failed(
                "query",
                "queries",
                format!("reading MSysQueries: {err}"),
            ));
            return Ok(());
        }
    };

    // Access stores form and report RecordSource SQL as hidden `~sq_` queries;
    // they vastly outnumber user queries, so they are opt-in.
    let visible: Vec<_> = queries
        .iter()
        .filter(|q| opts.include_system || !q.name.starts_with('~'))
        .collect();
    if visible.is_empty() {
        return Ok(());
    }

    let dir = out.join("queries");
    fs::create_dir_all(&dir)?;
    let mut namer = Namer::default();

    for query in visible {
        let model = Query::new(query);
        let stem = namer.stem(&query.name);
        write_json(&dir.join(format!("{stem}.json")), &model)?;
        fs::write(dir.join(format!("{stem}.sql")), &model.sql)?;

        let mut record = ObjectRecord::ok("query", &query.name, format!("queries/{stem}.json"));
        if query.incomplete {
            record = record.warn("some MSysQueries rows were unreadable; SQL may be incomplete");
        }
        objects.push(record);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// forms/ and reports/
// ---------------------------------------------------------------------------

fn write_forms(reader: &mut PageReader, out: &Path, objects: &mut Vec<ObjectRecord>) {
    let entries = match list_forms(reader) {
        Ok(entries) => entries,
        // A database with no form storage at all is the normal case, not a failure.
        Err(_) => return,
    };

    let mut form_namer = Namer::default();
    let mut report_namer = Namer::default();

    for entry in entries {
        let (kind, subdir, namer) = match entry.object_type {
            FormObjectType::Form => ("form", "forms", &mut form_namer),
            FormObjectType::Report => ("report", "reports", &mut report_namer),
        };

        match read_form_properties(reader, &entry.name) {
            Ok(props) => {
                let dir = out.join(subdir);
                if let Err(err) = fs::create_dir_all(&dir) {
                    objects.push(ObjectRecord::failed(kind, &entry.name, err.to_string()));
                    continue;
                }
                let file = namer.file(&entry.name, "json");
                match write_json(&dir.join(&file), &Form::new(&props)) {
                    Ok(()) => objects.push(ObjectRecord::ok(
                        kind,
                        &entry.name,
                        format!("{subdir}/{file}"),
                    )),
                    Err(err) => {
                        objects.push(ObjectRecord::failed(kind, &entry.name, err.to_string()))
                    }
                }
            }
            Err(err) => objects.push(ObjectRecord::failed(
                kind,
                &entry.name,
                format!("reading the design stream: {err}"),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// macros/
// ---------------------------------------------------------------------------

fn write_macros(reader: &mut PageReader, out: &Path, objects: &mut Vec<ObjectRecord>) {
    let entries = match list_macros(reader) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    if entries.is_empty() {
        return;
    }

    let dir = out.join("macros");
    if let Err(err) = fs::create_dir_all(&dir) {
        objects.push(ObjectRecord::failed("macro", "macros", err.to_string()));
        return;
    }
    let mut namer = Namer::default();

    for entry in entries {
        match read_macro(reader, &entry.name) {
            Ok(def) => {
                let file = namer.file(&entry.name, "json");
                match write_json(&dir.join(&file), &Macro::new(&entry.name, &def)) {
                    Ok(()) => objects.push(ObjectRecord::ok(
                        "macro",
                        &entry.name,
                        format!("macros/{file}"),
                    )),
                    Err(err) => {
                        objects.push(ObjectRecord::failed("macro", &entry.name, err.to_string()))
                    }
                }
            }
            Err(err) => objects.push(ObjectRecord::failed(
                "macro",
                &entry.name,
                format!("reading the macro definition: {err}"),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// modules/
// ---------------------------------------------------------------------------

fn write_vba(reader: &mut PageReader, out: &Path, objects: &mut Vec<ObjectRecord>) {
    let project = match read_vba_project(reader) {
        Ok(project) => project,
        Err(_) => return,
    };
    if project.modules.is_empty() {
        return;
    }

    let dir = out.join("modules");
    if let Err(err) = fs::create_dir_all(&dir) {
        objects.push(ObjectRecord::failed("module", "modules", err.to_string()));
        return;
    }
    let mut namer = Namer::default();

    for module in &project.modules {
        let extension = match module.module_type {
            VbaModuleType::Standard => "bas",
            VbaModuleType::Class | VbaModuleType::Document => "cls",
        };
        let file = namer.file(&module.name, extension);
        match fs::write(dir.join(&file), &module.source) {
            Ok(()) => objects.push(ObjectRecord::ok(
                "module",
                &module.name,
                format!("modules/{file}"),
            )),
            Err(err) => objects.push(ObjectRecord::failed(
                "module",
                &module.name,
                err.to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut json = serde_json::to_vec_pretty(value)?;
    json.push(b'\n');
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// The first C0 control byte a CSV reader will not accept, if any.
///
/// Tab, LF and CR are legal inside a quoted field; the rest are not, and Access
/// text columns really do hold them.
fn stray_control_byte(csv: &[u8]) -> Option<u8> {
    csv.iter()
        .copied()
        .find(|&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'))
}

/// Maps Access object names onto filenames, keeping them unique.
#[derive(Default)]
struct Namer {
    used: HashSet<String>,
}

impl Namer {
    fn stem(&mut self, name: &str) -> String {
        let base = sanitize(name);
        let mut candidate = base.clone();
        let mut n = 2;
        // Case-insensitive, so exports stay correct on Windows and macOS.
        while !self.used.insert(candidate.to_lowercase()) {
            candidate = format!("{base}~{n}");
            n += 1;
        }
        candidate
    }

    fn file(&mut self, name: &str, extension: &str) -> String {
        format!("{}.{extension}", self.stem(name))
    }
}

/// Percent-encodes whatever a filesystem will not take, keeping non-ASCII
/// names (Japanese form names are common) intact.
fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '%' => {
                let mut buf = [0u8; 4];
                for b in ch.encode_utf8(&mut buf).as_bytes() {
                    let _ = write!(out, "%{b:02X}");
                }
            }
            c if c.is_control() => {
                let _ = write!(out, "%{:02X}", c as u32 & 0xFF);
            }
            c => out.push(c),
        }
    }

    let trimmed = out.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "_".to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn worst_status(objects: &[ObjectRecord]) -> Status {
    if objects.iter().any(|o| o.status == Status::Failed) {
        Status::Failed
    } else if objects.iter().any(|o| o.status == Status::Partial) {
        Status::Partial
    } else {
        Status::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_encodes_path_separators() {
        assert_eq!(sanitize("Orders/2024"), "Orders%2F2024");
        assert_eq!(sanitize("a:b*c?"), "a%3Ab%2Ac%3F");
    }

    #[test]
    fn sanitize_keeps_unicode_and_spaces() {
        assert_eq!(sanitize("Order Details"), "Order Details");
        assert_eq!(sanitize("日本語フォーム"), "日本語フォーム");
    }

    #[test]
    fn sanitize_rejects_empty_and_trailing_dots() {
        assert_eq!(sanitize("   "), "_");
        assert_eq!(sanitize("report."), "report");
    }

    #[test]
    fn stray_control_byte_allows_tab_newline_and_cr() {
        assert_eq!(stray_control_byte(b"a,b\n\"x\ty\"\r\n"), None);
    }

    #[test]
    fn stray_control_byte_finds_nul_and_vertical_tab() {
        assert_eq!(stray_control_byte(b"a,\"A\x00A\"\n"), Some(0x00));
        assert_eq!(stray_control_byte(b"a,\"A\x0bA\"\n"), Some(0x0b));
    }

    #[test]
    fn namer_disambiguates_case_insensitive_collisions() {
        let mut namer = Namer::default();
        assert_eq!(namer.file("Orders", "csv"), "Orders.csv");
        assert_eq!(namer.file("orders", "csv"), "orders~2.csv");
        assert_eq!(namer.file("Orders/x", "csv"), "Orders%2Fx.csv");
    }
}
