use saphyr::LoadableYamlNode;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");
const SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    fixture: Vec<FixtureRecord>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    path: String,
    domain: String,
    source_type: String,
    source: String,
    version: String,
    license: String,
    reduction: String,
    expected_docs: usize,
    gates: Vec<String>,
}

#[test]
fn real_world_fixture_manifest_covers_files_counts_and_reference_gates() {
    let manifest: FixtureManifest =
        toml::from_str(SOURCE).expect("real-world fixture source manifest parses");
    assert_eq!(manifest.fixture.len(), 19);

    let root = Path::new(FIXTURE_ROOT);
    let manifest_paths: BTreeSet<_> = manifest
        .fixture
        .iter()
        .map(|fixture| fixture.path.clone())
        .collect();
    let actual_paths = yaml_fixture_paths(root);
    assert_eq!(manifest_paths, actual_paths);

    let mut domain_counts = BTreeMap::new();
    let mut total_docs = 0usize;
    for fixture in &manifest.fixture {
        assert_metadata_is_complete(fixture);

        let input = fs::read_to_string(root.join(&fixture.path))
            .unwrap_or_else(|error| panic!("read real-world fixture {}: {error}", fixture.path));
        let docs = yaml::parse_documents(&input)
            .unwrap_or_else(|error| panic!("parse real-world fixture {}: {error}", fixture.path));
        assert_eq!(
            docs.len(),
            fixture.expected_docs,
            "document count drifted for {}",
            fixture.path
        );
        total_docs += docs.len();
        *domain_counts
            .entry(fixture.domain.as_str())
            .or_insert(0usize) += 1;

        assert_shared_reference_acceptance(fixture, &input);
    }

    assert_eq!(total_docs, 25);
    assert_eq!(
        domain_counts,
        BTreeMap::from([
            ("ansible", 2),
            ("docker-compose", 4),
            ("github-actions", 3),
            ("helm", 2),
            ("kubernetes", 5),
            ("openapi", 2),
            ("wrangler", 1),
        ])
    );
}

fn assert_metadata_is_complete(fixture: &FixtureRecord) {
    assert!(
        matches!(
            fixture.source_type.as_str(),
            "synthetic" | "adapted" | "upstream-snapshot"
        ),
        "{} records unsupported source_type {}",
        fixture.path,
        fixture.source_type
    );
    for (name, value) in [
        ("domain", &fixture.domain),
        ("source", &fixture.source),
        ("version", &fixture.version),
        ("license", &fixture.license),
        ("reduction", &fixture.reduction),
    ] {
        assert!(
            !value.trim().is_empty(),
            "{} must record non-empty {name}",
            fixture.path
        );
    }

    let gates: BTreeSet<_> = fixture.gates.iter().map(String::as_str).collect();
    for required in [
        "typed-config",
        "event-parity",
        "tree-parity",
        "parser-properties",
        "shared-reference-acceptance",
    ] {
        assert!(
            gates.contains(required),
            "{} must record {required} gate coverage",
            fixture.path
        );
    }
}

fn assert_shared_reference_acceptance(fixture: &FixtureRecord, input: &str) {
    let serde_docs = serde_yaml::Deserializer::from_str(input).count();
    assert_eq!(
        serde_docs, fixture.expected_docs,
        "serde_yaml document count for {}",
        fixture.path
    );

    let yaml_rust_docs = yaml_rust2::YamlLoader::load_from_str(input)
        .unwrap_or_else(|error| panic!("yaml-rust2 parses {}: {error}", fixture.path));
    assert_eq!(
        yaml_rust_docs.len(),
        fixture.expected_docs,
        "yaml-rust2 document count for {}",
        fixture.path
    );

    let saphyr_docs = saphyr::Yaml::load_from_str(input)
        .unwrap_or_else(|error| panic!("saphyr parses {}: {error}", fixture.path));
    assert_eq!(
        saphyr_docs.len(),
        fixture.expected_docs,
        "saphyr document count for {}",
        fixture.path
    );
}

fn yaml_fixture_paths(root: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    collect_yaml_fixture_paths(root, root, &mut paths);
    paths
}

fn collect_yaml_fixture_paths(root: &Path, dir: &Path, paths: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let entry =
            entry.unwrap_or_else(|error| panic!("read entry in {}: {error}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_fixture_paths(root, &path, paths);
        } else if is_yaml(&path) {
            let relative = path.strip_prefix(root).unwrap_or_else(|error| {
                panic!("strip fixture root from {}: {error}", path.display())
            });
            paths.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn is_yaml(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("yaml")
}
