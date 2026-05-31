use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const EVENT_PARITY_SOURCE: &str = include_str!("event_parity.rs");
const TREE_PARITY_SOURCE: &str = include_str!("tree_parity.rs");
const COMPATIBILITY_HARNESS_SOURCE: &str = include_str!("compatibility_harness.rs");
const REAL_WORLD_CONFIGS_SOURCE: &str = include_str!("real_world_configs.rs");
const COMPATIBILITY_SOURCE: &str = include_str!("../COMPATIBILITY.md");
const YAML_SUITE_MANIFEST: &str = include_str!("fixtures/yaml-test-suite/manifest.toml");
const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const ADJACENT_FLOW_MAPPING_SCALARS_RECORD: &str =
    include_str!("fixtures/divergences/records/adjacent-flow-mapping-scalars.toml");
const BARE_DOCUMENT_STREAMS_RECORD: &str =
    include_str!("fixtures/divergences/records/bare-document-streams.toml");
const COLON_ANCHOR_NAMES_RECORD: &str =
    include_str!("fixtures/divergences/records/colon-anchor-names.toml");
const DIRECTIVE_LOOKING_FLOW_CONTENT_RECORD: &str =
    include_str!("fixtures/divergences/records/directive-looking-flow-content.toml");
const DOCUMENT_START_BLOCK_SCALARS_RECORD: &str =
    include_str!("fixtures/divergences/records/document-start-block-scalars.toml");
const EMPTY_IMPLICIT_KEYS_RECORD: &str =
    include_str!("fixtures/divergences/records/empty-implicit-keys.toml");
const EMPTY_SCALAR_ANCHORS_RECORD: &str =
    include_str!("fixtures/divergences/records/empty-scalar-anchors.toml");
const EXPLICIT_NON_SPECIFIC_TAG_SHAPE_RECORD: &str =
    include_str!("fixtures/divergences/records/explicit-non-specific-tag-shape.toml");
const MULTILINE_QUOTED_FLOW_KEY_RECORD: &str =
    include_str!("fixtures/divergences/records/multiline-quoted-flow-key.toml");
const RAW_EVENT_ANCHORS_ALIASES_RECORD: &str =
    include_str!("fixtures/divergences/records/raw-event-anchors-aliases.toml");
const RAW_EVENT_DIRECTIVES_RECORD: &str =
    include_str!("fixtures/divergences/records/raw-event-directives.toml");
const TAB_TOKEN_SEPARATION_RECORD: &str =
    include_str!("fixtures/divergences/records/tab-token-separation.toml");
const YAML_VERSION_DIRECTIVE_SCHEMA_RECORD: &str =
    include_str!("fixtures/divergences/records/yaml-version-directive-schema.toml");
const YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD: &str =
    include_str!("fixtures/divergences/records/yaml-suite-final-parity-deferrals.toml");
const RUST_REFERENCE_DIVERGENCE_CASES: &[&str] = &[
    "M7A3", // serde_yaml rejects the full bare-document stream; Rust parser references accept.
    "UT92", // serde_yaml rejects directive-looking lines inside open flow content; Rust parser references accept.
];
const EVENT_DEFERRED_DIVERGENCES: &[DeferredDivergenceCase] = &[
    DeferredDivergenceCase {
        id: "5TYM",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "7FWL",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "Q9WF",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "UGM3",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "K54U",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
];
const TREE_DEFERRED_DIVERGENCES: &[DeferredDivergenceCase] = &[
    DeferredDivergenceCase {
        id: "PW8X",
        record_case: "empty-scalar-anchors",
        record_source: EMPTY_SCALAR_ANCHORS_RECORD,
    },
    DeferredDivergenceCase {
        id: "6KGN",
        record_case: "empty-scalar-anchors",
        record_source: EMPTY_SCALAR_ANCHORS_RECORD,
    },
    DeferredDivergenceCase {
        id: "S4JQ",
        record_case: "explicit-non-specific-tag-shape",
        record_source: EXPLICIT_NON_SPECIFIC_TAG_SHAPE_RECORD,
    },
    DeferredDivergenceCase {
        id: "2AUY",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "33X3",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "74H7",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "C4HZ",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "F2C7",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "FH7J",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "L94M",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "WZ62",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "K54U",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
];
const SHARED_REFERENCE_DIVERGENCES: &[DeferredDivergenceCase] = &[
    DeferredDivergenceCase {
        id: "S3PD",
        record_case: "empty-implicit-keys",
        record_source: EMPTY_IMPLICIT_KEYS_RECORD,
    },
    DeferredDivergenceCase {
        id: "CFD4",
        record_case: "empty-implicit-keys",
        record_source: EMPTY_IMPLICIT_KEYS_RECORD,
    },
    DeferredDivergenceCase {
        id: "M2N8/00",
        record_case: "empty-implicit-keys",
        record_source: EMPTY_IMPLICIT_KEYS_RECORD,
    },
    DeferredDivergenceCase {
        id: "UKK6/00",
        record_case: "empty-implicit-keys",
        record_source: EMPTY_IMPLICIT_KEYS_RECORD,
    },
    DeferredDivergenceCase {
        id: "2SXE",
        record_case: "colon-anchor-names",
        record_source: COLON_ANCHOR_NAMES_RECORD,
    },
    DeferredDivergenceCase {
        id: "6LVF",
        record_case: "raw-event-directives",
        record_source: RAW_EVENT_DIRECTIVES_RECORD,
    },
    DeferredDivergenceCase {
        id: "6BCT",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "6CA3",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "Q5MG",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "Y79Y/001",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "Y79Y/010",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "DK95/00",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "DK95/03",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "DK95/04",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "5MUD",
        record_case: "adjacent-flow-mapping-scalars",
        record_source: ADJACENT_FLOW_MAPPING_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "5T43",
        record_case: "adjacent-flow-mapping-scalars",
        record_source: ADJACENT_FLOW_MAPPING_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "58MP",
        record_case: "adjacent-flow-mapping-scalars",
        record_source: ADJACENT_FLOW_MAPPING_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "9SA2",
        record_case: "multiline-quoted-flow-key",
        record_source: MULTILINE_QUOTED_FLOW_KEY_RECORD,
    },
    DeferredDivergenceCase {
        id: "6M2F",
        record_case: "raw-event-anchors-aliases",
        record_source: RAW_EVENT_ANCHORS_ALIASES_RECORD,
    },
    DeferredDivergenceCase {
        id: "W4TN",
        record_case: "document-start-block-scalars",
        record_source: DOCUMENT_START_BLOCK_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "BEC7",
        record_case: "yaml-version-directive-schema",
        record_source: YAML_VERSION_DIRECTIVE_SCHEMA_RECORD,
    },
    DeferredDivergenceCase {
        id: "2LFX",
        record_case: "raw-event-directives",
        record_source: RAW_EVENT_DIRECTIVES_RECORD,
    },
    DeferredDivergenceCase {
        id: "MUS6/05",
        record_case: "raw-event-directives",
        record_source: RAW_EVENT_DIRECTIVES_RECORD,
    },
    DeferredDivergenceCase {
        id: "MUS6/06",
        record_case: "raw-event-directives",
        record_source: RAW_EVENT_DIRECTIVES_RECORD,
    },
    DeferredDivergenceCase {
        id: "R4YG",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "96NN/00",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "96NN/01",
        record_case: "tab-token-separation",
        record_source: TAB_TOKEN_SEPARATION_RECORD,
    },
    DeferredDivergenceCase {
        id: "FP8R",
        record_case: "document-start-block-scalars",
        record_source: DOCUMENT_START_BLOCK_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "DK3J",
        record_case: "document-start-block-scalars",
        record_source: DOCUMENT_START_BLOCK_SCALARS_RECORD,
    },
    DeferredDivergenceCase {
        id: "M7A3",
        record_case: "bare-document-streams",
        record_source: BARE_DOCUMENT_STREAMS_RECORD,
    },
    DeferredDivergenceCase {
        id: "UT92",
        record_case: "directive-looking-flow-content",
        record_source: DIRECTIVE_LOOKING_FLOW_CONTENT_RECORD,
    },
    DeferredDivergenceCase {
        id: "4ABK",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "4MUZ/00",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "4MUZ/01",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "4MUZ/02",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "7Z25",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "8G76",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "8XYN",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "98YD",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "A2M4",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "AVM7",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "DBG4",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "FH7J",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "FRK4",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "HM87/00",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "HWV9",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "K3WX",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "NHX8",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "NJ66",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "NKF9",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "QT73",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "SM9W/01",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "VJP3/01",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
    DeferredDivergenceCase {
        id: "W5VH",
        record_case: "yaml-suite-final-parity-deferrals",
        record_source: YAML_SUITE_FINAL_PARITY_DEFERRALS_RECORD,
    },
];
const TREE_SHAPE_DIVERGENCE_CASES: &[TreeShapeDivergenceCase] = &[
    TreeShapeDivergenceCase {
        id: "PW8X",
        record_case: "empty-scalar-anchors",
        record_source: EMPTY_SCALAR_ANCHORS_RECORD,
        compatibility_terms: &["PW8X", "empty scalar nodes", "tree-shape divergences"],
    },
    TreeShapeDivergenceCase {
        id: "6KGN",
        record_case: "empty-scalar-anchors",
        record_source: EMPTY_SCALAR_ANCHORS_RECORD,
        compatibility_terms: &["6KGN", "empty scalar nodes", "tree-shape divergences"],
    },
    TreeShapeDivergenceCase {
        id: "S4JQ",
        record_case: "explicit-non-specific-tag-shape",
        record_source: EXPLICIT_NON_SPECIFIC_TAG_SHAPE_RECORD,
        compatibility_terms: &["S4JQ", "explicit non-specific tag"],
    },
];

struct DeferredDivergenceCase {
    id: &'static str,
    record_case: &'static str,
    record_source: &'static str,
}

struct TreeShapeDivergenceCase {
    id: &'static str,
    record_case: &'static str,
    record_source: &'static str,
    compatibility_terms: &'static [&'static str],
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
    parity: SuiteParity,
}

#[derive(Debug, Deserialize)]
struct SuiteParity {
    event: Vec<String>,
    event_deferred: Vec<String>,
    tree: Vec<String>,
    tree_deferred: Vec<String>,
    shared_reference: Vec<String>,
    shared_reference_deferred: Vec<String>,
    lossless_graph: Vec<String>,
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

    fn raw_events_should_parse(&self) -> bool {
        self.expected != ExpectedOutcome::SyntaxError
    }

    fn has_graph_syntax(&self) -> bool {
        self.features
            .iter()
            .any(|feature| matches!(feature.as_str(), "anchor" | "alias"))
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
fn yaml_suite_parity_sources_match_manifest_ledger() {
    let manifest = yaml_suite_manifest();
    let cases_by_id: BTreeMap<String, SuiteCase> = manifest
        .case
        .iter()
        .cloned()
        .map(|case| (case.id.clone(), case))
        .collect();

    let event_ids = yts_case_ids(
        source_section(EVENT_PARITY_SOURCE, "const CASES"),
        &cases_by_id,
    );
    let tree_ids = yts_case_ids(
        source_section(TREE_PARITY_SOURCE, "const VALUE_SHAPE_CASES"),
        &cases_by_id,
    );
    let shared_reference_ids = yts_case_ids(
        source_section(COMPATIBILITY_HARNESS_SOURCE, "const SHARED_ACCEPT_CASES"),
        &cases_by_id,
    );

    assert_eq!(
        event_ids,
        string_set(&manifest.parity.event),
        "event parity source cases must match the manifest-owned event ledger",
    );
    assert_eq!(
        tree_ids,
        string_set(&manifest.parity.tree),
        "tree parity source cases must match the manifest-owned tree ledger",
    );
    assert_eq!(
        shared_reference_ids,
        string_set(&manifest.parity.shared_reference),
        "shared-reference source cases must match the manifest-owned shared-reference ledger",
    );

    assert_parity_partition(
        "event",
        &manifest.parity.event,
        &manifest.parity.event_deferred,
        &cases_by_id,
    );
    assert_parity_partition(
        "tree",
        &manifest.parity.tree,
        &manifest.parity.tree_deferred,
        &cases_by_id,
    );
    assert_parity_partition(
        "shared-reference",
        &manifest.parity.shared_reference,
        &manifest.parity.shared_reference_deferred,
        &cases_by_id,
    );

    let graph_ids = cases_by_id
        .values()
        .filter(|case| case.raw_events_should_parse() && case.has_graph_syntax())
        .map(|case| case.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        string_set(&manifest.parity.lossless_graph),
        graph_ids,
        "lossless graph parity ledger must match graph-sensitive selected YAML-suite raw-event cases",
    );
}

#[test]
fn yaml_suite_deferred_parity_cases_are_explicit_divergences() {
    let manifest = yaml_suite_manifest();
    assert_deferred_cases_have_records(
        "event",
        &manifest.parity.event_deferred,
        EVENT_DEFERRED_DIVERGENCES,
    );
    assert_deferred_cases_have_records(
        "tree",
        &manifest.parity.tree_deferred,
        TREE_DEFERRED_DIVERGENCES,
    );
    assert_deferred_cases_have_records(
        "shared-reference",
        &manifest.parity.shared_reference_deferred,
        SHARED_REFERENCE_DIVERGENCES,
    );
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
fn yaml_suite_tag_anchor_metadata_cases_are_reference_gated() {
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

    let tag_anchor_cases: BTreeSet<&str> = cases_by_id
        .values()
        .filter(|case| {
            case.expected == ExpectedOutcome::Accept
                && case.features.iter().any(|feature| feature == "tag")
                && case.features.iter().any(|feature| feature == "anchor")
        })
        .map(|case| case.id.as_str())
        .collect();
    assert_eq!(
        tag_anchor_cases,
        BTreeSet::from([
            "3MYT", "9KAX", "BU8L", "C4HZ", "CUP7", "F2C7", "HMQ5", "LE5A", "UGM3",
        ]),
        "all accepted tag+anchor YAML-suite cases must be explicitly audited",
    );

    let event_deferred_ids = deferred_ids(EVENT_DEFERRED_DIVERGENCES);
    let tree_deferred_ids = deferred_ids(TREE_DEFERRED_DIVERGENCES);
    let compatibility_deferred_ids = deferred_ids(SHARED_REFERENCE_DIVERGENCES);

    for id in tag_anchor_cases {
        let case = cases_by_id
            .get(id)
            .unwrap_or_else(|| panic!("missing tag+anchor case {id}"));
        let fixture_dir = case.fixture_dir();
        for (surface, dirs, deferred) in [
            ("event parity", &event_dirs, &event_deferred_ids),
            ("tree parity", &tree_dirs, &tree_deferred_ids),
            (
                "compatibility harness",
                &compatibility_dirs,
                &compatibility_deferred_ids,
            ),
        ] {
            assert!(
                dirs.contains(&fixture_dir) || deferred.contains(id),
                "{surface} must reference-gate tag+anchor YAML-suite case {id}",
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
    yaml_suite_manifest()
        .case
        .into_iter()
        .map(|case| (case.id.clone(), case))
        .collect()
}

fn yaml_suite_manifest() -> SuiteManifest {
    toml::from_str(YAML_SUITE_MANIFEST).expect("YAML-suite manifest parses")
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

fn yts_case_ids(source: &str, cases_by_id: &BTreeMap<String, SuiteCase>) -> BTreeSet<String> {
    let cases_by_fixture_dir: BTreeMap<_, _> = cases_by_id
        .values()
        .map(|case| (case.fixture_dir(), case.id.clone()))
        .collect();
    yts_fixture_dirs(source)
        .into_iter()
        .map(|fixture_dir| {
            cases_by_fixture_dir
                .get(&fixture_dir)
                .cloned()
                .unwrap_or_else(|| panic!("source references unmanifested case {fixture_dir}"))
        })
        .collect()
}

fn string_set(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn assert_parity_partition(
    surface: &str,
    included: &[String],
    deferred: &[String],
    cases_by_id: &BTreeMap<String, SuiteCase>,
) {
    let included = string_set(included);
    let deferred = string_set(deferred);
    assert!(
        included.is_disjoint(&deferred),
        "{surface} parity included and deferred sets must not overlap",
    );

    let accepted: BTreeSet<_> = cases_by_id
        .values()
        .filter(|case| case.expected == ExpectedOutcome::Accept)
        .map(|case| case.id.clone())
        .collect();
    let partition: BTreeSet<_> = included.union(&deferred).cloned().collect();
    assert_eq!(
        partition, accepted,
        "{surface} parity ledger must include or explicitly defer every accepted YAML-suite case",
    );

    for id in included.union(&deferred) {
        let case = cases_by_id
            .get(id.as_str())
            .unwrap_or_else(|| panic!("{surface} parity ledger references unknown case {id}"));
        assert_eq!(
            case.expected,
            ExpectedOutcome::Accept,
            "{surface} parity ledger must only include accepted cases",
        );
    }
}

fn assert_deferred_cases_have_records(
    surface: &str,
    deferred: &[String],
    expected: &[DeferredDivergenceCase],
) {
    let actual: BTreeSet<_> = deferred.iter().map(String::as_str).collect();
    let expected_ids: BTreeSet<_> = expected.iter().map(|case| case.id).collect();
    assert_eq!(
        actual, expected_ids,
        "{surface} deferred cases must be exactly the documented divergence set",
    );

    for case in expected {
        let expected_record = format!("case = \"{}\"", case.record_case);
        assert!(
            case.record_source.contains(&expected_record),
            "{surface} deferred case {} must link to divergence record {}",
            case.id,
            case.record_case,
        );
        assert!(
            case.record_source.contains(case.id),
            "{surface} deferred case {} must be named in divergence record {}",
            case.id,
            case.record_case,
        );
        assert!(
            case.record_source.contains("decision"),
            "{surface} deferred case {} record must document the decision",
            case.id,
        );
    }
}

fn deferred_ids(cases: &[DeferredDivergenceCase]) -> BTreeSet<&'static str> {
    cases.iter().map(|case| case.id).collect()
}

fn real_world_paths(source: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let mut rest = source;
    let marker = "include_str!(";

    while let Some(marker_start) = rest.find(marker) {
        let after_marker = &rest[marker_start + marker.len()..];
        let after_whitespace = after_marker.trim_start();
        if let Some(after_quote) = after_whitespace.strip_prefix('"')
            && let Some(path) = after_quote.strip_prefix("fixtures/real-world/")
        {
            let end = path
                .find('"')
                .unwrap_or_else(|| panic!("missing closing quote after real-world fixture path"));
            values.insert(path[..end].to_owned());
        }
        rest = after_marker;
    }

    values
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
