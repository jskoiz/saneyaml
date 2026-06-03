use saneyaml::{ErrorCategory, Event, LoadOptions, NodeValue, Value};

/// Builds a `target` mapping whose merge source is an `n`-deep chain of
/// `<<`-referencing anchors, e.g. `a1: {<<: *a0}`, `a2: {<<: *a1}`, ...
fn deep_merge_chain(n: usize) -> String {
    let mut input = String::from("a0: &a0 {v: 0}\n");
    for i in 1..=n {
        input.push_str(&format!("a{i}: &a{i} {{<<: *a{}}}\n", i - 1));
    }
    input.push_str(&format!("top: {{<<: *a{n}}}\n"));
    input
}

fn top_mapping(node: &saneyaml::Node) -> &[(saneyaml::Node, saneyaml::Node)] {
    let NodeValue::Mapping(entries) = &node.value else {
        panic!("expected top-level mapping");
    };
    entries
}

fn mapping_value<'a>(
    entries: &'a [(saneyaml::Node, saneyaml::Node)],
    key: &str,
) -> &'a saneyaml::Node {
    entries
        .iter()
        .find_map(|(existing, value)| (existing.as_str() == Some(key)).then_some(value))
        .unwrap_or_else(|| panic!("missing key {key}"))
}

#[test]
fn default_merge_expands_loaded_tree_and_value_reads() {
    let input = "\
defaults: &defaults
  retries: 3
  command: deploy
job:
  <<: *defaults
  command: smoke
";

    let node = saneyaml::parse_str(input).expect("parse default merge tree");
    let root = top_mapping(&node);
    let job = mapping_value(root, "job");
    let NodeValue::Mapping(job_entries) = &job.value else {
        panic!("expected job mapping");
    };

    assert!(
        job_entries
            .iter()
            .all(|(key, _)| key.as_str() != Some("<<"))
    );
    assert_eq!(mapping_value(job_entries, "retries").as_str(), None);
    assert!(matches!(
        mapping_value(job_entries, "retries").value,
        NodeValue::Number(saneyaml::Number::Integer(3))
    ));
    assert_eq!(
        mapping_value(job_entries, "command").as_str(),
        Some("smoke")
    );

    let mut value: Value = saneyaml::from_str(input).expect("value read applies merge by default");
    assert!(value["job"]["<<"].is_null());
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));
    assert_eq!(value["job"]["command"].as_str(), Some("smoke"));
    let before = value.clone();
    value
        .apply_merge()
        .expect("default-expanded values remain idempotent");
    assert!(value.equivalent(&before));
}

#[test]
fn default_merge_list_preserves_precedence_and_explicit_override() {
    let input = "\
base1: &base1 {a: 1, b: 1, shared: first}
base2: &base2 {b: 2, c: 2, shared: second}
merged:
  <<: [*base1, *base2]
  b: explicit
";

    let value: Value = saneyaml::from_str(input).expect("merge list expands by default");
    assert!(value["merged"]["<<"].is_null());
    assert_eq!(value["merged"]["a"].as_u64(), Some(1));
    assert_eq!(value["merged"]["b"].as_str(), Some("explicit"));
    assert_eq!(value["merged"]["c"].as_u64(), Some(2));
    assert_eq!(value["merged"]["shared"].as_str(), Some("first"));
}

#[test]
fn default_merge_expands_nested_sources_before_list_precedence() {
    let input = "\
base: &base {a: 1, shared: base}
mid: &mid {<<: *base, b: 2, shared: mid}
other: &other {shared: other, c: 3}
target:
  <<: [*mid, *other]
  shared: target
";

    let value: Value = saneyaml::from_str(input).expect("nested merge sources expand by default");
    assert!(value["mid"]["<<"].is_null());
    assert_eq!(value["mid"]["a"].as_u64(), Some(1));
    assert_eq!(value["mid"]["b"].as_u64(), Some(2));
    assert_eq!(value["mid"]["shared"].as_str(), Some("mid"));

    assert!(value["target"]["<<"].is_null());
    assert_eq!(value["target"]["a"].as_u64(), Some(1));
    assert_eq!(value["target"]["b"].as_u64(), Some(2));
    assert_eq!(value["target"]["c"].as_u64(), Some(3));
    assert_eq!(value["target"]["shared"].as_str(), Some("target"));
}

#[test]
fn default_merge_expands_explicit_merge_tag_keys() {
    let input = "\
base: &base {a: 1, b: 1}
tagged:
  !!merge <<: *base
  b: tagged
canonical:
  !<tag:yaml.org,2002:merge> <<: *base
  b: canonical
";

    let node = saneyaml::parse_str(input).expect("parse explicit merge-tag keys");
    let root = top_mapping(&node);
    for (key, expected_b) in [("tagged", "tagged"), ("canonical", "canonical")] {
        let target = mapping_value(root, key);
        let NodeValue::Mapping(entries) = &target.value else {
            panic!("expected {key} mapping");
        };

        assert!(
            entries.iter().all(|(key, _)| key.as_str() != Some("<<")),
            "{key} merge tag must be expanded"
        );
        assert!(matches!(
            mapping_value(entries, "a").value,
            NodeValue::Number(saneyaml::Number::Integer(1))
        ));
        assert_eq!(mapping_value(entries, "b").as_str(), Some(expected_b));
    }

    let value: Value = saneyaml::from_str(input).expect("value read expands explicit merge tags");
    assert!(value["tagged"]["<<"].is_null());
    assert_eq!(value["tagged"]["a"].as_u64(), Some(1));
    assert_eq!(value["tagged"]["b"].as_str(), Some("tagged"));
    assert!(value["canonical"]["<<"].is_null());
    assert_eq!(value["canonical"]["a"].as_u64(), Some(1));
    assert_eq!(value["canonical"]["b"].as_str(), Some("canonical"));
}

#[test]
fn default_merge_expands_flow_and_directive_tagged_merge_keys() {
    let input = "\
%TAG !m! tag:yaml.org,2002:
---
base: &base {a: 1, shared: base}
flow: {<<: *base, shared: flow}
tagged: {!!merge <<: *base, shared: tagged}
canonical: {!<tag:yaml.org,2002:merge> <<: *base, shared: canonical}
handle: {!m!merge <<: *base, shared: handle}
sequence: [<<: *base]
";

    let value: Value = saneyaml::from_str(input).expect("flow merge keys expand by default");
    for (key, expected_shared) in [
        ("flow", "flow"),
        ("tagged", "tagged"),
        ("canonical", "canonical"),
        ("handle", "handle"),
    ] {
        assert!(value[key]["<<"].is_null(), "{key} merge key removed");
        assert_eq!(value[key]["a"].as_u64(), Some(1), "{key} inherited a");
        assert_eq!(
            value[key]["shared"].as_str(),
            Some(expected_shared),
            "{key} explicit value wins"
        );
    }
    assert!(value["sequence"][0]["<<"].is_null());
    assert_eq!(value["sequence"][0]["a"].as_u64(), Some(1));
    assert_eq!(value["sequence"][0]["shared"].as_str(), Some("base"));
}

#[test]
fn default_merge_state_resets_between_no_merge_documents() {
    let input = "\
---
plain: before
---
base: &base {a: 1}
target: {<<: *base, b: 2}
---
plain: after
";
    let batch = saneyaml::parse_documents(input).expect("batch parse mixed merge stream");
    let streamed = saneyaml::DocumentStream::from_str(input)
        .expect("document stream")
        .collect::<saneyaml::Result<Vec<saneyaml::Node>>>()
        .expect("streamed mixed merge documents");

    assert_eq!(streamed, batch);
    assert_eq!(batch.len(), 3);
    let middle = top_mapping(&batch[1]);
    let target = mapping_value(middle, "target");
    let NodeValue::Mapping(entries) = &target.value else {
        panic!("expected target mapping");
    };
    assert!(entries.iter().all(|(key, _)| key.as_str() != Some("<<")));
    assert!(matches!(
        mapping_value(entries, "a").value,
        NodeValue::Number(saneyaml::Number::Integer(1))
    ));
    assert!(matches!(
        mapping_value(entries, "b").value,
        NodeValue::Number(saneyaml::Number::Integer(2))
    ));
    assert_eq!(
        mapping_value(top_mapping(&batch[0]), "plain").as_str(),
        Some("before")
    );
    assert_eq!(
        mapping_value(top_mapping(&batch[2]), "plain").as_str(),
        Some("after")
    );
}

#[test]
fn default_merge_reports_spanful_invalid_payloads() {
    let error = saneyaml::parse_str("item:\n  <<: scalar\n").expect_err("invalid merge payload");

    assert!(
        error
            .to_string()
            .contains("expected a mapping or list of mappings for merging, but found scalar"),
        "{error}"
    );
    assert_eq!(error.line(), Some(2));
    assert_eq!(error.column(), Some(7));
}

#[test]
fn default_merge_reports_spanful_invalid_list_payloads() {
    let error = saneyaml::parse_str("base: &base {a: 1}\ntarget:\n  <<: [*base, scalar]\n")
        .expect_err("invalid merge-list payload");

    assert!(
        error
            .to_string()
            .contains("expected a mapping for merging, but found scalar"),
        "{error}"
    );
    assert_eq!(error.line(), Some(3));
    assert_eq!(error.column(), Some(15));
}

#[test]
fn default_merge_rejects_duplicate_local_keys_inside_merged_mapping() {
    let error =
        saneyaml::parse_str("base: &base {a: 1}\ntarget:\n  <<: *base\n  a: local1\n  a: local2\n")
            .expect_err("duplicate local key stays rejected");

    assert!(
        error.to_string().contains("duplicate mapping key `a`"),
        "{error}"
    );
    assert_eq!(error.line(), Some(5));
    assert_eq!(error.column(), Some(3));
}

#[test]
fn merge_aliases_reset_across_documents() {
    let input = "\
---
base: &base {a: 1}
---
merged:
  <<: *base
";
    let error = saneyaml::parse_events(input).expect_err("cross-document merge alias is unknown");

    assert!(
        error.to_string().contains("unknown anchor `base`"),
        "{error}"
    );
    assert_eq!(error.line(), Some(5));
    assert_eq!(error.column(), Some(7));
}

#[test]
fn raw_events_keep_merge_key_and_alias_events() {
    let input = "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n";
    let events = saneyaml::parse_events(input).expect("raw events parse merge syntax");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Scalar { value, .. } if value == "<<")),
        "raw events should expose the merge key spelling"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Alias { anchor } if anchor.name == "defaults")),
        "raw events should expose the merge alias"
    );
}

#[test]
fn default_merge_keeps_tagged_merge_key_literal() {
    let input = "target: {!Thing <<: tagged, plain: value}\n";
    let value: Value = saneyaml::from_str(input).expect("tagged merge key stays literal");
    let target = value["target"].as_mapping().expect("target mapping");

    assert_eq!(value["target"]["plain"].as_str(), Some("value"));
    assert!(
        target.keys().any(|key| matches!(key, Value::Tagged(tagged)
            if tagged.value.as_str() == Some("<<"))),
        "tagged << key must not be default-expanded"
    );
}

#[test]
fn default_merge_keeps_explicit_string_merge_key_literal() {
    let input = "target: {!!str <<: tagged, plain: value}\n";
    let value: Value = saneyaml::from_str(input).expect("explicit string merge key stays literal");
    let target = value["target"].as_mapping().expect("target mapping");

    assert_eq!(value["target"]["plain"].as_str(), Some("value"));
    assert!(
        target.keys().any(|key| matches!(key, Value::Tagged(tagged)
            if tagged.tag == saneyaml::Tag::new("!!str")
                && tagged.value.as_str() == Some("<<"))),
        "explicit string << key must not be default-expanded"
    );
}

/// Merge-key expansion recurses through nested merge sources. Even when the
/// caller disables the parser's static nesting-depth limit, the merge traversal
/// must keep its own depth backstop so a pathological merge chain cannot exhaust
/// the stack. The error must be reported as a limit error.
#[test]
fn default_merge_rejects_unbounded_merge_chains_without_nesting_limit() {
    let input = deep_merge_chain(200);
    let options = LoadOptions::new().without_nesting_depth_limit();
    let error = options
        .parse_str(&input)
        .expect_err("deep merge chain must be rejected even without a static nesting limit");

    assert_eq!(error.category(), ErrorCategory::Limit);
    assert!(
        error
            .to_string()
            .contains("maximum YAML nesting depth exceeded"),
        "unexpected error: {error}"
    );
}

/// Shallow merge chains stay within the depth backstop and expand normally,
/// so the guard only rejects pathological inputs.
#[test]
fn default_merge_accepts_shallow_merge_chains() {
    let input = deep_merge_chain(8);
    let value: Value = saneyaml::from_str(&input).expect("shallow merge chain expands");
    assert_eq!(value["top"]["v"].as_u64(), Some(0));
}

/// Builds an `n`-deep chain of mergeable mappings whose deepest `<<` payload is
/// a *scalar* — a non-mergeable literal that YAML 1.1 preserves rather than
/// expands. `a0` is the scalar; `a1: {<<: *a0}` keeps `<<` as a literal entry.
fn deep_merge_chain_with_literal_base(n: usize) -> String {
    let mut input = String::from("a0: &a0 \"literal-merge-payload\"\n");
    for i in 1..=n {
        input.push_str(&format!("a{i}: &a{i} {{<<: *a{}}}\n", i - 1));
    }
    input.push_str(&format!("top: {{<<: *a{n}}}\n"));
    input
}

/// A non-mergeable literal `<<` payload is a leaf under YAML 1.1: it is
/// preserved as an explicit entry and does not recurse, so it must not consume
/// the merge-depth backstop. A deep mergeable chain that bottoms out in such a
/// literal therefore expands successfully — the literal does not tip the chain
/// over the depth limit — and the literal survives in the resolved tree.
#[test]
fn yaml11_merge_preserves_literal_payload_without_consuming_depth() {
    let input = deep_merge_chain_with_literal_base(128);
    let node = LoadOptions::yaml_1_1()
        .without_nesting_depth_limit()
        .parse_str(&input)
        .expect("literal merge payload must not consume the merge-depth budget");

    let root = top_mapping(&node);
    let top = mapping_value(root, "top");
    let NodeValue::Mapping(entries) = &top.value else {
        panic!("expected top mapping");
    };
    let merge_literal = entries
        .iter()
        .find_map(|(key, value)| (key.as_str() == Some("<<")).then_some(value))
        .expect("literal merge key is preserved verbatim");
    assert_eq!(merge_literal.as_str(), Some("literal-merge-payload"));
}

/// Merged entries are appended in source order after the explicit entries, and
/// keys already present in the target are never reordered or duplicated. This
/// documents the deterministic, source-stable insertion order that lossless
/// round-trips depend on.
#[test]
fn default_merge_preserves_deterministic_source_order() {
    let input = "\
base: &base {alpha: base, beta: base, gamma: base}
target:
  beta: local
  <<: *base
  delta: local
";

    let node = saneyaml::parse_str(input).expect("parse deterministic-order merge tree");
    let root = top_mapping(&node);
    let target = mapping_value(root, "target");
    let NodeValue::Mapping(entries) = &target.value else {
        panic!("expected target mapping");
    };

    let keys: Vec<&str> = entries
        .iter()
        .map(|(key, _)| key.as_str().expect("string key"))
        .collect();
    // Explicit entries keep their source order; the merge source only
    // contributes keys missing from the target, appended in source order.
    assert_eq!(keys, ["beta", "delta", "alpha", "gamma"]);

    // Explicit values win over merged values for shared keys.
    assert_eq!(mapping_value(entries, "beta").as_str(), Some("local"));
    assert_eq!(mapping_value(entries, "alpha").as_str(), Some("base"));
}
