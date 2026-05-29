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
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn psych_libyaml_probe_artifact_is_version_pinned_and_linked() {
    for term in [
        "EXPECTED_RUBY = \"2.6.10\"",
        "EXPECTED_PSYCH = \"3.1.0\"",
        "EXPECTED_LIBYAML = \"0.2.1\"",
        "Psych.libyaml_version",
        "legacy-scalar-resolution",
        "merge-keys",
        "alias-graph-identity",
        "explicit-core-tags",
        "yaml11-collection-tags",
        "yaml11-core-structural-tags",
        "legacy-merge-edge-recovery",
        "explicit-merge-tags",
        "lossless-merge-graph",
        "lossless-recursive-graph",
        "raw-event-directives",
        "raw-event-document-markers",
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
    assert_eq!(cases.len(), 28);

    let expected_ids = BTreeSet::from([
        "adjacent-flow-mapping-scalars",
        "alias-graph-identity",
        "alias-recursive-identity",
        "alias-redefinition-identity",
        "bare-document-streams",
        "core-structural-tags",
        "directive-looking-flow-content",
        "document-start-block-scalars",
        "document-start-inline-node",
        "duplicate-scalar-keys",
        "explicit-core-tags",
        "explicit-merge-tags",
        "legacy-scalar-resolution",
        "legacy-merge-edge-recovery",
        "lossless-merge-graph",
        "lossless-recursive-graph",
        "merge-keys",
        "multiline-quoted-flow-key",
        "null-like-string-targets",
        "numeric-key-identity",
        "raw-event-directives",
        "raw-event-document-markers",
        "rw-github-actions-on-key",
        "tab-token-separation",
        "tag-directive-scope-and-undeclared-handles",
        "yaml-version-directive-schema",
        "yaml11-collection-tags",
        "yaml11-core-structural-tags",
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
    assert_case_summary_contains(&artifact, "rw-github-actions-on-key", "TrueClass");
    assert_case_summary_contains(&artifact, "merge-keys", "app:v2");
    assert_merge_key_precedence(&artifact);
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
    assert_case_summary_contains(&artifact, "explicit-core-tags", "Hello");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "123");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "string_null");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "TrueClass");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "NilClass");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Set");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Omap");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "repeat");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "Array");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "Hash");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "value_mapping");
    assert_case_summary_contains(&artifact, "yaml11-core-structural-tags", "\"=\"");
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

    assert_case_summary_contains(&artifact, "raw-event-directives", "start_document");
    assert_case_summary_contains(
        &artifact,
        "raw-event-directives",
        "tag:example.com,2026:Thing",
    );
    assert_case_summary_contains(&artifact, "raw-event-directives", "root");
    assert_event_count(&artifact, "raw-event-document-markers", 11);
    assert_case_summary_contains(&artifact, "raw-event-document-markers", "end_document");
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
