use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const EVENT_PARITY_SOURCE: &str = include_str!("event_parity.rs");
const TREE_PARITY_SOURCE: &str = include_str!("tree_parity.rs");
const COMPATIBILITY_HARNESS_SOURCE: &str = include_str!("compatibility_harness.rs");
const REAL_WORLD_CONFIGS_SOURCE: &str = include_str!("real_world_configs.rs");
const COMPATIBILITY_SOURCE: &str = include_str!("../COMPATIBILITY.md");
const YAML_SUITE_MANIFEST: &str = include_str!("fixtures/yaml-test-suite/manifest.toml");
const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const EMPTY_SCALAR_ANCHORS_RECORD: &str =
    include_str!("fixtures/divergences/records/empty-scalar-anchors.toml");
const RUST_REFERENCE_DIVERGENCE_CASES: &[&str] = &[
    "M7A3", // serde_yaml rejects the full bare-document stream; Rust parser references accept.
    "UT92", // serde_yaml rejects directive-looking lines inside open flow content; Rust parser references accept.
];
const TREE_SHAPE_DIVERGENCE_CASES: &[TreeShapeDivergenceCase] = &[TreeShapeDivergenceCase {
    id: "PW8X",
    record_case: "empty-scalar-anchors",
    record_source: EMPTY_SCALAR_ANCHORS_RECORD,
    compatibility_terms: &["PW8X", "empty scalar nodes", "tree-shape divergences"],
}];

struct TreeShapeDivergenceCase {
    id: &'static str,
    record_case: &'static str,
    record_source: &'static str,
    compatibility_terms: &'static [&'static str],
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
}

#[derive(Clone, Debug, Deserialize)]
struct SuiteCase {
    id: String,
    expected: ExpectedOutcome,
    policy: String,
    features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExpectedOutcome {
    Accept,
    SyntaxError,
    TreeError,
}

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    fixture: Vec<FixtureRecord>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    path: String,
    gates: Vec<String>,
}

impl SuiteCase {
    fn fixture_dir(&self) -> String {
        self.id.replace('/', "-")
    }
}

#[test]
fn yaml_suite_parity_sources_are_manifested_acceptance_cases() {
    let cases = yaml_suite_cases_by_fixture_dir();
    let event_dirs = yts_fixture_dirs(source_section(EVENT_PARITY_SOURCE, "const CASES"));
    let tree_dirs = yts_fixture_dirs(source_section(
        TREE_PARITY_SOURCE,
        "const VALUE_SHAPE_CASES",
    ));
    let compatibility_dirs = yts_fixture_dirs(source_section(
        COMPATIBILITY_HARNESS_SOURCE,
        "const SHARED_ACCEPT_CASES",
    ));

    assert_manifested_accept_cases("event parity", &event_dirs, &cases);
    assert_manifested_accept_cases("tree parity", &tree_dirs, &cases);
    assert_manifested_accept_cases("compatibility harness", &compatibility_dirs, &cases);
}

#[test]
fn yaml_suite_tree_only_duplicate_key_exclusions_are_explicit() {
    let cases_by_id = yaml_suite_cases_by_id();
    let excluded_ids: BTreeSet<&str> = [
        "2JQS", // duplicate missing block mapping keys
        "X38W", // alias-expanded collection keys
    ]
    .into_iter()
    .collect();
    let actual_tree_errors: BTreeSet<&str> = cases_by_id
        .values()
        .filter(|case| case.expected == ExpectedOutcome::TreeError)
        .map(|case| case.id.as_str())
        .collect();

    assert_eq!(
        actual_tree_errors, excluded_ids,
        "every tree-only YAML-suite rejection must be listed as an explicit parity exclusion",
    );

    let event_dirs = yts_fixture_dirs(source_section(EVENT_PARITY_SOURCE, "const CASES"));
    let tree_dirs = yts_fixture_dirs(source_section(
        TREE_PARITY_SOURCE,
        "const VALUE_SHAPE_CASES",
    ));
    let compatibility_dirs = yts_fixture_dirs(source_section(
        COMPATIBILITY_HARNESS_SOURCE,
        "const SHARED_ACCEPT_CASES",
    ));

    for id in excluded_ids {
        let case = cases_by_id
            .get(id)
            .unwrap_or_else(|| panic!("missing explicit exclusion case {id}"));
        assert_eq!(case.expected, ExpectedOutcome::TreeError);
        assert_eq!(case.policy, "raw-events-accept-tree-serde-reject");
        assert!(
            case.features
                .iter()
                .any(|feature| feature == "duplicate-key"),
            "{id} must keep the duplicate-key feature for exclusion searchability",
        );

        let fixture_dir = case.fixture_dir();
        for (surface, dirs) in [
            ("event parity", &event_dirs),
            ("tree parity", &tree_dirs),
            ("compatibility harness", &compatibility_dirs),
        ] {
            assert!(
                !dirs.contains(&fixture_dir),
                "{surface} must not silently include tree-only exclusion {id}",
            );
        }
    }
}

#[test]
fn yaml_suite_rust_reference_divergences_are_event_and_tree_gated() {
    let cases_by_id = yaml_suite_cases_by_id();
    let event_dirs = yts_fixture_dirs(source_section(EVENT_PARITY_SOURCE, "const CASES"));
    let tree_dirs = yts_fixture_dirs(source_section(
        TREE_PARITY_SOURCE,
        "const VALUE_SHAPE_CASES",
    ));
    let compatibility_dirs = yts_fixture_dirs(source_section(
        COMPATIBILITY_HARNESS_SOURCE,
        "const SHARED_ACCEPT_CASES",
    ));

    for id in RUST_REFERENCE_DIVERGENCE_CASES {
        let case = cases_by_id
            .get(*id)
            .unwrap_or_else(|| panic!("missing Rust-reference divergence case {id}"));
        assert_eq!(case.expected, ExpectedOutcome::Accept);
        assert_eq!(case.policy, "raw-events-tree-serde-accept");

        let fixture_dir = case.fixture_dir();
        assert!(
            event_dirs.contains(&fixture_dir),
            "Rust-reference divergence {id} must stay in event parity",
        );
        assert!(
            tree_dirs.contains(&fixture_dir),
            "Rust-reference divergence {id} must stay in tree parity",
        );
        assert!(
            !compatibility_dirs.contains(&fixture_dir),
            "Rust-reference divergence {id} must not be treated as shared serde_yaml acceptance",
        );
        assert!(
            COMPATIBILITY_HARNESS_SOURCE.contains(&format!("data/{fixture_dir}/in.yaml")),
            "Rust-reference divergence {id} must keep dedicated compatibility coverage",
        );
    }
}

#[test]
fn yaml_suite_tree_shape_divergences_are_explicitly_gated() {
    let cases_by_id = yaml_suite_cases_by_id();
    let event_dirs = yts_fixture_dirs(source_section(EVENT_PARITY_SOURCE, "const CASES"));
    let tree_dirs = yts_fixture_dirs(source_section(
        TREE_PARITY_SOURCE,
        "const VALUE_SHAPE_CASES",
    ));
    let compatibility_dirs = yts_fixture_dirs(source_section(
        COMPATIBILITY_HARNESS_SOURCE,
        "const SHARED_ACCEPT_CASES",
    ));

    for divergence in TREE_SHAPE_DIVERGENCE_CASES {
        let case = cases_by_id.get(divergence.id).unwrap_or_else(|| {
            panic!(
                "missing tree-shape divergence YAML-suite case {}",
                divergence.id
            )
        });
        assert_eq!(case.expected, ExpectedOutcome::Accept);
        assert_eq!(case.policy, "raw-events-tree-serde-accept");

        let fixture_dir = case.fixture_dir();
        assert!(
            event_dirs.contains(&fixture_dir),
            "tree-shape divergence {} must stay in event parity",
            divergence.id,
        );
        assert!(
            !tree_dirs.contains(&fixture_dir),
            "tree-shape divergence {} must not silently enter loaded-tree value-shape parity",
            divergence.id,
        );
        assert!(
            compatibility_dirs.contains(&fixture_dir),
            "tree-shape divergence {} must keep shared-reference acceptance coverage",
            divergence.id,
        );

        let expected_record = format!("case = \"{}\"", divergence.record_case);
        assert!(
            divergence.record_source.contains(&expected_record),
            "tree-shape divergence {} must link to divergence record {}",
            divergence.id,
            divergence.record_case,
        );
        assert!(
            divergence.record_source.contains("saphyr"),
            "tree-shape divergence {} record must document saphyr behavior",
            divergence.id,
        );
        assert!(
            divergence.record_source.contains("decision"),
            "tree-shape divergence {} record must document the compatibility decision",
            divergence.id,
        );
        for term in divergence.compatibility_terms {
            assert!(
                COMPATIBILITY_SOURCE.contains(term),
                "tree-shape divergence {} must keep COMPATIBILITY.md term {term:?}",
                divergence.id,
            );
        }
    }
}

#[test]
fn real_world_parity_sources_match_manifest_gates() {
    let typed_paths = real_world_paths(REAL_WORLD_CONFIGS_SOURCE);
    let event_paths = real_world_paths(source_section(EVENT_PARITY_SOURCE, "const CASES"));
    let tree_paths = real_world_paths(source_section(
        TREE_PARITY_SOURCE,
        "const REAL_WORLD_TREE_CASES",
    ));
    let compatibility_paths = real_world_paths(source_section(
        COMPATIBILITY_HARNESS_SOURCE,
        "const SHARED_ACCEPT_CASES",
    ));

    assert_eq!(
        typed_paths,
        real_world_paths_for_gate("typed-config"),
        "typed real-world config tests must match SOURCE.toml typed-config gates",
    );
    assert_eq!(
        event_paths,
        real_world_paths_for_gate("event-parity"),
        "event parity fixtures must match SOURCE.toml event-parity gates",
    );
    assert_eq!(
        tree_paths,
        real_world_paths_for_gate("tree-parity"),
        "tree parity fixtures must match SOURCE.toml tree-parity gates",
    );
    assert_eq!(
        compatibility_paths,
        real_world_paths_for_gate("shared-reference-acceptance"),
        "compatibility harness fixtures must match SOURCE.toml shared-reference-acceptance gates",
    );
}

fn yaml_suite_cases_by_fixture_dir() -> BTreeMap<String, SuiteCase> {
    yaml_suite_cases_by_id()
        .into_values()
        .map(|case| (case.fixture_dir(), case))
        .collect()
}

fn yaml_suite_cases_by_id() -> BTreeMap<String, SuiteCase> {
    let manifest: SuiteManifest =
        toml::from_str(YAML_SUITE_MANIFEST).expect("YAML-suite manifest parses");
    manifest
        .case
        .into_iter()
        .map(|case| (case.id.clone(), case))
        .collect()
}

fn real_world_paths_for_gate(gate: &str) -> BTreeSet<String> {
    let manifest: FixtureManifest =
        toml::from_str(REAL_WORLD_SOURCE).expect("real-world fixture source manifest parses");
    manifest
        .fixture
        .into_iter()
        .filter(|fixture| fixture.gates.iter().any(|recorded| recorded == gate))
        .map(|fixture| fixture.path)
        .collect()
}

fn assert_manifested_accept_cases(
    surface: &str,
    fixture_dirs: &BTreeSet<String>,
    cases: &BTreeMap<String, SuiteCase>,
) {
    assert!(
        !fixture_dirs.is_empty(),
        "{surface} has no YAML-suite cases"
    );
    for fixture_dir in fixture_dirs {
        let case = cases
            .get(fixture_dir)
            .unwrap_or_else(|| panic!("{surface} references unmanifested case {fixture_dir}"));
        assert_eq!(
            case.expected,
            ExpectedOutcome::Accept,
            "{surface} includes non-accept YAML-suite case {}",
            case.id,
        );
        assert_eq!(
            case.policy, "raw-events-tree-serde-accept",
            "{surface} includes YAML-suite case {} with non-accept policy",
            case.id,
        );
    }
}

fn yts_fixture_dirs(source: &str) -> BTreeSet<String> {
    extract_between_all(
        source,
        "include_str!(\"fixtures/yaml-test-suite/data/",
        "/in.yaml\")",
    )
}

fn real_world_paths(source: &str) -> BTreeSet<String> {
    extract_between_all(source, "include_str!(\"fixtures/real-world/", "\")")
}

fn source_section<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing source marker {marker}"));
    let section = &source[start..];
    let end = section
        .find("];")
        .unwrap_or_else(|| panic!("missing array terminator after {marker}"));
    &section[..end + 2]
}

fn extract_between_all(source: &str, prefix: &str, suffix: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let mut rest = source;
    while let Some(prefix_start) = rest.find(prefix) {
        rest = &rest[prefix_start + prefix.len()..];
        let suffix_start = rest
            .find(suffix)
            .unwrap_or_else(|| panic!("missing suffix {suffix} after {prefix}"));
        values.insert(rest[..suffix_start].to_owned());
        rest = &rest[suffix_start + suffix.len()..];
    }
    values
}
