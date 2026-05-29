use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use toml::Value as Toml;

const PROBE_SCRIPT: &str = include_str!("../scripts/probe-psych-libyaml.rb");
const PROBE_ARTIFACT: &str =
    include_str!("fixtures/divergences/probes/psych-3.1.0-libyaml-0.2.1.json");
const PROBE_COMPARISON: &str =
    include_str!("fixtures/divergences/probes/psych-libyaml-comparison.toml");
const PROBE_COVERAGE: &str =
    include_str!("fixtures/divergences/probes/psych-libyaml-coverage.toml");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn psych_libyaml_probe_artifact_is_version_pinned_and_linked() {
    for term in [
        "EXPECTED_RUBY = \"2.6.10\"",
        "EXPECTED_PSYCH = \"3.1.0\"",
        "EXPECTED_LIBYAML = \"0.2.1\"",
        "Psych.libyaml_version",
        "legacy-scalar-resolution",
        "yaml11-scalar-denominator",
        "yaml11-invalid-binary-payload",
        "merge-keys",
        "merge-nested-list-precedence",
        "merge-duplicate-local-key-policy",
        "merge-cross-document-anchor-reset",
        "merge-mixed-invalid-list-payload",
        "alias-graph-identity",
        "explicit-core-tags",
        "yaml11-collection-tags",
        "yaml11-set-non-null-payload",
        "yaml11-omap-non-singleton-entry",
        "yaml11-pairs-scalar-entry",
        "yaml11-core-structural-tags",
        "yaml11-value-resolved-handle",
        "yaml11-value-duplicate-key",
        "yaml11-signed-zero-key-collision",
        "yaml11-alias-key-collision",
        "legacy-merge-edge-recovery",
        "explicit-merge-tags",
        "lossless-merge-graph",
        "lossless-recursive-graph",
        "raw-event-directives",
        "raw-event-document-markers",
        "directive-stream-boundary",
        "reserved-directive",
        "repeated-tag-directive",
        "tag-directive-scope-reset",
    ] {
        assert!(
            PROBE_SCRIPT.contains(term),
            "probe script must contain {term}"
        );
    }

    let artifact: Json = serde_json::from_str(PROBE_ARTIFACT).expect("probe artifact JSON");
    assert_eq!(artifact["probe"], "psych-libyaml-divergence");
    assert_eq!(artifact["ruby"], "2.6.10");
    assert_eq!(artifact["psych"], "3.1.0");
    assert_eq!(artifact["libyaml"], "0.2.1");

    let cases = artifact["cases"].as_array().expect("probe cases array");
    assert_eq!(cases.len(), 45);

    let expected_ids = BTreeSet::from([
        "adjacent-flow-mapping-scalars",
        "alias-graph-identity",
        "alias-recursive-identity",
        "alias-redefinition-identity",
        "bare-document-streams",
        "core-structural-tags",
        "directive-looking-flow-content",
        "directive-stream-boundary",
        "document-start-block-scalars",
        "document-start-inline-node",
        "duplicate-scalar-keys",
        "explicit-core-tags",
        "explicit-merge-tags",
        "legacy-scalar-resolution",
        "yaml11-invalid-binary-payload",
        "yaml11-scalar-denominator",
        "legacy-merge-edge-recovery",
        "lossless-merge-graph",
        "lossless-recursive-graph",
        "merge-keys",
        "merge-cross-document-anchor-reset",
        "merge-duplicate-local-key-policy",
        "merge-mixed-invalid-list-payload",
        "merge-nested-list-precedence",
        "multiline-quoted-flow-key",
        "null-like-string-targets",
        "numeric-key-identity",
        "raw-event-directives",
        "raw-event-document-markers",
        "repeated-tag-directive",
        "reserved-directive",
        "rw-github-actions-on-key",
        "tab-token-separation",
        "tag-directive-scope-and-undeclared-handles",
        "tag-directive-scope-reset",
        "yaml-version-directive-schema",
        "yaml11-alias-key-collision",
        "yaml11-collection-tags",
        "yaml11-omap-non-singleton-entry",
        "yaml11-pairs-scalar-entry",
        "yaml11-signed-zero-key-collision",
        "yaml11-set-non-null-payload",
        "yaml11-core-structural-tags",
        "yaml11-value-duplicate-key",
        "yaml11-value-resolved-handle",
    ]);
    let actual_ids = cases
        .iter()
        .map(|case| case["id"].as_str().expect("case id"))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_ids, expected_ids);

    for case in cases {
        let id = case["id"].as_str().expect("case id");
        let record = case["record"].as_str().expect("case record");
        assert!(
            Path::new(ROOT).join(record).is_file(),
            "{id} must link to an existing divergence record"
        );
        assert!(
            matches!(case["status"].as_str(), Some("ok" | "error")),
            "{id} must record ok or error status"
        );
        let digest = case["input_sha256"]
            .as_str()
            .unwrap_or_else(|| panic!("{id} must record input_sha256"));
        assert_eq!(digest.len(), 64, "{id} input_sha256 length");
        assert!(
            digest.chars().all(|ch| ch.is_ascii_hexdigit()),
            "{id} input_sha256 must be hex"
        );
        assert!(
            case["input_bytes"].as_u64().unwrap_or_default() > 0,
            "{id} must record input byte length"
        );
    }

    assert_case_summary_contains(&artifact, "legacy-scalar-resolution", "TrueClass");
    assert_case_summary_contains(&artifact, "legacy-scalar-resolution", "Date");
    assert_case_summary_contains(&artifact, "legacy-scalar-resolution", "Infinity");
    assert_case_summary_contains(&artifact, "yaml11-scalar-denominator", "NilClass");
    assert_case_summary_contains(&artifact, "yaml11-scalar-denominator", "FalseClass");
    assert_case_summary_contains(
        &artifact,
        "yaml11-scalar-denominator",
        "340282366920938463463374607431768211456",
    );
    assert_case_summary_contains(
        &artifact,
        "yaml11-scalar-denominator",
        "2026-05-24T12:34:56+05:00",
    );
    assert_case_summary_contains(&artifact, "yaml11-scalar-denominator", "Hello");
    assert_case_summary_contains(&artifact, "yaml11-invalid-binary-payload", "Hello");
    assert_case_summary_contains(&artifact, "rw-github-actions-on-key", "TrueClass");
    assert_case_summary_contains(&artifact, "merge-keys", "app:v2");
    assert_merge_key_precedence(&artifact);
    assert_merge_permutation_cases(&artifact);
    let alias_graph = case_by_id(&artifact, "alias-graph-identity");
    assert_eq!(alias_graph["summary"]["shared_alias_identity"], true);
    assert_eq!(alias_graph["summary"]["mutation_visible_in_b"], 2);
    let alias_redefinition = case_by_id(&artifact, "alias-redefinition-identity");
    assert_eq!(
        entry_value(&alias_redefinition["summary"], "b")["value"].as_str(),
        Some("one")
    );
    assert_eq!(
        entry_value(&alias_redefinition["summary"], "d")["value"].as_str(),
        Some("two")
    );
    let alias_recursive = case_by_id(&artifact, "alias-recursive-identity");
    assert_eq!(alias_recursive["summary"]["recursive_identity"], true);
    assert_case_summary_contains(&artifact, "duplicate-scalar-keys", "second");
    assert_case_summary_contains(&artifact, "yaml11-signed-zero-key-collision", "negative");
    assert_case_summary_contains(&artifact, "yaml11-alias-key-collision", "second");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "Hello");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "123");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "string_null");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "TrueClass");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "NilClass");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Set");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Omap");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "repeat");
    assert_case_summary_contains(&artifact, "yaml11-set-non-null-payload", "Psych::Set");
    assert_case_summary_contains(&artifact, "yaml11-set-non-null-payload", "TrueClass");
    assert_case_summary_contains(&artifact, "yaml11-omap-non-singleton-entry", "Psych::Omap");
    assert_case_summary_contains(&artifact, "yaml11-omap-non-singleton-entry", "first");
    assert_case_summary_contains(&artifact, "yaml11-pairs-scalar-entry", "Array");
    assert_case_summary_contains(&artifact, "yaml11-pairs-scalar-entry", "scalar");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "Array");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "Hash");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "value_mapping");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "\"=\"");
    assert_case_summary_contains(&artifact, "yaml11-value-resolved-handle", "value_key");
    assert_case_summary_contains(&artifact, "yaml11-value-resolved-handle", "\"=\"");
    assert_case_summary_contains(&artifact, "yaml11-value-duplicate-key", "second");
    assert_case_summary_contains(&artifact, "core-structural-tags", "Array");
    assert_case_summary_contains(&artifact, "core-structural-tags", "Hash");
    assert_case_summary_contains(&artifact, "core-structural-tags", "value_mapping");
    assert_yaml11_fixture_merge_recovery(&artifact);
    assert_yaml11_fixture_explicit_merge_tags(&artifact);
    assert_case_summary_contains(&artifact, "lossless-merge-graph", "start_mapping");
    assert_case_summary_contains(&artifact, "lossless-merge-graph", "base");
    assert_case_summary_contains(&artifact, "lossless-merge-graph", "override");
    assert_case_summary_contains(&artifact, "lossless-merge-graph", "alias");
    assert_case_summary_contains(&artifact, "lossless-recursive-graph", "start_mapping");
    assert_case_summary_contains(&artifact, "lossless-recursive-graph", "root");
    assert_case_summary_contains(&artifact, "lossless-recursive-graph", "alias");
    assert_case_summary_contains(&artifact, "null-like-string-targets", "NilClass");
    assert_case_summary_contains(&artifact, "numeric-key-identity", "Float");

    for id in ["adjacent-flow-mapping-scalars", "multiline-quoted-flow-key"] {
        let case = case_by_id(&artifact, id);
        assert_eq!(case["status"], "error", "{id}");
        assert_eq!(case["error_class"], "Psych::SyntaxError", "{id}");
    }
    assert_error_location(&artifact, "adjacent-flow-mapping-scalars", 1, 7);
    assert_error_location(&artifact, "directive-looking-flow-content", 2, 1);

    assert_case_summary_contains(&artifact, "raw-event-directives", "start_document");
    assert_case_summary_contains(
        &artifact,
        "raw-event-directives",
        "tag:example.com,2026:Thing",
    );
    assert_case_summary_contains(&artifact, "raw-event-directives", "root");
    assert_event_count(&artifact, "raw-event-document-markers", 11);
    assert_case_summary_contains(&artifact, "raw-event-document-markers", "end_document");
    assert_case_summary_contains(&artifact, "directive-stream-boundary", "document_count");
    assert_case_summary_contains(&artifact, "directive-stream-boundary", "TrueClass");
    assert_case_summary_contains(&artifact, "reserved-directive", "unknown directive name");
    assert_error_contains(
        &artifact,
        "repeated-tag-directive",
        "duplicate %TAG directive",
    );
    assert_error_contains(
        &artifact,
        "tag-directive-scope-reset",
        "undefined tag handle",
    );
    assert_case_summary_contains(&artifact, "document-start-inline-node", "!Thing");
    assert_case_summary_contains(&artifact, "document-start-inline-node", "root");

    assert_error_contains(
        &artifact,
        "yaml-version-directive-schema",
        "incompatible YAML document",
    );
    assert_error_contains(
        &artifact,
        "tag-directive-scope-and-undeclared-handles",
        "undefined tag handle",
    );
    assert_error_contains(
        &artifact,
        "document-start-block-scalars",
        "incompatible YAML document",
    );
    assert_error_contains(
        &artifact,
        "bare-document-streams",
        "expected <document start>",
    );
    assert_error_contains(
        &artifact,
        "directive-looking-flow-content",
        "expected ',' or '}'",
    );
    let cross_document = case_by_id(&artifact, "merge-cross-document-anchor-reset");
    assert_eq!(cross_document["status"], "error");
    assert_eq!(cross_document["error_class"], "Psych::BadAlias");
    assert!(
        cross_document["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Unknown alias: base")
    );
}

#[test]
fn psych_libyaml_probe_reproduces_artifact_when_pinned_runtime_is_available() {
    let output = Command::new("ruby")
        .arg("scripts/probe-psych-libyaml.rb")
        .current_dir(ROOT)
        .output()
        .expect("ruby command runs");
    if !output.status.success() {
        return;
    }

    let regenerated: Json =
        serde_json::from_slice(&output.stdout).expect("regenerated probe JSON parses");
    let checked: Json = serde_json::from_str(PROBE_ARTIFACT).expect("checked probe JSON parses");
    assert_eq!(regenerated, checked);
}

#[test]
fn psych_libyaml_probe_cases_have_rust_policy_gate() {
    let artifact: Json = serde_json::from_str(PROBE_ARTIFACT).expect("probe artifact JSON");
    let manifest: Toml = toml::from_str(PROBE_COMPARISON).expect("comparison manifest TOML");
    assert_eq!(
        manifest["schema"].as_str(),
        Some("psych-libyaml-rust-comparison-v1")
    );

    let artifact_cases = artifact["cases"].as_array().expect("artifact cases");
    let manifest_cases = manifest["case"].as_array().expect("manifest cases");
    assert_eq!(manifest_cases.len(), artifact_cases.len());

    let artifact_ids = artifact_cases
        .iter()
        .map(|case| case["id"].as_str().expect("artifact id"))
        .collect::<BTreeSet<_>>();
    let manifest_ids = manifest_cases
        .iter()
        .map(|case| case["id"].as_str().expect("manifest id"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        manifest_ids, artifact_ids,
        "every pinned Psych/libyaml probe case must have a Rust comparison policy"
    );

    for case in manifest_cases {
        let id = toml_str(case, "id");
        let record = toml_str(case, "record");
        let psych_case = case_by_id(&artifact, id);
        let input = case_input(case);
        let expected_digest = sha256_hex(input.as_bytes());
        assert_eq!(
            psych_case["input_sha256"].as_str(),
            Some(expected_digest.as_str()),
            "{id} Psych and Rust comparison inputs must be byte-identical"
        );
        assert_eq!(
            psych_case["input_bytes"].as_u64(),
            Some(input.len() as u64),
            "{id} Psych and Rust comparison input byte lengths must match"
        );
        assert_eq!(
            Some(toml_str(case, "psych_status")),
            psych_case["status"].as_str(),
            "{id} psych status must stay pinned to the artifact"
        );
        assert!(
            Path::new(ROOT).join(record).is_file(),
            "{id} must link to an existing divergence record"
        );

        let rust_policy = toml_str(case, "rust_policy");
        assert!(
            matches!(rust_policy, "matches-psych" | "intentional-divergence"),
            "{id} must choose a final Rust-vs-Psych policy"
        );
        assert!(
            !toml_str(case, "reason").is_empty(),
            "{id} must describe why that policy is final"
        );

        let rust_probe = run_rust_probe_case(case);
        let expected_rust_status = toml_str(case, "rust_status");
        assert_eq!(rust_probe.status, expected_rust_status, "{id} Rust status");

        if rust_policy == "matches-psych" {
            assert_eq!(
                expected_rust_status,
                toml_str(case, "psych_status"),
                "{id} cannot be marked matches-psych with different success/error status"
            );
        } else {
            let record_text =
                fs::read_to_string(Path::new(ROOT).join(record)).expect("divergence record");
            assert!(
                record_text.contains("decision"),
                "{id} intentional divergence must have a decision field"
            );
            assert!(
                record_text.contains("migration_impact"),
                "{id} intentional divergence must explain migration impact"
            );
        }

        for fragment in toml_str_array(case, "rust_contains") {
            let compact_output = compact_whitespace(&rust_probe.output);
            let compact_fragment = compact_whitespace(fragment);
            assert!(
                rust_probe.output.contains(fragment) || compact_output.contains(&compact_fragment),
                "{id} Rust output must contain {fragment}; output was {}",
                rust_probe.output
            );
        }

        if let Some(fragment) = toml_optional_str(case, "rust_error_contains") {
            assert!(
                rust_probe.output.contains(fragment),
                "{id} Rust error must contain {fragment}; output was {}",
                rust_probe.output
            );
        }
    }
}

#[test]
fn psych_libyaml_probe_coverage_ledger_groups_all_pinned_cases() {
    let artifact: Json = serde_json::from_str(PROBE_ARTIFACT).expect("probe artifact JSON");
    let comparison: Toml = toml::from_str(PROBE_COMPARISON).expect("comparison manifest TOML");
    let coverage: Toml = toml::from_str(PROBE_COVERAGE).expect("coverage ledger TOML");
    assert_eq!(toml_str(&coverage, "schema"), "psych-libyaml-coverage-v1");
    assert_eq!(artifact["ruby"].as_str(), Some(toml_str(&coverage, "ruby")));
    assert_eq!(
        artifact["psych"].as_str(),
        Some(toml_str(&coverage, "psych")),
    );
    assert_eq!(
        artifact["libyaml"].as_str(),
        Some(toml_str(&coverage, "libyaml")),
    );
    assert!(
        toml_str(&coverage, "coverage_policy")
            .contains("not blanket YAML 1.1/libyaml compatibility"),
        "coverage policy must avoid claiming blanket libyaml compatibility",
    );

    for path_key in ["probe_artifact", "comparison_manifest"] {
        let path = toml_str(&coverage, path_key);
        assert!(
            Path::new(ROOT).join(path).is_file(),
            "coverage ledger {path_key} must point to an existing file",
        );
    }

    let artifact_ids = artifact["cases"]
        .as_array()
        .expect("artifact cases")
        .iter()
        .map(|case| case["id"].as_str().expect("artifact case id"))
        .collect::<BTreeSet<_>>();
    let comparison_cases = comparison["case"].as_array().expect("comparison cases");
    let comparison_ids = comparison_cases
        .iter()
        .map(|case| toml_str(case, "id"))
        .collect::<BTreeSet<_>>();
    assert_eq!(comparison_ids, artifact_ids);
    assert_eq!(
        coverage["probe_case_count"].as_integer(),
        Some(artifact_ids.len() as i64),
    );

    let families = coverage["behavior_family"]
        .as_array()
        .expect("coverage behavior families");
    assert_eq!(
        coverage["behavior_family_count"].as_integer(),
        Some(families.len() as i64),
    );
    assert_eq!(families.len(), 8);

    let mut family_ids = BTreeSet::new();
    let mut family_case_union = BTreeSet::new();
    for family in families {
        let id = toml_str(family, "id");
        assert!(family_ids.insert(id), "duplicate behavior family {id}");
        assert!(
            matches!(
                toml_str(family, "status"),
                "covered" | "partial" | "covered-divergence"
            ),
            "{id} must use a known coverage status",
        );
        for field in ["summary", "adoption_risk", "next_expansion"] {
            assert!(
                !toml_str(family, field).trim().is_empty(),
                "{id} must document {field}",
            );
        }

        let cases = toml_str_array(family, "cases");
        assert!(!cases.is_empty(), "{id} must own at least one probe case");
        for case_id in &cases {
            assert!(
                artifact_ids.contains(case_id),
                "{id} references unknown probe case {case_id}",
            );
            family_case_union.insert(*case_id);
        }

        let actual_entrypoints = comparison_cases
            .iter()
            .filter(|case| cases.contains(&toml_str(case, "id")))
            .map(|case| toml_str(case, "rust_entrypoint"))
            .collect::<BTreeSet<_>>();
        for entrypoint in toml_str_array(family, "rust_entrypoints") {
            assert!(
                actual_entrypoints.contains(entrypoint),
                "{id} must include a case for rust entrypoint {entrypoint}",
            );
        }
    }
    assert_eq!(
        family_case_union, artifact_ids,
        "every pinned Psych/libyaml probe case must belong to a behavior family",
    );

    let gaps = coverage["tracked_gap"]
        .as_array()
        .expect("coverage tracked gaps");
    assert_eq!(
        coverage["tracked_gap_count"].as_integer(),
        Some(gaps.len() as i64),
    );
    assert_eq!(gaps.len(), 3);
    let mut gap_ids = BTreeSet::new();
    for gap in gaps {
        let id = toml_str(gap, "id");
        assert!(gap_ids.insert(id), "duplicate tracked gap {id}");
        let family = toml_str(gap, "family");
        assert!(
            family_ids.contains(family),
            "tracked gap {id} must link to a behavior family",
        );
        assert!(
            !toml_str(gap, "reason").trim().is_empty(),
            "tracked gap {id} must explain why coverage is incomplete",
        );
        assert!(
            !toml_str_array(gap, "next_probe_candidates").is_empty(),
            "tracked gap {id} must list next probe candidates",
        );
    }
}

fn assert_case_summary_contains(artifact: &Json, id: &str, expected: &str) {
    let case = case_by_id(artifact, id);
    assert!(
        case.to_string().contains(expected),
        "{id} summary must contain {expected}"
    );
}

struct RustProbe {
    status: &'static str,
    output: String,
}

fn run_rust_probe_case(case: &Toml) -> RustProbe {
    let input = case_input(case);
    match toml_str(case, "rust_entrypoint") {
        "default-value" => rust_probe_from_result(
            yaml::from_str::<yaml::Value>(&input).map(|value| format!("{value:#?}")),
        ),
        "yaml11-value" => rust_probe_from_result(
            yaml::LoadOptions::yaml_1_1()
                .from_str::<yaml::Value>(&input)
                .map(|value| format!("{value:#?}")),
        ),
        "directive-value" => rust_probe_from_result(
            yaml::LoadOptions::yaml_version_directive()
                .from_str::<yaml::Value>(&input)
                .map(|value| format!("{value:#?}")),
        ),
        "directive-stream" => rust_probe_from_result(
            yaml::LoadOptions::yaml_version_directive()
                .from_documents_str::<yaml::Value>(&input)
                .map(|value| format!("{value:#?}")),
        ),
        "typed-set-strings" => rust_probe_from_result(
            yaml::from_str::<BTreeSet<String>>(&input).map(|value| format!("{value:#?}")),
        ),
        "typed-string-i64-pairs" => rust_probe_from_result(
            yaml::from_str::<Vec<(String, i64)>>(&input).map(|value| format!("{value:#?}")),
        ),
        "events" => {
            rust_probe_from_result(yaml::parse_events(&input).map(|events| format!("{events:#?}")))
        }
        "lossless" => rust_probe_from_result(
            yaml::parse_lossless(&input).map(|stream| format!("{stream:#?}")),
        ),
        other => panic!("unknown Rust probe entrypoint {other}"),
    }
}

fn rust_probe_from_result(result: yaml::Result<String>) -> RustProbe {
    match result {
        Ok(output) => RustProbe {
            status: "ok",
            output,
        },
        Err(error) => RustProbe {
            status: "error",
            output: error.to_string(),
        },
    }
}

fn case_input(case: &Toml) -> String {
    if let Some(yaml) = toml_optional_str(case, "yaml") {
        return yaml.to_owned();
    }
    if let Some(fixture) = toml_optional_str(case, "fixture") {
        return fs::read_to_string(Path::new(ROOT).join(fixture))
            .unwrap_or_else(|error| panic!("{fixture}: {error}"));
    }
    panic!("{} must define yaml or fixture", toml_str(case, "id"));
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn toml_str<'a>(value: &'a Toml, key: &str) -> &'a str {
    toml_optional_str(value, key).unwrap_or_else(|| panic!("missing TOML string key {key}"))
}

fn toml_optional_str<'a>(value: &'a Toml, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Toml::as_str)
}

fn toml_str_array<'a>(value: &'a Toml, key: &str) -> Vec<&'a str> {
    value
        .get(key)
        .and_then(Toml::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{key} entries must be strings"))
        })
        .collect()
}

fn compact_whitespace(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != ',')
        .collect()
}

fn assert_event_count(artifact: &Json, id: &str, expected: usize) {
    let case = case_by_id(artifact, id);
    assert_eq!(
        case["summary"]["event_count"].as_u64(),
        Some(expected as u64),
        "{id} event count"
    );
}

fn assert_error_contains(artifact: &Json, id: &str, expected: &str) {
    let case = case_by_id(artifact, id);
    assert_eq!(case["status"], "error", "{id}");
    assert_eq!(case["error_class"], "Psych::SyntaxError", "{id}");
    assert!(
        case["error"]
            .as_str()
            .unwrap_or_default()
            .contains(expected),
        "{id} error must contain {expected}"
    );
}

fn assert_error_location(artifact: &Json, id: &str, line: u64, column: u64) {
    let case = case_by_id(artifact, id);
    assert_eq!(case["status"], "error", "{id}");
    assert_eq!(case["line"].as_u64(), Some(line), "{id} line");
    assert_eq!(case["column"].as_u64(), Some(column), "{id} column");
}

fn case_by_id<'a>(artifact: &'a Json, id: &str) -> &'a Json {
    artifact["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing probe case {id}"))
}

fn assert_merge_key_precedence(artifact: &Json) {
    let case = case_by_id(artifact, "merge-keys");
    assert_eq!(case["status"], "ok");
    let summary = &case["summary"];

    assert_mapping_entry_value(summary, "service", "image", "app:v2");
    assert_mapping_entry_value(summary, "service", "replicas", "2");

    assert_mapping_entry_value(summary, "list_service", "shared", "first");
    assert_mapping_entry_value(summary, "list_service", "image", "app:first");
    assert_mapping_entry_value(summary, "list_service", "timeout", "10");
    assert_mapping_entry_value(summary, "list_service", "retries", "3");

    assert_mapping_entry_value(summary, "explicit_service", "shared", "explicit");
    assert_mapping_entry_value(summary, "explicit_service", "image", "app:first");
    assert_mapping_entry_value(summary, "explicit_service", "timeout", "explicit");
    assert_mapping_entry_value(summary, "explicit_service", "retries", "3");

    assert_mapping_entry_value(summary, "tagged_service", "image", "app:tagged");
    assert_mapping_entry_value(summary, "tagged_service", "replicas", "2");
    assert_mapping_entry_value(summary, "canonical_service", "image", "app:canonical");
    assert_mapping_entry_value(summary, "canonical_service", "replicas", "2");
    assert_mapping_entry_value(summary, "string_service", "<<", "literal");
    assert_mapping_entry_value(summary, "string_service", "image", "app:string");
    assert_mapping_entry_value(summary, "custom_service", "<<", "literal");
    assert_mapping_entry_value(summary, "custom_service", "image", "app:custom");

    assert_mapping_entry_value(summary, "scalar_merge", "<<", "scalar");
    assert_mapping_entry_value(summary, "scalar_merge", "keep", "value");
    assert_mapping_entry_value(summary, "quoted_scalar_merge", "<<", "literal");
    assert_mapping_entry_value(summary, "quoted_scalar_merge", "keep", "value");
    assert_mapping_entry_value(summary, "tagged_scalar_merge", "<<", "literal");
    assert_mapping_entry_value(summary, "tagged_scalar_merge", "keep", "value");
    assert_mapping_entry_sequence_item(summary, "sequence_scalar_merge", "<<", 0, "scalar");
    assert_mapping_entry_value(summary, "sequence_scalar_merge", "keep", "value");

    assert_mapping_entry_value(summary, "repeated_merge", "shared", "second");
    assert_mapping_entry_value(summary, "repeated_merge", "retries", "3");
    assert_mapping_entry_value(summary, "repeated_merge", "timeout", "10");
    assert_mapping_entry_value(summary, "repeated_merge", "keep", "value");
    assert_mapping_entry_value(summary, "repeated_tagged_merge", "shared", "second");
    assert_mapping_entry_value(summary, "repeated_tagged_merge", "retries", "3");
    assert_mapping_entry_value(summary, "repeated_tagged_merge", "timeout", "10");
    assert_mapping_entry_value(summary, "repeated_tagged_merge", "keep", "value");
}

fn assert_merge_permutation_cases(artifact: &Json) {
    let nested = case_by_id(artifact, "merge-nested-list-precedence");
    assert_eq!(nested["status"], "ok");
    let nested_summary = &nested["summary"];
    assert_mapping_entry_value(nested_summary, "mid", "a", "1");
    assert_mapping_entry_value(nested_summary, "mid", "shared", "mid");
    assert_mapping_entry_value(nested_summary, "target", "shared", "target");
    assert_mapping_entry_value(nested_summary, "target", "a", "1");
    assert_mapping_entry_value(nested_summary, "target", "b", "2");
    assert_mapping_entry_value(nested_summary, "target", "c", "3");

    let duplicate = case_by_id(artifact, "merge-duplicate-local-key-policy");
    assert_eq!(duplicate["status"], "ok");
    assert_mapping_entry_value(&duplicate["summary"], "target", "a", "local2");

    let mixed = case_by_id(artifact, "merge-mixed-invalid-list-payload");
    assert_eq!(mixed["status"], "ok");
    assert_mapping_entry_sequence_item(&mixed["summary"], "target", "<<", 1, "scalar");
    assert_mapping_entry_value(&mixed["summary"], "target", "keep", "value");
}

fn assert_yaml11_fixture_merge_recovery(artifact: &Json) {
    let case = case_by_id(artifact, "legacy-merge-edge-recovery");
    assert_eq!(case["status"], "ok");
    let summary = &case["summary"];

    assert_mapping_entry_value(summary, "repeated_merge", "shared", "second");
    assert_mapping_entry_value(summary, "repeated_merge", "retries", "3");
    assert_mapping_entry_value(summary, "repeated_merge", "timeout", "10");
    assert_mapping_entry_value(summary, "override_merge", "shared", "explicit");
    assert_mapping_entry_value(summary, "scalar_merge", "<<", "scalar");
    assert_mapping_entry_value(summary, "tagged_scalar_merge", "<<", "literal");
    assert_mapping_entry_sequence_item(summary, "sequence_scalar_merge", "<<", 0, "scalar");
    assert_mapping_entry_value(summary, "literal_and_merge", "<<", "literal");
    assert_mapping_entry_value(summary, "literal_and_merge", "image", "explicit");
}

fn assert_yaml11_fixture_explicit_merge_tags(artifact: &Json) {
    let case = case_by_id(artifact, "explicit-merge-tags");
    assert_eq!(case["status"], "ok");
    let summary = &case["summary"];

    assert_mapping_entry_value(summary, "tagged_service", "image", "app:tagged");
    assert_mapping_entry_value(summary, "tagged_service", "replicas", "2");
    assert_mapping_entry_value(summary, "canonical_service", "image", "app:canonical");
    assert_mapping_entry_value(summary, "canonical_service", "replicas", "2");
    assert_mapping_entry_value(summary, "resolved_service", "image", "app:resolved");
    assert_mapping_entry_value(summary, "resolved_service", "replicas", "2");
    assert_mapping_entry_value(summary, "resolved_list_service", "shared", "first");
    assert_mapping_entry_value(summary, "resolved_list_service", "timeout", "explicit");
    assert_mapping_entry_value(summary, "resolved_list_service", "retries", "3");
    assert_mapping_entry_value(summary, "string_service", "<<", "literal");
    assert_mapping_entry_value(summary, "custom_service", "<<", "literal");
}

fn assert_mapping_entry_value(summary: &Json, mapping_key: &str, entry_key: &str, expected: &str) {
    let mapping = entry_value(summary, mapping_key);
    let value = entry_value(mapping, entry_key);
    assert_eq!(
        value["value"].as_str(),
        Some(expected),
        "{mapping_key}.{entry_key}"
    );
}

fn assert_mapping_entry_sequence_item(
    summary: &Json,
    mapping_key: &str,
    entry_key: &str,
    index: usize,
    expected: &str,
) {
    let mapping = entry_value(summary, mapping_key);
    let value = entry_value(mapping, entry_key);
    assert_eq!(
        value["items"][index]["value"].as_str(),
        Some(expected),
        "{mapping_key}.{entry_key}[{index}]"
    );
}

fn entry_value<'a>(summary: &'a Json, key: &str) -> &'a Json {
    let entries = summary["entries"]
        .as_array()
        .expect("summary entries array");
    entries
        .iter()
        .find(|entry| entry["key"]["value"] == key)
        .map(|entry| &entry["value"])
        .unwrap_or_else(|| panic!("missing summary entry {key}"))
}
