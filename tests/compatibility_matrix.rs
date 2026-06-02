#![allow(deprecated)]

use saphyr::LoadableYamlNode;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const COMPATIBILITY: &str = include_str!("../COMPATIBILITY.md");
const MATRIX_MANIFEST: &str = include_str!("fixtures/compatibility-matrix/manifest.toml");
const CROSS_ECOSYSTEM: &str =
    include_str!("fixtures/compatibility-matrix/cross-ecosystem-vectors.toml");
const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const REAL_WORLD_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");

#[derive(Debug, Deserialize)]
struct MatrixManifest {
    schema: String,
    render_start: String,
    render_end: String,
    row: Vec<MatrixRow>,
}

#[derive(Debug, Deserialize)]
struct MatrixRow {
    id: String,
    behavior: String,
    probe: String,
    proof: String,
    input: Option<String>,
    expected_docs: usize,
    yaml_policy: String,
    yaml: String,
    serde_yaml: String,
    serde_yml: String,
    serde_yaml_ng: String,
    yaml_rust2: String,
    saphyr: String,
    cross_ecosystem: Vec<String>,
    divergence_record: String,
    migration_impact: String,
}

#[derive(Debug, Deserialize)]
struct CrossEcosystemVectors {
    schema: String,
    capture_policy: String,
    vector: Vec<CrossVector>,
}

#[derive(Debug, Deserialize)]
struct CrossVector {
    row: String,
    implementation: String,
    version: String,
    status: String,
    provenance: String,
}

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    fixture: Vec<FixtureRecord>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    path: String,
    expected_docs: usize,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AppConfig {
    name: String,
    enabled: bool,
    ports: Vec<u16>,
}

#[test]
fn compatibility_matrix_rust_loaders_match_manifest() {
    let manifest = matrix_manifest();
    assert_eq!(manifest.schema, "compatibility-matrix-v1");
    assert_eq!(manifest.row.len(), 6);

    for row in &manifest.row {
        assert_known_status("yaml", &row.yaml);
        assert_known_status("serde_yaml", &row.serde_yaml);
        assert_known_status("serde_yml", &row.serde_yml);
        assert_known_status("serde_yaml_ng", &row.serde_yaml_ng);
        assert_known_status("yaml-rust2", &row.yaml_rust2);
        assert_known_status("saphyr", &row.saphyr);
        assert!(
            !row.yaml_policy.trim().is_empty(),
            "{} must document this crate's policy",
            row.id
        );
        assert!(
            !row.migration_impact.trim().is_empty(),
            "{} must document migration impact",
            row.id
        );

        match row.probe.as_str() {
            "typed-serde-config" => assert_typed_serde_row(row),
            "parse-documents" => assert_parse_documents_row(row),
            "real-world-registry" => assert_real_world_registry_row(row),
            other => panic!("unknown matrix probe {other} for {}", row.id),
        }
    }
}

#[test]
fn compatibility_matrix_cross_ecosystem_vectors_are_pinned() {
    let manifest = matrix_manifest();
    let vectors = cross_vectors();
    assert_eq!(vectors.schema, "cross-ecosystem-vectors-v1");
    assert!(
        vectors.capture_policy.contains("do not execute"),
        "cross-ecosystem vectors must stay offline-only"
    );

    let row_ids = manifest
        .row
        .iter()
        .map(|row| row.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for vector in &vectors.vector {
        assert!(
            row_ids.contains(vector.row.as_str()),
            "unknown vector row {}",
            vector.row
        );
        assert!(
            matches!(
                vector.status.as_str(),
                "accept" | "reject" | "value-diverges"
            ),
            "{} {} has unsupported status {}",
            vector.row,
            vector.implementation,
            vector.status
        );
        assert!(
            !vector.version.trim().is_empty(),
            "{} {} must pin a version",
            vector.row,
            vector.implementation
        );
        assert!(
            vector.provenance.contains("Offline spot-check vector"),
            "{} {} must record offline-vector provenance",
            vector.row,
            vector.implementation
        );
        assert!(
            seen.insert((vector.row.as_str(), vector.implementation.as_str())),
            "duplicate vector for {} {}",
            vector.row,
            vector.implementation
        );
    }

    for row in &manifest.row {
        for vector_row in &row.cross_ecosystem {
            for implementation in ["go-yaml", "PyYAML", "yaml-cpp"] {
                assert!(
                    seen.contains(&(vector_row.as_str(), implementation)),
                    "{} must have {implementation} vector",
                    row.id
                );
            }
        }
    }
}

#[test]
fn compatibility_matrix_block_in_docs_is_generated() {
    let manifest = matrix_manifest();
    let vectors = cross_vectors();
    let expected = format!(
        "{}\n{}\n{}",
        manifest.render_start,
        render_matrix(&manifest, &vectors),
        manifest.render_end
    );
    let actual = marked_block(
        COMPATIBILITY,
        manifest.render_start.as_str(),
        manifest.render_end.as_str(),
    );
    assert_eq!(actual, expected);
}

fn assert_typed_serde_row(row: &MatrixRow) {
    let input = row_input(row);
    let expected = AppConfig {
        name: "api".to_owned(),
        enabled: true,
        ports: vec![80, 443],
    };

    assert_status(row, "yaml", &row.yaml, || {
        yaml::from_str::<AppConfig>(input).map(|value| {
            assert_eq!(value, expected);
            row.expected_docs
        })
    });
    assert_status(row, "serde_yaml", &row.serde_yaml, || {
        serde_yaml::from_str::<AppConfig>(input).map(|value| {
            assert_eq!(value, expected);
            row.expected_docs
        })
    });
    assert_status(row, "serde_yml", &row.serde_yml, || {
        serde_yml::from_str::<AppConfig>(input).map(|value| {
            assert_eq!(value, expected);
            row.expected_docs
        })
    });
    assert_status(row, "serde_yaml_ng", &row.serde_yaml_ng, || {
        serde_yaml_ng::from_str::<AppConfig>(input).map(|value| {
            assert_eq!(value, expected);
            row.expected_docs
        })
    });
    assert_eq!(row.yaml_rust2, "n/a");
    assert_eq!(row.saphyr, "n/a");
}

fn assert_parse_documents_row(row: &MatrixRow) {
    let input = row_input(row);
    assert_status(row, "yaml", &row.yaml, || {
        yaml::parse_documents(input).map(|docs| docs.len())
    });
    assert_status(row, "serde_yaml", &row.serde_yaml, || {
        Ok::<_, serde_yaml::Error>(serde_yaml::Deserializer::from_str(input).count())
    });
    assert_status(row, "serde_yml", &row.serde_yml, || {
        serde_yml::from_str::<serde_yml::Value>(input).map(|_| 1)
    });
    assert_status(row, "serde_yaml_ng", &row.serde_yaml_ng, || {
        Ok::<_, serde_yaml_ng::Error>(serde_yaml_ng::Deserializer::from_str(input).count())
    });
    assert_status(row, "yaml-rust2", &row.yaml_rust2, || {
        yaml_rust2::YamlLoader::load_from_str(input).map(|docs| docs.len())
    });
    assert_status(row, "saphyr", &row.saphyr, || {
        saphyr::Yaml::load_from_str(input).map(|docs| docs.len())
    });
}

fn assert_real_world_registry_row(row: &MatrixRow) {
    let manifest: FixtureManifest =
        toml::from_str(REAL_WORLD_SOURCE).expect("real-world SOURCE.toml parses");
    assert_eq!(manifest.fixture.len(), 33);
    let total_docs = manifest
        .fixture
        .iter()
        .map(|fixture| fixture.expected_docs)
        .sum::<usize>();
    assert_eq!(total_docs, row.expected_docs);

    for fixture in &manifest.fixture {
        let input = fs::read_to_string(Path::new(REAL_WORLD_ROOT).join(&fixture.path))
            .unwrap_or_else(|error| panic!("read {}: {error}", fixture.path));
        assert_loader_accepts(
            row,
            "yaml",
            &row.yaml,
            fixture,
            yaml::parse_documents(&input).map(|docs| docs.len()),
        );
        assert_loader_accepts(
            row,
            "serde_yaml",
            &row.serde_yaml,
            fixture,
            Ok::<_, serde_yaml::Error>(serde_yaml::Deserializer::from_str(&input).count()),
        );
        if row.serde_yml != "n/a" {
            assert_loader_accepts(
                row,
                "serde_yml",
                &row.serde_yml,
                fixture,
                serde_yml::from_str::<serde_yml::Value>(&input).map(|_| fixture.expected_docs),
            );
        }
        assert_loader_accepts(
            row,
            "serde_yaml_ng",
            &row.serde_yaml_ng,
            fixture,
            Ok::<_, serde_yaml_ng::Error>(serde_yaml_ng::Deserializer::from_str(&input).count()),
        );
        assert_loader_accepts(
            row,
            "yaml-rust2",
            &row.yaml_rust2,
            fixture,
            yaml_rust2::YamlLoader::load_from_str(&input).map(|docs| docs.len()),
        );
        assert_loader_accepts(
            row,
            "saphyr",
            &row.saphyr,
            fixture,
            saphyr::Yaml::load_from_str(&input).map(|docs| docs.len()),
        );
    }
}

fn assert_status<E, F>(row: &MatrixRow, loader: &str, status: &str, run: F)
where
    E: std::fmt::Display,
    F: FnOnce() -> Result<usize, E>,
{
    if status == "n/a" {
        return;
    }
    match (status, run()) {
        ("accept", Ok(docs)) => assert_eq!(docs, row.expected_docs, "{loader} docs for {}", row.id),
        ("reject", Err(_)) => {}
        ("accept", Err(error)) => panic!("{loader} should accept {}: {error}", row.id),
        ("reject", Ok(_)) => panic!("{loader} should reject {}", row.id),
        (other, _) => panic!("unsupported status {other} for {loader} {}", row.id),
    }
}

fn assert_loader_accepts<E>(
    row: &MatrixRow,
    loader: &str,
    status: &str,
    fixture: &FixtureRecord,
    result: Result<usize, E>,
) where
    E: std::fmt::Display,
{
    match (status, result) {
        ("accept", Ok(docs)) => assert_eq!(
            docs, fixture.expected_docs,
            "{loader} docs for fixture {} in {}",
            fixture.path, row.id
        ),
        ("accept", Err(error)) => panic!(
            "{loader} should accept fixture {} in {}: {error}",
            fixture.path, row.id
        ),
        ("n/a", _) => {}
        (other, _) => panic!(
            "unsupported registry status {other} for {loader} {}",
            row.id
        ),
    }
}

fn assert_known_status(loader: &str, status: &str) {
    assert!(
        matches!(
            status,
            "accept" | "reject" | "value-diverges" | "event-diverges" | "n/a"
        ),
        "{loader} has unsupported matrix status {status}"
    );
}

fn row_input(row: &MatrixRow) -> &str {
    row.input
        .as_deref()
        .unwrap_or_else(|| panic!("{} must include input", row.id))
}

fn matrix_manifest() -> MatrixManifest {
    toml::from_str(MATRIX_MANIFEST).expect("compatibility matrix manifest parses")
}

fn cross_vectors() -> CrossEcosystemVectors {
    toml::from_str(CROSS_ECOSYSTEM).expect("cross-ecosystem vectors parse")
}

fn render_matrix(manifest: &MatrixManifest, vectors: &CrossEcosystemVectors) -> String {
    let by_row = vectors_by_row(vectors);
    let mut output = String::from(
        "| Behavior family | Proof source | `yaml` policy | `yaml` | `serde_yaml` | `serde_yml` | `serde_yaml_ng` | `yaml-rust2` | `saphyr` | Cross-ecosystem vector | Divergence / migration impact |\n\
         |---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for row in &manifest.row {
        let cross = if row.cross_ecosystem.is_empty() {
            "n/a".to_owned()
        } else {
            row.cross_ecosystem
                .iter()
                .flat_map(|row_id| by_row.get(row_id.as_str()).into_iter().flatten())
                .map(|vector| {
                    format!(
                        "{} {}: {}",
                        vector.implementation, vector.version, vector.status
                    )
                })
                .collect::<Vec<_>>()
                .join("<br>")
        };
        let divergence = if row.divergence_record == "none" {
            row.migration_impact.clone()
        } else {
            format!("{}; {}", row.divergence_record, row.migration_impact)
        };
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            cell(&row.behavior),
            cell(&row.proof),
            cell(&row.yaml_policy),
            cell(&row.yaml),
            cell(&row.serde_yaml),
            cell(&row.serde_yml),
            cell(&row.serde_yaml_ng),
            cell(&row.yaml_rust2),
            cell(&row.saphyr),
            cell(&cross),
            cell(&divergence),
        ));
    }
    output.trim_end().to_owned()
}

fn vectors_by_row(vectors: &CrossEcosystemVectors) -> BTreeMap<&str, Vec<&CrossVector>> {
    let mut by_row: BTreeMap<&str, Vec<&CrossVector>> = BTreeMap::new();
    for vector in &vectors.vector {
        by_row.entry(vector.row.as_str()).or_default().push(vector);
    }
    by_row
}

fn cell(value: &str) -> String {
    value.replace('\n', " ").replace('|', "\\|")
}

fn marked_block(source: &str, start: &str, end: &str) -> String {
    let start_index = source
        .find(start)
        .unwrap_or_else(|| panic!("missing marker {start}"));
    let after_start = start_index + start.len();
    let end_offset = source[after_start..]
        .find(end)
        .unwrap_or_else(|| panic!("missing marker {end}"));
    source[start_index..after_start + end_offset + end.len()]
        .trim()
        .to_owned()
}
