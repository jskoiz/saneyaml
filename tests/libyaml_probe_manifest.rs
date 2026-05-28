use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::path::Path;

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
        "explicit-core-tags",
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
    assert_eq!(cases.len(), 10);

    let expected_ids = BTreeSet::from([
        "adjacent-flow-mapping-scalars",
        "duplicate-scalar-keys",
        "explicit-core-tags",
        "legacy-scalar-resolution",
        "merge-keys",
        "multiline-quoted-flow-key",
        "null-like-string-targets",
        "numeric-key-identity",
        "rw-github-actions-on-key",
        "tab-token-separation",
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
    assert_case_summary_contains(&artifact, "duplicate-scalar-keys", "second");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "Hello");
    assert_case_summary_contains(&artifact, "explicit-core-tags", "123");
    assert_case_summary_contains(&artifact, "null-like-string-targets", "NilClass");
    assert_case_summary_contains(&artifact, "numeric-key-identity", "Float");

    for id in ["adjacent-flow-mapping-scalars", "multiline-quoted-flow-key"] {
        let case = case_by_id(&artifact, id);
        assert_eq!(case["status"], "error", "{id}");
        assert_eq!(case["error_class"], "Psych::SyntaxError", "{id}");
    }
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
