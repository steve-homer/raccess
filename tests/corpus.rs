//! Runs the extractor against the vendored corpora.
//!
//! Two are used, because they cover different things: the Jackcess suite has
//! every format version, and jetdb's has the forms, macros and VBA that
//! Jackcess cannot read at all. Both skip when absent, so a fresh clone
//! passes; see `fixtures/README.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// `fixtures/<corpus>`, or `None` when it has not been fetched.
fn corpus(name: &str) -> Option<PathBuf> {
    let dir = fixtures().join(name);
    dir.is_dir().then_some(dir)
}

fn skip(what: &str) {
    eprintln!("skipping {what}: run ./scripts/fetch-fixtures.sh to populate the corpora");
}

fn databases_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("readable fixture entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("mdb") || e.eq_ignore_ascii_case("accdb"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn out_dir(label: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(label)
}

/// Extracts `database`, returning the parsed manifest.
fn extract(database: &Path, out: &Path, password: Option<&str>) -> Result<Value, String> {
    let _ = std::fs::remove_dir_all(out);

    let mut command = Command::new(env!("CARGO_BIN_EXE_raccess"));
    command.arg("extract").arg(database).arg("-o").arg(out);
    if let Some(password) = password {
        command.arg("--password").arg(password);
    }

    let result = command.output().map_err(|e| format!("spawning: {e}"))?;

    // 0 = ok, 1 = partial, 2 = an object failed. Anything else did not run.
    let code = result.status.code().unwrap_or(-1);
    if !(0..=2).contains(&code) {
        return Err(format!(
            "exit {code}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }

    let text = std::fs::read_to_string(out.join("manifest.json"))
        .map_err(|e| format!("reading manifest.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parsing manifest.json: {e}"))
}

/// How many objects of each `kind` the manifest records.
fn kinds(manifest: &Value) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for object in manifest["objects"].as_array().expect("objects array") {
        let kind = object["kind"].as_str().expect("object kind");
        *counts.entry(kind.to_owned()).or_insert(0) += 1;
    }
    counts
}

// ---------------------------------------------------------------------------
// Format version coverage
// ---------------------------------------------------------------------------

/// Directory name -> the marker the detected format string must contain.
const VERSION_DIRS: &[(&str, &str)] = &[
    ("V1997", "Jet3"),
    ("V2000", "Jet4"),
    ("V2003", "Jet4"),
    ("V2007", "ACE12"),
    ("V2010", "ACE14"),
    ("V2016", "ACE16"),
    ("V2019", "ACE17"),
];

#[test]
fn extracts_every_format_version() {
    let roots: Vec<PathBuf> = ["jackcess", "jetdb"]
        .iter()
        .filter_map(|c| corpus(c))
        .collect();
    if roots.is_empty() {
        skip("extracts_every_format_version");
        return;
    }

    let mut failures = Vec::new();
    let mut covered = Vec::new();

    for (dir, want) in VERSION_DIRS {
        // Smallest database in that directory, from whichever corpus has one.
        let mut candidates: Vec<PathBuf> = roots
            .iter()
            .flat_map(|root| databases_in(&root.join(dir)))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX));
        let database = &candidates[0];
        let name = database.file_name().unwrap().to_string_lossy().into_owned();
        covered.push(*dir);

        match extract(database, &out_dir(&format!("version-{dir}")), None) {
            Ok(manifest) => {
                let format = manifest["source"]["format"].as_str().unwrap_or("");
                if !format.contains(want) {
                    failures.push(format!("{name}: expected format {want}, got {format:?}"));
                }
                if manifest["source"]["sha256"].as_str().unwrap_or("").len() != 64 {
                    failures.push(format!("{name}: manifest is missing the source checksum"));
                }
                if kinds(&manifest).get("table").copied().unwrap_or(0) == 0 {
                    failures.push(format!("{name}: no tables exported"));
                }
            }
            Err(err) => failures.push(format!("{name}: {err}")),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(
        covered.len(),
        VERSION_DIRS.len(),
        "only covered {covered:?}; both corpora together should supply all of {:?}",
        VERSION_DIRS.iter().map(|(d, _)| *d).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Forms, macros and VBA — only the jetdb corpus has these
// ---------------------------------------------------------------------------

/// `(file, minimum objects of each kind it must yield)`.
///
/// Minimums rather than exact counts, so an upstream fixture gaining an object
/// does not fail the build. Measured against the pinned corpus revision.
const OBJECT_FIXTURES: &[(&str, &[(&str, usize)])] = &[
    (
        "jetdb-sample.accdb",
        &[
            ("table", 3),
            ("query", 1),
            ("form", 1),
            ("macro", 1),
            ("module", 2),
        ],
    ),
    (
        "formPropTest.accdb",
        &[("form", 4), ("macro", 1), ("module", 3), ("query", 2)],
    ),
    ("vbaV2007.accdb", &[("form", 1), ("module", 3)]),
    ("vbaV2003.mdb", &[("form", 1), ("module", 3)]),
    // Jet RC4 encrypted, and readable with no password at all.
    ("enc_vbaV2003.mdb", &[("form", 1), ("module", 3)]),
];

#[test]
fn extracts_forms_macros_and_vba() {
    let Some(root) = corpus("jetdb") else {
        skip("extracts_forms_macros_and_vba");
        return;
    };

    let mut failures = Vec::new();
    for (file, expected) in OBJECT_FIXTURES {
        let database = root.join(file);
        if !database.is_file() {
            failures.push(format!("{file}: not in the corpus"));
            continue;
        }

        let out = out_dir(&format!("objects-{file}"));
        match extract(&database, &out, None) {
            Ok(manifest) => {
                let counts = kinds(&manifest);
                for (kind, least) in *expected {
                    let got = counts.get(*kind).copied().unwrap_or(0);
                    if got < *least {
                        failures.push(format!("{file}: expected >={least} {kind}, got {got}"));
                    }
                }
                // Every recorded file must actually be on disk.
                for object in manifest["objects"].as_array().unwrap() {
                    if let Some(path) = object["file"].as_str()
                        && !out.join(path).exists()
                    {
                        failures.push(format!("{file}: manifest lists {path}, which is missing"));
                    }
                }
            }
            Err(err) => failures.push(format!("{file}: {err}")),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn extracted_vba_has_real_source() {
    let Some(root) = corpus("jetdb") else {
        skip("extracted_vba_has_real_source");
        return;
    };

    let out = out_dir("vba-source");
    let manifest = extract(&root.join("jetdb-sample.accdb"), &out, None).expect("extracts");

    let modules: Vec<&Value> = manifest["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["kind"] == "module")
        .collect();
    assert!(!modules.is_empty(), "no VBA modules extracted");

    for module in modules {
        let path = out.join(module["file"].as_str().expect("module file"));
        let source = std::fs::read_to_string(&path).expect("readable module");
        assert!(
            source.contains("Attribute VB_Name"),
            "{} does not look like VBA source:\n{}",
            path.display(),
            source.chars().take(200).collect::<String>()
        );
    }
}

#[test]
fn extracts_non_ascii_object_names() {
    let Some(root) = corpus("jetdb") else {
        skip("extracts_non_ascii_object_names");
        return;
    };
    let database = root.join("vbaJapaneseV2007.accdb");
    if !database.is_file() {
        skip("extracts_non_ascii_object_names");
        return;
    }

    let out = out_dir("non-ascii");
    let manifest = extract(&database, &out, None).expect("extracts");

    let non_ascii: Vec<&str> = manifest["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["name"].as_str())
        .filter(|n| !n.is_ascii())
        .collect();
    assert!(
        !non_ascii.is_empty(),
        "expected non-ASCII object names, got {:?}",
        manifest["objects"]
    );

    // The name must survive into a real file, not be mangled or percent-escaped.
    for object in manifest["objects"].as_array().unwrap() {
        if let Some(path) = object["file"].as_str() {
            assert!(
                out.join(path).exists(),
                "manifest lists {path}, which is missing"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Encryption
// ---------------------------------------------------------------------------

#[test]
fn extracts_password_protected_database() {
    let Some(root) = corpus("jetdb") else {
        skip("extracts_password_protected_database");
        return;
    };
    let database = root.join("db2007-enc.accdb");
    if !database.is_file() {
        skip("extracts_password_protected_database");
        return;
    }

    let manifest = extract(&database, &out_dir("enc-ok"), Some("Test123"))
        .expect("extracts with the right password");
    assert!(
        kinds(&manifest).get("table").copied().unwrap_or(0) > 0,
        "no tables exported from the encrypted database"
    );

    let err = extract(&database, &out_dir("enc-bad"), Some("wrong"))
        .expect_err("a wrong password must fail");
    assert!(err.starts_with("exit 3"), "unexpected failure: {err}");

    let err =
        extract(&database, &out_dir("enc-none"), None).expect_err("a missing password must fail");
    assert!(err.starts_with("exit 3"), "unexpected failure: {err}");
}

// ---------------------------------------------------------------------------
// Negative cases, and the full sweep
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_file_that_is_not_a_database() {
    let not_a_database = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let err = extract(&not_a_database, &out_dir("not-a-database"), None)
        .expect_err("should refuse a TOML file");
    assert!(err.starts_with("exit 3"), "unexpected failure: {err}");
}

/// Databases that cannot open without a password, and so are expected to be
/// skipped by the sweep. Named explicitly rather than matched on the error
/// text, so a genuine failure that happens to mention a password cannot hide.
/// `extracts_password_protected_database` covers the password path itself.
const NEEDS_PASSWORD: &[&str] = &[
    "db-nonstandard-aes.accdb",
    "db2007-enc.accdb",
    "db2007-rc4cryptoapi.accdb",
    "db2013-enc.accdb",
    "enc_vbaV2007.accdb",
];

/// Every database in both corpora. Slow, so opt in with
/// `cargo test -- --ignored --nocapture`.
#[test]
#[ignore = "slow: extracts the whole corpus"]
fn sweeps_every_database() {
    let roots: Vec<PathBuf> = ["jackcess", "jetdb"]
        .iter()
        .filter_map(|c| corpus(c))
        .collect();
    if roots.is_empty() {
        skip("sweeps_every_database");
        return;
    }

    let databases: Vec<PathBuf> = roots.iter().flat_map(|r| databases_in(r)).collect();
    assert!(!databases.is_empty(), "corpora are empty");

    let out = out_dir("sweep");
    let mut failures = Vec::new();
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();

    let mut skipped = Vec::new();
    for database in &databases {
        let name = database.file_name().unwrap().to_string_lossy().into_owned();
        if NEEDS_PASSWORD.contains(&name.as_str()) {
            skipped.push(name);
            continue;
        }
        match extract(database, &out, None) {
            Ok(manifest) => {
                for (kind, n) in kinds(&manifest) {
                    *totals.entry(kind).or_insert(0) += n;
                }
            }
            Err(err) => failures.push(format!("{name}: {err}")),
        }
    }

    eprintln!(
        "swept {} databases ({} skipped as password-protected): {}",
        databases.len() - skipped.len(),
        skipped.len(),
        totals
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    assert!(
        failures.is_empty(),
        "{} of {} failed:\n{}",
        failures.len(),
        databases.len(),
        failures.join("\n")
    );
    // Every object kind must appear somewhere, or a whole code path is dark.
    for kind in ["table", "query", "form", "report", "macro", "module"] {
        assert!(
            totals.get(kind).copied().unwrap_or(0) > 0,
            "the sweep produced no {kind} objects at all"
        );
    }
}
