use yaml::{Event, NodeValue, Value};

fn top_mapping(node: &yaml::Node) -> &[(yaml::Node, yaml::Node)] {
    let NodeValue::Mapping(entries) = &node.value else {
        panic!("expected top-level mapping");
    };
    entries
}

fn mapping_value<'a>(entries: &'a [(yaml::Node, yaml::Node)], key: &str) -> &'a yaml::Node {
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

    let node = yaml::parse_str(input).expect("parse default merge tree");
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
        NodeValue::Number(yaml::Number::Integer(3))
    ));
    assert_eq!(
        mapping_value(job_entries, "command").as_str(),
        Some("smoke")
    );

    let mut value: Value = yaml::from_str(input).expect("value read applies merge by default");
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

    let value: Value = yaml::from_str(input).expect("merge list expands by default");
    assert!(value["merged"]["<<"].is_null());
    assert_eq!(value["merged"]["a"].as_u64(), Some(1));
    assert_eq!(value["merged"]["b"].as_str(), Some("explicit"));
    assert_eq!(value["merged"]["c"].as_u64(), Some(2));
    assert_eq!(value["merged"]["shared"].as_str(), Some("first"));
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

    let node = yaml::parse_str(input).expect("parse explicit merge-tag keys");
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
            NodeValue::Number(yaml::Number::Integer(1))
        ));
        assert_eq!(mapping_value(entries, "b").as_str(), Some(expected_b));
    }

    let value: Value = yaml::from_str(input).expect("value read expands explicit merge tags");
    assert!(value["tagged"]["<<"].is_null());
    assert_eq!(value["tagged"]["a"].as_u64(), Some(1));
    assert_eq!(value["tagged"]["b"].as_str(), Some("tagged"));
    assert!(value["canonical"]["<<"].is_null());
    assert_eq!(value["canonical"]["a"].as_u64(), Some(1));
    assert_eq!(value["canonical"]["b"].as_str(), Some("canonical"));
}

#[test]
fn default_merge_reports_spanful_invalid_payloads() {
    let error = yaml::parse_str("item:\n  <<: scalar\n").expect_err("invalid merge payload");

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
fn raw_events_keep_merge_key_and_alias_events() {
    let input = "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n";
    let events = yaml::parse_events(input).expect("raw events parse merge syntax");

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
    let value: Value = yaml::from_str(input).expect("tagged merge key stays literal");
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
    let value: Value = yaml::from_str(input).expect("explicit string merge key stays literal");
    let target = value["target"].as_mapping().expect("target mapping");

    assert_eq!(value["target"]["plain"].as_str(), Some("value"));
    assert!(
        target.keys().any(|key| matches!(key, Value::Tagged(tagged)
            if tagged.tag == yaml::Tag::new("!!str")
                && tagged.value.as_str() == Some("<<"))),
        "explicit string << key must not be default-expanded"
    );
}
