use super::*;
use serde::{Deserialize, de::IgnoredAny};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read};

struct FailingAfterPrefixReader {
    prefix: Cursor<Vec<u8>>,
}

impl FailingAfterPrefixReader {
    fn new(prefix: &[u8]) -> Self {
        Self {
            prefix: Cursor::new(prefix.to_vec()),
        }
    }
}

impl Read for FailingAfterPrefixReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.prefix.read(buf)?;
        if read == 0 {
            Err(io::Error::other("stream interrupted"))
        } else {
            Ok(read)
        }
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct EventConfig<'a> {
    name: &'a str,
    ports: Vec<u16>,
    enabled: bool,
    labels: BTreeMap<String, String>,
    optional: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OwnedEventConfig {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
    labels: BTreeMap<String, String>,
    optional: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ExplicitCoreScalars {
    string_null: String,
    optional_string_null: Option<String>,
    string_bool: String,
    yes: bool,
    off: bool,
    maybe: Option<String>,
    unit: (),
}

#[derive(Debug, Deserialize, PartialEq)]
struct ExplicitCoreNumbers {
    integer: i64,
    unsigned: u64,
    float: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TargetMap {
    target: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TargetValueMap {
    target: BTreeMap<String, crate::Value>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct KnownOnly {
    name: String,
}

fn assert_value_tagged_key(
    mapping: &crate::Value,
    expected_tag: crate::Tag,
    expected_key: &str,
    expected_value: &str,
) {
    let mapping = mapping.as_mapping().expect("mapping value");
    assert!(
        mapping.iter().any(|(key, value)| {
            matches!(key, crate::Value::Tagged(tagged)
                    if tagged.tag == expected_tag
                        && tagged.value.as_str() == Some(expected_key)
                        && value.as_str() == Some(expected_value))
        }),
        "expected tagged key {expected_tag:?} {expected_key:?}: {expected_value:?}"
    );
}

#[test]
fn event_deserializer_reads_typed_structs() {
    let input = "\
name: api
ports: [80, 443]
enabled: true
labels:
  tier: backend
  release: stable
optional: null
";

    let parsed: EventConfig<'_> =
        from_str_with_options(input, LoadOptions::new()).expect("event-backed typed config");
    assert_eq!(parsed.name, "api");
    assert!(std::ptr::eq(parsed.name.as_ptr(), input[6..9].as_ptr()));
    assert_eq!(parsed.ports, vec![80, 443]);
    assert!(parsed.enabled);
    assert_eq!(parsed.labels["tier"], "backend");
    assert_eq!(parsed.labels["release"], "stable");
    assert_eq!(parsed.optional, None);
}

#[test]
fn event_deserializer_rejects_duplicate_scalar_keys() {
    let input = "labels:\n  tier: backend\n  tier: worker\n";
    let error = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
        input,
        LoadOptions::new(),
    )
    .expect_err("event-backed duplicate keys reject");
    assert!(error.to_string().contains("duplicate mapping key"));
}

#[test]
fn event_deserializer_rejects_duplicate_sequence_alias_mapping_keys() {
    let input = "seq: &seq [a, b]\nroot: {? *seq : first, ? [a, b] : second}\n";
    let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
        .expect_err("event-backed alias-expanded sequence keys reject");

    assert!(error.to_string().contains("duplicate mapping key"));
}

#[test]
fn event_deserializer_rejects_duplicate_mapping_alias_keys_order_insensitively() {
    let input = "base: &base {a: 1, b: 2}\nroot: {? *base : first, ? {b: 2, a: 1} : second}\n";
    let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
        .expect_err("event-backed alias-expanded mapping keys reject");

    assert!(error.to_string().contains("duplicate mapping key"));
}

#[test]
fn event_deserializer_accepts_distinct_complex_alias_mapping_keys() {
    let input = "seq: &seq [a, b]\nroot: {? *seq : first, ? [a, c] : second}\n";

    from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
        .expect("distinct complex alias keys pass duplicate preflight");
}

#[test]
fn event_deserializer_rejects_recursive_alias_mapping_keys() {
    let input = "root: {? &self [*self] : value}\n";
    let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
        .expect_err("recursive alias key rejects");

    assert!(error.to_string().contains("recursive alias"));
}

#[test]
fn event_deserializer_rejects_complex_alias_mapping_keys_over_budget() {
    let input = "seq: &seq [a, b]\nroot: {? *seq : first}\n";
    let error =
        from_str_with_options::<IgnoredAny>(input, LoadOptions::new().max_alias_expansion_nodes(1))
            .expect_err("complex alias key replay budget rejects");

    assert!(
        error
            .to_string()
            .contains("alias event replay limit exceeded")
    );
}

#[test]
fn event_deserializer_expands_merge_keys() {
    let input = "\
base: &base
  retries: 3
  command: deploy
target:
  <<: *base
  command: smoke
";
    let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("merge keys");

    assert_eq!(parsed.target["retries"], "3");
    assert_eq!(parsed.target["command"], "smoke");
}

#[test]
fn event_deserializer_expands_merge_lists_with_earlier_sources_winning() {
    let input = "\
base1: &base1 {a: one, shared: first}
base2: &base2 {b: two, shared: second}
target: {<<: [*base1, *base2], local: ok}
";
    let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("merge list");

    assert_eq!(parsed.target["a"], "one");
    assert_eq!(parsed.target["b"], "two");
    assert_eq!(parsed.target["shared"], "first");
    assert_eq!(parsed.target["local"], "ok");
}

#[test]
fn event_deserializer_expands_explicit_merge_tag_keys() {
    let input = "\
%TAG !m! tag:yaml.org,2002:
---
base: &base {a: one, shared: base}
tagged: {!!merge <<: *base, shared: tagged}
canonical: {!<tag:yaml.org,2002:merge> <<: *base, shared: canonical}
handle: {!m!merge <<: *base, shared: handle}
";
    let parsed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
        input,
        LoadOptions::new(),
    )
    .expect("explicit merge tag keys");

    for (key, expected_shared) in [
        ("tagged", "tagged"),
        ("canonical", "canonical"),
        ("handle", "handle"),
    ] {
        assert_eq!(parsed[key]["a"], "one");
        assert_eq!(parsed[key]["shared"], expected_shared);
    }
}

#[test]
fn event_deserializer_keeps_explicit_string_merge_key_literal() {
    let input = "base: &base {!!str <<: literal, a: one}\ntarget: {<<: *base}\n";
    let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::new())
        .expect("explicit string merge key stays literal");

    assert_eq!(parsed.target["a"], "one");
    assert_eq!(parsed.target["<<"], "literal");
}

#[test]
fn event_deserializer_reports_invalid_merge_payloads() {
    let input = "target: {<<: scalar}\n";
    let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
        .expect_err("invalid merge payload rejects");

    assert!(
        error
            .to_string()
            .contains("expected a mapping or list of mappings for merging"),
        "{error}"
    );
}

#[test]
fn event_deserializer_skips_valid_merge_maps_for_ignored_values() {
    let input = "base: &base {a: one}\nname: app\nignored: {<<: *base, b: two}\n";
    let parsed = from_str_with_options::<KnownOnly>(input, LoadOptions::new())
        .expect("unknown merge-bearing field is skipped");

    assert_eq!(parsed.name, "app");
    from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
        .expect("ignored-any skips merge-bearing maps");
}

#[test]
fn event_deserializer_rejects_invalid_merge_payloads_in_ignored_values() {
    let input = "name: app\nignored: {<<: scalar}\n";
    let error = from_str_with_options::<KnownOnly>(input, LoadOptions::new())
        .expect_err("strict invalid merge payload rejects while skipping");

    assert!(
        error
            .to_string()
            .contains("expected a mapping or list of mappings for merging"),
        "{error}"
    );
}

#[test]
fn event_deserializer_yaml11_skips_literal_merge_payload_in_ignored_value() {
    let input = "%YAML 1.1\n---\nname: app\nignored: {<<: scalar, keep: value}\n";
    let parsed = from_str_with_options::<KnownOnly>(input, LoadOptions::yaml_version_directive())
        .expect("directive-driven YAML 1.1 literal merge payload is skipped");

    assert_eq!(parsed.name, "app");
}

#[test]
fn event_deserializer_rejects_repeated_merge_keys_by_default() {
    let input = "\
first: &first {shared: first}
second: &second {shared: second}
target:
  <<: *first
  !!merge <<: *second
";
    let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
        .expect_err("default repeated merge keys reject");

    assert!(error.to_string().contains("duplicate mapping key `<<`"));
}

#[test]
fn event_deserializer_yaml11_recovers_repeated_merge_keys() {
    let input = "\
first: &first {shared: first, retries: 3}
second: &second {shared: second, timeout: 10}
target:
  <<: *first
  !<tag:yaml.org,2002:merge> <<: *second
  keep: value
";
    let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::yaml_1_1())
        .expect("YAML 1.1 repeated merge keys recover");

    assert_eq!(parsed.target["shared"], "second");
    assert_eq!(parsed.target["retries"], "3");
    assert_eq!(parsed.target["timeout"], "10");
    assert_eq!(parsed.target["keep"], "value");
}

#[test]
fn event_deserializer_yaml11_keeps_scalar_merge_payload_literal() {
    let input = "\
target:
  <<: scalar
  keep: value
";
    let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::yaml_1_1())
        .expect("YAML 1.1 scalar merge payload stays literal");

    assert_eq!(parsed.target["<<"], "scalar");
    assert_eq!(parsed.target["keep"], "value");
}

#[test]
fn event_deserializer_yaml11_keeps_mixed_invalid_merge_list_literal() {
    let input = "\
base: &base {a: 1}
target:
  <<: [*base, scalar]
  keep: value
";
    let parsed = from_str_with_options::<TargetValueMap>(input, LoadOptions::yaml_1_1())
        .expect("YAML 1.1 mixed invalid merge list stays literal");
    let merge = &parsed.target["<<"];
    let merge = merge.as_sequence().expect("literal merge list");

    assert_eq!(merge[0]["a"].as_u64(), Some(1));
    assert_eq!(merge[1].as_str(), Some("scalar"));
    assert_eq!(parsed.target["keep"].as_str(), Some("value"));
}

#[test]
fn event_deserializer_reads_explicit_core_scalar_tags() {
    let input = "\
string_null: !!str null
optional_string_null: !!str null
string_bool: !!str true
yes: !!bool YES
off: !!bool off
maybe: !!null null
unit: !!null ~
";
    let parsed = from_str_with_options::<ExplicitCoreScalars>(input, LoadOptions::new()).unwrap();

    assert_eq!(
        parsed,
        ExplicitCoreScalars {
            string_null: "null".to_string(),
            optional_string_null: Some("null".to_string()),
            string_bool: "true".to_string(),
            yes: true,
            off: false,
            maybe: None,
            unit: (),
        }
    );
}

#[test]
fn event_deserializer_reads_explicit_core_numeric_tags() {
    let input = "integer: !!int \"42\"\nunsigned: !!int 0x2A\nfloat: !!float \"1.5\"\n";
    let parsed = from_str_with_options::<ExplicitCoreNumbers>(input, LoadOptions::new()).unwrap();

    assert_eq!(
        parsed,
        ExplicitCoreNumbers {
            integer: 42,
            unsigned: 42,
            float: 1.5,
        }
    );
}

#[test]
fn event_deserializer_explicit_tags_follow_directive_schema() {
    let parsed = from_str_with_options::<bool>(
        "%YAML 1.1\n--- !!bool YES\n",
        LoadOptions::yaml_version_directive(),
    )
    .expect("directive-driven explicit bool");

    assert!(parsed);
}

#[test]
fn event_deserializer_rejects_invalid_explicit_core_scalar_tags() {
    let bool_error = from_str_with_options::<bool>("!!bool maybe\n", LoadOptions::new())
        .expect_err("invalid explicit bool");
    assert!(
        bool_error
            .to_string()
            .contains("failed to parse explicit !!bool scalar"),
        "{bool_error}"
    );

    let str_error = from_str_with_options::<i64>("!!str 7\n", LoadOptions::new())
        .expect_err("explicit string does not coerce to integer");
    assert!(str_error.to_string().contains("expected integer"));
}

#[test]
fn event_deserializer_retains_tagged_scalars_for_value_and_unwraps_typed_strings() {
    let value = from_str_with_options::<crate::Value>("!Thing tagged\n", LoadOptions::new())
        .expect("custom tagged scalar value");
    let tagged = value.as_tagged().expect("custom tag retained");

    assert_eq!(tagged.tag, crate::Tag::new("Thing"));
    assert_eq!(tagged.value.as_str(), Some("tagged"));

    let typed = from_str_with_options::<String>("!Thing tagged\n", LoadOptions::new())
        .expect("typed string unwraps custom tag");
    assert_eq!(typed, "tagged");

    let explicit = from_str_with_options::<crate::Value>("!!str null\n", LoadOptions::new())
        .expect("explicit core string tag value");
    let tagged = explicit.as_tagged().expect("explicit core tag retained");
    assert_eq!(tagged.tag, crate::Tag::new("!!str"));
    assert_eq!(tagged.value.as_str(), Some("null"));
}

#[test]
fn event_deserializer_retains_tagged_collections_for_value_and_unwraps_typed_targets() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct TaggedCollections {
        seq: Vec<String>,
        map: BTreeMap<String, String>,
    }

    let input = "seq: !Seq [a, b]\nmap: !Map {k: v}\n";
    let value = from_str_with_options::<crate::Value>(input, LoadOptions::new()).expect("value");

    let sequence = value["seq"].as_tagged().expect("sequence tag retained");
    assert_eq!(sequence.tag, crate::Tag::new("Seq"));
    assert_eq!(
        sequence
            .value
            .as_sequence()
            .expect("sequence payload")
            .len(),
        2
    );
    assert_eq!(sequence.value[0].as_str(), Some("a"));
    assert_eq!(sequence.value[1].as_str(), Some("b"));

    let mapping = value["map"].as_tagged().expect("mapping tag retained");
    assert_eq!(mapping.tag, crate::Tag::new("Map"));
    assert_eq!(mapping.value["k"].as_str(), Some("v"));

    let typed = from_str_with_options::<TaggedCollections>(input, LoadOptions::new())
        .expect("typed collections unwrap tags");
    assert_eq!(
        typed,
        TaggedCollections {
            seq: vec!["a".to_string(), "b".to_string()],
            map: BTreeMap::from([("k".to_string(), "v".to_string())]),
        }
    );

    let top_value = from_str_with_options::<crate::Value>("!Seq [a, b]\n", LoadOptions::new())
        .expect("top-level tagged sequence value");
    let tagged = top_value.as_tagged().expect("top-level tag retained");
    assert_eq!(tagged.tag, crate::Tag::new("Seq"));
    assert_eq!(tagged.value[1].as_str(), Some("b"));

    let top_typed = from_str_with_options::<Vec<String>>("!Seq [a, b]\n", LoadOptions::new())
        .expect("top-level typed sequence unwraps tag");
    assert_eq!(top_typed, ["a", "b"]);
}

#[test]
fn event_deserializer_projects_yaml11_collection_tags_for_typed_targets() {
    let set =
        from_str_with_options::<BTreeSet<String>>("!!set\n? alpha\n? beta\n", LoadOptions::new())
            .expect("typed !!set");
    assert_eq!(
        set,
        BTreeSet::from(["alpha".to_string(), "beta".to_string()])
    );

    let omap_pairs = from_str_with_options::<Vec<(String, i64)>>(
        "!!omap\n- first: 1\n- second: 2\n",
        LoadOptions::new(),
    )
    .expect("typed !!omap pair sequence");
    assert_eq!(
        omap_pairs,
        vec![("first".to_string(), 1), ("second".to_string(), 2)]
    );

    let omap_map = from_str_with_options::<BTreeMap<String, i64>>(
        "!!omap\n- second: 2\n- first: 1\n",
        LoadOptions::new(),
    )
    .expect("typed !!omap map");
    assert_eq!(
        omap_map,
        BTreeMap::from([("first".to_string(), 1), ("second".to_string(), 2)])
    );

    let pairs = from_str_with_options::<Vec<(String, i64)>>(
        "!!pairs\n- repeat: 1\n- repeat: 2\n",
        LoadOptions::new(),
    )
    .expect("typed !!pairs preserves duplicate keys");
    assert_eq!(
        pairs,
        vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)]
    );
}

#[test]
fn event_deserializer_rejects_lossy_yaml11_collection_tag_shapes() {
    let duplicate = from_str_with_options::<BTreeMap<String, i64>>(
        "!!omap\n- z: 1\n- a: 2\n- z: 3\n",
        LoadOptions::new(),
    )
    .expect_err("typed !!omap map rejects duplicate keys");
    assert!(duplicate.to_string().contains("duplicate mapping key `z`"));

    let set_error =
        from_str_with_options::<BTreeSet<String>>("!!set {alpha: true}\n", LoadOptions::new())
            .expect_err("typed !!set rejects non-null values");
    assert!(
        set_error
            .to_string()
            .contains("expected explicit !!set entry value to be null"),
        "{set_error}"
    );

    let omap_error =
        from_str_with_options::<Vec<(String, i64)>>("!!omap\n- {a: 1, b: 2}\n", LoadOptions::new())
            .expect_err("typed !!omap rejects multi-pair entries");
    assert!(
        omap_error
            .to_string()
            .contains("expected explicit !!omap entry to contain exactly one pair"),
        "{omap_error}"
    );

    let pairs_error =
        from_str_with_options::<Vec<(String, i64)>>("!!pairs\n- scalar\n", LoadOptions::new())
            .expect_err("typed !!pairs rejects scalar entries");
    assert!(
        pairs_error
            .to_string()
            .contains("expected single-pair mapping entry for explicit !!pairs"),
        "{pairs_error}"
    );
}

#[test]
fn event_deserializer_retains_tagged_merge_maps_for_value_and_unwraps_typed_targets() {
    let input = "base: &base {a: one}\ntarget: !Thing {<<: *base, b: two}\n";
    let value = from_str_with_options::<crate::Value>(input, LoadOptions::new())
        .expect("tagged merge map value");
    let tagged = value["target"].as_tagged().expect("target tag retained");

    assert_eq!(tagged.tag, crate::Tag::new("Thing"));
    assert_eq!(tagged.value["a"].as_str(), Some("one"));
    assert_eq!(tagged.value["b"].as_str(), Some("two"));

    let typed = from_str_with_options::<TargetMap>(input, LoadOptions::new())
        .expect("typed tagged merge map unwraps tag");
    assert_eq!(typed.target["a"], "one");
    assert_eq!(typed.target["b"], "two");
}

#[test]
fn event_deserializer_retains_tagged_literal_merge_keys_without_expansion() {
    let input = "\
custom: {!Thing <<: literal, image: app:custom}
string: {!!str <<: literal, image: app:string}
";
    let value =
        from_str_with_options::<crate::Value>(input, LoadOptions::new()).expect("tagged keys");

    assert_value_tagged_key(&value["custom"], crate::Tag::new("Thing"), "<<", "literal");
    assert_value_tagged_key(&value["string"], crate::Tag::new("!!str"), "<<", "literal");
    assert_eq!(value["custom"]["image"].as_str(), Some("app:custom"));
    assert_eq!(value["string"]["image"].as_str(), Some("app:string"));

    let typed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
        input,
        LoadOptions::new(),
    )
    .expect("typed maps unwrap tagged literal keys");
    assert_eq!(typed["custom"]["<<"], "literal");
    assert_eq!(typed["string"]["<<"], "literal");
}

#[test]
fn event_deserializer_replays_acyclic_scalar_aliases() {
    let input = "base: &base api\nservice: *base\n";
    let parsed = from_str_with_options::<BTreeMap<String, String>>(input, LoadOptions::new())
        .expect("event-backed scalar alias replay");

    assert_eq!(parsed["base"], "api");
    assert_eq!(parsed["service"], "api");
}

#[test]
fn event_deserializer_replays_acyclic_sequence_aliases() {
    let input = "base: &base [api, worker]\nservice: *base\n";
    let parsed = from_str_with_options::<BTreeMap<String, Vec<String>>>(input, LoadOptions::new())
        .expect("event-backed sequence alias replay");

    assert_eq!(parsed["base"], ["api", "worker"]);
    assert_eq!(parsed["service"], ["api", "worker"]);
}

#[test]
fn event_deserializer_validates_alias_expanded_mapping_values() {
    let input = "base: &base {a: one, b: two}\ntarget: *base\n";
    let parsed =
        from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("mapping alias");

    assert_eq!(parsed.target["a"], "one");
    assert_eq!(parsed.target["b"], "two");
}

#[test]
fn event_deserializer_replays_scalar_alias_mapping_keys() {
    let input = "root: {anchor: &svc service, ? *svc : api}\n";
    let parsed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
        input,
        LoadOptions::new(),
    )
    .expect("event-backed scalar alias mapping key replay");

    assert_eq!(parsed["root"]["anchor"], "service");
    assert_eq!(parsed["root"]["service"], "api");
}

#[test]
fn event_deserializer_rejects_duplicate_alias_mapping_keys() {
    let input = "root: {? &name name : api, ? *name : worker}\n";
    let error = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
        input,
        LoadOptions::new(),
    )
    .expect_err("event-backed alias-expanded duplicate keys reject");
    assert!(error.to_string().contains("duplicate mapping key"));
}

#[test]
fn event_deserializer_rejects_alias_replay_over_budget() {
    let input = "base: &base api\nservice: *base\n";
    let error = from_str_with_options::<BTreeMap<String, String>>(
        input,
        LoadOptions::new().max_alias_expansion_nodes(0),
    )
    .expect_err("event-backed alias replay budget rejects");

    assert!(
        error
            .to_string()
            .contains("alias event replay limit exceeded")
    );
}

#[test]
fn event_deserializer_rejects_duplicate_keys_in_ignored_mappings() {
    let input = "base: &base {a: one, a: two}\ntarget: *base\n";
    let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
        .expect_err("ignored anchor source duplicate keys reject");

    assert!(error.to_string().contains("duplicate mapping key"));
}

#[test]
fn event_deserializer_reads_multiple_documents() {
    let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [8080]\nenabled: false\nlabels:\n  tier: job\noptional: note\n";
    let parsed: Vec<OwnedEventConfig> = from_documents_str_with_options(input, LoadOptions::new())
        .expect("event-backed document stream");

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "api");
    assert_eq!(parsed[0].ports, vec![80]);
    assert_eq!(parsed[1].name, "worker");
    assert_eq!(parsed[1].ports, vec![8080]);
    assert_eq!(parsed[1].labels["tier"], "job");
    assert_eq!(parsed[1].optional.as_deref(), Some("note"));
}

#[test]
fn event_document_iterator_yields_borrowed_typed_documents() {
    let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [8080]\nenabled: false\nlabels: {}\noptional: null\n";
    let mut iter = document_iter_str_with_options::<EventConfig<'_>>(input, LoadOptions::new())
        .expect("event-backed document iterator");

    let first = iter.next().expect("first document").expect("first parses");
    assert_eq!(first.name, "api");
    assert!(std::ptr::eq(first.name.as_ptr(), input[10..13].as_ptr()));

    let second = iter
        .next()
        .expect("second document")
        .expect("second parses");
    assert_eq!(second.name, "worker");
    let worker_offset = input.find("worker").expect("worker text in input");
    assert!(std::ptr::eq(
        second.name.as_ptr(),
        input[worker_offset..worker_offset + "worker".len()].as_ptr()
    ));
    assert!(iter.next().is_none());
}

#[test]
fn event_document_iterator_continues_after_typed_document_error() {
    let input = "\
---
name: api
ports: [80]
enabled: true
labels: {}
optional: null
---
name: bad
ports: [70000]
enabled: true
labels: {}
optional: null
---
name: worker
ports: [8080]
enabled: false
labels: {}
optional: null
";
    let mut iter = document_iter_str_with_options::<OwnedEventConfig>(input, LoadOptions::new())
        .expect("event-backed document iterator");

    let first = iter.next().expect("first document").expect("first parses");
    assert_eq!(first.name, "api");

    let error = iter
        .next()
        .expect("second document")
        .expect_err("second document has typed range error");
    assert_eq!(error.document_index(), Some(1));
    assert!(error.to_string().contains("70000"), "{error}");

    let third = iter.next().expect("third document").expect("third parses");
    assert_eq!(third.name, "worker");
    assert!(iter.next().is_none());
}

#[test]
fn event_document_iterator_defers_later_parse_error_and_then_stops() {
    let input = "---\nname: one\n---\n:\tbad\n---\nname: never\n";
    let mut iter = document_iter_str_with_options::<KnownOnly>(input, LoadOptions::new())
        .expect("event-backed document iterator");

    let first = iter.next().expect("first document").expect("first parses");
    assert_eq!(first.name, "one");

    let error = iter
        .next()
        .expect("second document item")
        .expect_err("later parser error");
    assert_eq!(error.document_index(), Some(1));
    assert_eq!(error.line(), Some(4));
    assert_eq!(error.column(), Some(2));
    assert!(iter.next().is_none());
}

#[test]
fn event_document_iterator_empty_stream_yields_no_documents() {
    let mut iter = document_iter_str_with_options::<crate::Value>("", LoadOptions::new())
        .expect("empty event-backed document iterator");

    assert!(iter.next().is_none());
    let collected = from_documents_str_with_options::<crate::Value>("", LoadOptions::new())
        .expect("empty document collection");
    assert!(collected.is_empty());
}

#[test]
fn event_document_iterator_slice_checks_utf8_and_input_limits() {
    let invalid =
        match document_iter_slice_with_options::<crate::Value>(b"name: \xFF\n", LoadOptions::new())
        {
            Ok(_) => panic!("invalid UTF-8 should fail"),
            Err(error) => error,
        };
    assert!(invalid.to_string().contains("input is not valid UTF-8"));

    let limited = match document_iter_slice_with_options::<crate::Value>(
        b"name: app\n",
        LoadOptions::new().max_input_bytes(4),
    ) {
        Ok(_) => panic!("input limit should fail"),
        Err(error) => error,
    };
    assert!(
        limited
            .to_string()
            .contains("YAML input exceeds configured limit of 4 bytes")
    );
}

#[test]
fn event_document_reader_iterator_uses_owned_input_and_preserves_merge_alias_semantics() {
    let input = "\
---
base: &base {a: one}
target: {<<: *base, b: two}
---
base: &base {a: three}
target: *base
";
    let docs = document_iter_reader_with_options::<TargetMap, _>(
        Cursor::new(input.as_bytes()),
        LoadOptions::new(),
    )
    .expect("reader-backed event iterator")
    .collect::<Result<Vec<_>>>()
    .expect("reader-backed documents");

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].target["a"], "one");
    assert_eq!(docs[0].target["b"], "two");
    assert_eq!(docs[1].target["a"], "three");
}

#[test]
fn event_document_reader_iterator_reports_read_errors_before_iteration() {
    let error = match document_iter_reader_with_options::<OwnedEventConfig, _>(
        FailingAfterPrefixReader::new(b"name: api\n"),
        LoadOptions::new(),
    ) {
        Ok(_) => panic!("reader failure should reject iterator construction"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("failed to read YAML input"));
    assert_eq!(error.location(), None);
}

#[test]
fn event_deserializer_document_errors_carry_document_index() {
    let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [70000]\nenabled: true\nlabels: {}\noptional: null\n";
    let error = from_documents_str_with_options::<OwnedEventConfig>(input, LoadOptions::new())
        .expect_err("event-backed stream reports second document error");
    assert_eq!(error.document_index(), Some(1));
}

#[test]
fn event_deserializer_skips_ignored_any_without_materializing_values() {
    let input = "root:\n  - name: api\n    ports: [80, 443]\n  - nested:\n      ok: true\n";
    IgnoredAny::deserialize(EventNodeDeserializer {
        source: &mut EventSource::new(
            input,
            crate::parse::EventStream::from_str(input)
                .expect("event stream")
                .collect::<Result<Vec<_>>>()
                .expect("events"),
            Schema::Yaml12,
            LoadOptions::new().alias_expansion_budget(input.len()),
            LoadOptions::new().selected_max_nesting_depth(),
        ),
    })
    .expect_err("raw stream markers must still be explicit");

    from_str_with_options::<IgnoredAny>(input, LoadOptions::new()).expect("ignored any");
}

fn alias_depth_chain(levels: usize) -> String {
    // A literally shallow document (max nesting depth 2) whose final anchor
    // expands, via the alias chain, to a structure `levels` deep.
    let mut input = String::from("- &n0 0\n");
    for k in 1..levels {
        input.push_str(&format!("- &n{k} [*n{prev}]\n", prev = k - 1));
    }
    input
}

#[test]
fn event_deserializer_bounds_alias_expansion_depth() {
    // The event-backed path expands aliases lazily while walking, so the
    // parser's literal-depth check does not bound the expanded depth. Without
    // an explicit ceiling this recurses until the stack overflows; it must
    // instead reject, matching the tree-backed `AnchorTable::resolve` guard.
    let input = alias_depth_chain(400);
    let error = from_str_with_options::<Vec<crate::Value>>(&input, LoadOptions::new())
        .expect_err("deep alias chain must hit the nesting-depth ceiling");
    assert!(
        error.to_string().contains("nesting depth"),
        "unexpected error: {error}"
    );
}

#[test]
fn event_deserializer_allows_alias_chain_within_depth_limit() {
    let input = alias_depth_chain(8);
    let parsed = from_str_with_options::<Vec<crate::Value>>(&input, LoadOptions::new())
        .expect("alias chain within the depth limit deserializes");
    assert_eq!(parsed.len(), 8);
}

#[test]
fn event_deserializer_reads_map_form_enum_variants() {
    // Externally-tagged enum variants carrying a payload — the forms the
    // earlier scalar-only path rejected. Covers unit, newtype, tuple, and
    // struct variants in one sequence.
    #[derive(Debug, Deserialize, PartialEq)]
    enum EventEnum {
        Unit,
        Newtype(u32),
        Tuple(u8, u8),
        Struct { width: u32, height: u32 },
    }

    let input = "\
- Unit
- Newtype: 7
- Tuple: [1, 2]
- Struct:
    width: 3
    height: 4
";
    let parsed: Vec<EventEnum> =
        from_str_with_options(input, LoadOptions::new()).expect("event-backed enum variants");
    assert_eq!(
        parsed,
        vec![
            EventEnum::Unit,
            EventEnum::Newtype(7),
            EventEnum::Tuple(1, 2),
            EventEnum::Struct {
                width: 3,
                height: 4,
            },
        ]
    );
}

#[test]
fn event_deserializer_reads_map_form_enum_variant_through_alias() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Mode {
        Tuned { level: u8 },
    }

    // The anchored definition and the alias must both resolve to the same
    // map-form variant.
    let parsed =
        from_str_with_options::<Vec<Mode>>("- &m {Tuned: {level: 9}}\n- *m\n", LoadOptions::new())
            .expect("aliased map-form enum variant");
    assert_eq!(
        parsed,
        vec![Mode::Tuned { level: 9 }, Mode::Tuned { level: 9 }]
    );
}
