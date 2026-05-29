use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const PROBE_SCRIPT: &str = include_str!("../scripts/probe-psych-libyaml.rb");
const PROBE_ARTIFACT: &str =
    include_str!("fixtures/divergences/probes/psych-3.1.0-libyaml-0.2.1.json");
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
    assert_eq!(cases.len(), 12);

    let expected_ids = BTreeSet::from([
        "adjacent-flow-mapping-scalars",
        "alias-graph-identity",
        "duplicate-scalar-keys",
        "explicit-core-tags",
        "legacy-scalar-resolution",
        "merge-keys",
        "multiline-quoted-flow-key",
        "null-like-string-targets",
        "numeric-key-identity",
        "rw-github-actions-on-key",
        "tab-token-separation",
        "yaml11-collection-tags",
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
    assert_eq!(alias_graph["summary"]["redefinition_b"], "one");
    assert_eq!(alias_graph["summary"]["redefinition_d"], "two");
    assert_eq!(alias_graph["summary"]["recursive_identity"], true);
    assert_case_summary_contains(&artifact, "duplicate-scalar-keys", "second");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "Hello");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "123");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "string_null");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "TrueClass");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "NilClass");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Set");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "Psych::Omap");
    assert_case_summary_contains(&artifact, "yaml11-collection-tags", "repeat");
    assert_case_summary_contains(&artifact, "null-like-string-targets", "NilClass");
    assert_case_summary_contains(&artifact, "numeric-key-identity", "Float");

    for id in ["adjacent-flow-mapping-scalars", "multiline-quoted-flow-key"] {
        let case = case_by_id(&artifact, id);
        assert_eq!(case["status"], "error", "{id}");
        assert_eq!(case["error_class"], "Psych::SyntaxError", "{id}");
    }
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

fn assert_case_summary_contains(artifact: &Json, id: &str, expected: &str) {
    let case = case_by_id(artifact, id);
    assert!(
        case.to_string().contains(expected),
        "{id} summary must contain {expected}"
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
