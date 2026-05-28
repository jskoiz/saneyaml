use std::fs;
use std::path::{Path, PathBuf};

const EXPECTED_RECORDS: usize = 34;
const REQUIRED_FIELDS: &[&str] = &["case", "policy", "prototype", "decision"];
const REFERENCE_FIELDS: &[&str] = &[
    "serde_yaml",
    "serde_yaml_libyaml",
    "yaml_rust2",
    "saphyr",
    "yaml_rust2_saphyr",
    "saphyr_yaml_rust2",
    "libyaml",
    "legacy_yaml_1_1",
    "raw_event_decision",
];
const EVIDENCE_SOURCES: &[&str] = &[
    include_str!("divergences.rs"),
    include_str!("compatibility_harness.rs"),
    include_str!("event_policy.rs"),
    include_str!("serde_value_api.rs"),
    include_str!("yaml_test_suite.rs"),
    include_str!("../COMPATIBILITY.md"),
];

#[test]
fn divergence_records_have_uniform_registry_fields_and_test_links() {
    let paths = record_paths();
    assert_eq!(
        paths.len(),
        EXPECTED_RECORDS,
        "new divergence records must update the registry count and schema gate",
    );

    for path in paths {
        let record = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let parsed: toml::Value = toml::from_str(&record)
            .unwrap_or_else(|error| panic!("parse {} as TOML: {error}", path.display()));
        let table = parsed
            .as_table()
            .unwrap_or_else(|| panic!("{} must be a TOML table", path.display()));

        for field in REQUIRED_FIELDS {
            required_string(table, field, &path);
        }

        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("record path has UTF-8 filename: {}", path.display()));
        let stem = filename
            .strip_suffix(".toml")
            .unwrap_or_else(|| panic!("record file must end in .toml: {}", path.display()));
        let case = required_string(table, "case", &path);
        assert_eq!(
            case,
            stem,
            "{} case must match filename stem",
            path.display(),
        );

        assert!(
            REFERENCE_FIELDS
                .iter()
                .any(|field| nonempty_string(table, field).is_some()),
            "{} must record at least one reference/evidence field",
            path.display(),
        );
        assert!(
            record_has_external_test_or_doc_reference(filename, case),
            "{} must be referenced by tests/docs outside the record registry",
            path.display(),
        );
    }
}

fn record_paths() -> Vec<PathBuf> {
    let record_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("divergences")
        .join("records");
    let mut paths = Vec::new();
    for entry in
        fs::read_dir(&record_dir).unwrap_or_else(|error| panic!("read {record_dir:?}: {error}"))
    {
        let path = entry
            .unwrap_or_else(|error| panic!("read entry in {record_dir:?}: {error}"))
            .path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            paths.push(path);
        }
    }
    paths.sort();
    paths
}

fn required_string<'a>(table: &'a toml::Table, field: &str, path: &Path) -> &'a str {
    nonempty_string(table, field).unwrap_or_else(|| {
        panic!(
            "{} must have non-empty string field {field}",
            path.display()
        )
    })
}

fn nonempty_string<'a>(table: &'a toml::Table, field: &str) -> Option<&'a str> {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn record_has_external_test_or_doc_reference(filename: &str, case: &str) -> bool {
    let path_fragment = format!("records/{filename}");
    EVIDENCE_SOURCES
        .iter()
        .any(|source| source.contains(&path_fragment) || source.contains(case))
}
