use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const SOURCE: &str = include_str!("fixtures/yaml-test-suite/SOURCE.toml");
const MANIFEST: &str = include_str!("fixtures/yaml-test-suite/manifest.toml");
const COVERAGE: &str = include_str!("fixtures/yaml-test-suite/coverage.toml");
const FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/yaml-test-suite/data"
);

#[derive(Debug, Deserialize)]
struct SuiteSource {
    upstream: String,
    data_branch_commit: String,
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SuiteCoverage {
    schema: String,
    upstream: String,
    upstream_commit: String,
    upstream_case_count: usize,
    selected_case_count: usize,
    not_imported_case_count: usize,
    id_format: String,
    selection_policy: String,
    upstream_cases: Vec<String>,
    selected_cases: Vec<String>,
    not_imported_cases: Vec<String>,
    local_case_alias: Vec<LocalCaseAlias>,
}

#[derive(Debug, Deserialize)]
struct LocalCaseAlias {
    manifest_id: String,
    upstream_id: String,
    fixture_dir: String,
    input_sha256: String,
    reason: String,
}

#[test]
fn yaml_suite_coverage_matches_source_and_partitions_upstream() {
    let source = suite_source();
    let manifest = suite_manifest();
    let coverage = suite_coverage();

    assert_eq!(coverage.schema, "yaml-test-suite-coverage-v1");
    assert_eq!(coverage.upstream, source.upstream);
    assert_eq!(coverage.upstream_commit, source.data_branch_commit);
    assert_eq!(
        coverage.id_format,
        "Upstream slash IDs for directories containing in.yaml at the pinned data branch commit."
    );
    assert!(
        coverage.selection_policy.contains("not-imported cases"),
        "coverage selection policy must name remaining coverage debt",
    );

    let upstream = unique_set("upstream cases", &coverage.upstream_cases);
    let selected = unique_set("selected cases", &coverage.selected_cases);
    let not_imported = unique_set("not-imported cases", &coverage.not_imported_cases);
    assert_eq!(coverage.upstream_case_count, 402);
    assert_eq!(coverage.selected_case_count, 131);
    assert_eq!(coverage.not_imported_case_count, 271);
    assert_eq!(upstream.len(), coverage.upstream_case_count);
    assert_eq!(selected.len(), coverage.selected_case_count);
    assert_eq!(not_imported.len(), coverage.not_imported_case_count);
    assert!(
        selected.is_subset(&upstream),
        "selected YAML-suite cases must all be pinned upstream cases",
    );
    assert!(
        not_imported.is_subset(&upstream),
        "not-imported YAML-suite cases must all be pinned upstream cases",
    );
    assert!(
        selected.is_disjoint(&not_imported),
        "selected and not-imported YAML-suite coverage must not overlap",
    );
    let covered_partition = selected
        .union(&not_imported)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        covered_partition, upstream,
        "selected plus not-imported cases must cover the pinned upstream YAML test-suite",
    );

    let alias_by_manifest = alias_by_manifest_id(&coverage.local_case_alias);
    assert_eq!(alias_by_manifest.len(), 4);
    let canonical_manifest_ids =
        canonical_manifest_ids(&manifest.case, &upstream, &selected, &alias_by_manifest);
    assert_eq!(
        canonical_manifest_ids, selected,
        "coverage selected cases must be the canonical upstream IDs for every selected manifest case",
    );
}

fn canonical_manifest_ids(
    cases: &[SuiteCase],
    upstream: &BTreeSet<String>,
    selected: &BTreeSet<String>,
    alias_by_manifest: &BTreeMap<&str, &LocalCaseAlias>,
) -> BTreeSet<String> {
    let mut manifest_ids = BTreeSet::new();
    let mut canonical_ids = BTreeSet::new();
    for case in cases {
        assert!(
            manifest_ids.insert(case.id.as_str()),
            "duplicate selected YAML-suite manifest id {}",
            case.id,
        );
        if upstream.contains(&case.id) {
            canonical_ids.insert(case.id.clone());
            continue;
        }

        let alias = alias_by_manifest.get(case.id.as_str()).unwrap_or_else(|| {
            panic!(
                "missing coverage alias for selected manifest id {}",
                case.id
            )
        });
        assert!(upstream.contains(&alias.upstream_id));
        assert!(selected.contains(&alias.upstream_id));
        assert!(
            alias.reason.contains("Legacy local fixture id"),
            "alias {} must explain why the local id differs from upstream",
            alias.manifest_id,
        );
        assert_alias_fixture_hash(alias);
        canonical_ids.insert(alias.upstream_id.clone());
    }
    assert_eq!(manifest_ids.len(), 131);
    canonical_ids
}

fn assert_alias_fixture_hash(alias: &LocalCaseAlias) {
    let path = Path::new(FIXTURE_ROOT)
        .join(&alias.fixture_dir)
        .join("in.yaml");
    let bytes = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read aliased YAML-suite fixture {}: {error}",
            path.display()
        )
    });
    let actual = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(
        actual, alias.input_sha256,
        "aliased YAML-suite fixture {} must keep the mapped upstream input digest",
        alias.manifest_id,
    );
}

fn alias_by_manifest_id(aliases: &[LocalCaseAlias]) -> BTreeMap<&str, &LocalCaseAlias> {
    let mut by_id = BTreeMap::new();
    for alias in aliases {
        assert!(
            by_id.insert(alias.manifest_id.as_str(), alias).is_none(),
            "duplicate local YAML-suite alias {}",
            alias.manifest_id,
        );
    }
    by_id
}

fn unique_set(label: &str, values: &[String]) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for value in values {
        assert!(set.insert(value.clone()), "duplicate {label} entry {value}",);
    }
    set
}

fn suite_source() -> SuiteSource {
    toml::from_str(SOURCE).expect("YAML-suite SOURCE.toml parses")
}

fn suite_manifest() -> SuiteManifest {
    toml::from_str(MANIFEST).expect("YAML-suite manifest parses")
}

fn suite_coverage() -> SuiteCoverage {
    toml::from_str(COVERAGE).expect("YAML-suite coverage ledger parses")
}
