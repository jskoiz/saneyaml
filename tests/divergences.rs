use saphyr::LoadableYamlNode;
use yaml::{LoadOptions, parse_lossless, parse_str};

#[test]
fn divergence_yaml_1_1_boolean_words_are_strings() {
    let node = parse_str("on: off\nyes: no\nY: N\n").expect("parse");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("off"));
    assert_eq!(entries[1].0.as_str(), Some("yes"));
    assert_eq!(entries[1].1.as_str(), Some("no"));
    assert_eq!(entries[2].0.as_str(), Some("Y"));
    assert_eq!(entries[2].1.as_str(), Some("N"));
}

#[test]
fn divergence_yaml_1_1_version_directive_switching_requires_explicit_option() {
    let default =
        parse_str("%YAML 1.1\n---\nflag: ON\n").expect("default entrypoint accepts directive");
    let yaml::NodeValue::Mapping(entries) = default.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].1.as_str(), Some("ON"));

    let directive = LoadOptions::yaml_version_directive()
        .parse_str("%YAML 1.1\n---\nflag: ON\n")
        .expect("directive-driven option follows YAML version");
    let yaml::NodeValue::Mapping(entries) = directive.value else {
        panic!("expected directive-driven mapping");
    };
    assert!(matches!(entries[0].1.value, yaml::NodeValue::Bool(true)));
}

#[test]
fn divergence_merge_keys_expand_by_default_in_loaded_trees() {
    let node =
        parse_str("defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n")
            .expect("merge key expands by default");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[1].1.value else {
        panic!("expected job mapping");
    };
    assert!(job.iter().all(|(key, _)| key.as_str() != Some("<<")));
    assert_eq!(job[0].0.as_str(), Some("name"));
    assert!(
        job.iter()
            .any(|(key, value)| key.as_str() == Some("retries")
                && matches!(
                    value.value,
                    yaml::NodeValue::Number(yaml::Number::Integer(3))
                )),
        "merge key expands into job.retries"
    );
}

#[test]
fn divergence_merge_list_expands_by_default_in_loaded_trees() {
    let node = parse_str(
        "base1: &base1 {a: 1, b: 1, shared: first}\nbase2: &base2 {b: 2, c: 2, shared: second}\nmerged:\n  <<: [*base1, *base2]\n  b: explicit\n",
    )
    .expect("merge-list key expands by default");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(merged) = &entries[2].1.value else {
        panic!("expected merged mapping");
    };
    assert!(merged.iter().all(|(key, _)| key.as_str() != Some("<<")));
    assert_eq!(merged[0].0.as_str(), Some("b"));
    assert_eq!(merged[0].1.as_str(), Some("explicit"));
    assert!(
        merged
            .iter()
            .any(|(key, value)| key.as_str() == Some("shared") && value.as_str() == Some("first")),
        "earlier merge-list mappings win"
    );
}

#[test]
fn divergence_merge_key_record_documents_default_and_opt_in_policy() {
    let record = include_str!("fixtures/divergences/records/merge-keys.toml");
    assert!(record.contains("expand untagged block, flow, merge-list, and explicit"));
    assert!(record.contains("Value::apply_merge() remains available"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("earlier merge-list mappings"));
    assert!(record.contains("explicit !!merge"));
    assert!(record.contains("custom-tagged << keys remain literal"));
    assert!(record.contains("explicit target keys override"));
    assert!(record.contains("non-mergeable scalar/list merge payloads literal"));
    assert!(record.contains("repeated << keys as cumulative merge entries"));
    assert!(record.contains("strict diagnostics for invalid merge payloads"));
    assert!(record.contains("YAML 1.1 load options adopt Psych-compatible"));
}

#[test]
fn divergence_merge_key_edges_keep_default_strict_and_yaml11_compatible() {
    let invalid = parse_str("target:\n  <<: scalar\n").expect_err("invalid scalar merge payload");
    assert!(
        invalid
            .to_string()
            .contains("expected a mapping or list of mappings for merging, but found scalar"),
        "{invalid}"
    );

    let repeated = parse_str(
        "first: &first {shared: first}\nsecond: &second {shared: second}\ntarget:\n  <<: *first\n  <<: *second\n",
    )
    .expect_err("repeated merge keys stay duplicate keys");
    assert!(repeated.to_string().contains("duplicate mapping key `<<`"));

    let recovered = LoadOptions::yaml_1_1()
        .parse_str(
            "first: &first {shared: first, retries: 3}\nsecond: &second {shared: second, timeout: 10}\ntarget:\n  <<: *first\n  <<: *second\n  keep: value\nscalar_merge:\n  <<: scalar\n  keep: value\n",
        )
        .expect("YAML 1.1 mode recovers merge edges");
    let recovered = yaml::Value::from(recovered);
    assert_eq!(recovered["target"]["shared"].as_str(), Some("second"));
    assert_eq!(recovered["target"]["retries"].as_u64(), Some(3));
    assert_eq!(recovered["target"]["timeout"].as_u64(), Some(10));
    assert_eq!(recovered["target"]["keep"].as_str(), Some("value"));
    assert_eq!(recovered["scalar_merge"]["<<"].as_str(), Some("scalar"));
}

#[test]
fn divergence_custom_tags_are_preserved_without_schema_coercion() {
    let node = parse_str("value: !Thing tagged\n").expect("custom tag preserved");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    let yaml::NodeValue::Tagged(tagged) = &entries[0].1.value else {
        panic!("expected tagged value");
    };
    assert_eq!(tagged.tag, yaml::Tag::new("Thing"));
    assert_eq!(tagged.value.as_str(), Some("tagged"));
}

#[test]
fn divergence_custom_tags_record_is_present() {
    let record = include_str!("fixtures/divergences/records/custom-tags.toml");
    assert!(record.contains("custom-tags"));
    assert!(record.contains("flow mapping keys"));
    assert!(record.contains("parse_events"));
    assert!(record.contains("libyaml"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_block_merge_key_expands_by_default() {
    let node = parse_str("job:\n  <<: {retries: 3}\n").expect("parse default merge key");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[0].1.value else {
        panic!("expected job mapping");
    };
    assert_eq!(job[0].0.as_str(), Some("retries"));
}

#[test]
fn divergence_flow_merge_key_expands_by_default() {
    let node = parse_str("job: {<<: {retries: 3}, name: deploy}\n").expect("parse flow merge key");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[0].1.value else {
        panic!("expected job mapping");
    };
    assert_eq!(job[0].0.as_str(), Some("name"));
    assert_eq!(job[1].0.as_str(), Some("retries"));
}

#[test]
fn divergence_default_schema_keeps_legacy_dates_and_sexagesimal_as_strings() {
    let node = parse_str("date: 2026-05-24\nlegacy_octal: 0123\nduration: 1:20\n").expect("parse");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].1.as_str(), Some("2026-05-24"));
    assert!(matches!(
        entries[1].1.value,
        yaml::NodeValue::Number(yaml::Number::Integer(123))
    ));
    assert_eq!(entries[2].1.as_str(), Some("1:20"));
}

#[test]
fn divergence_yaml11_timestamps_have_native_typed_api() {
    let default = parse_str("date: 2026-05-24\ndatetime: 2026-05-24T12:34:56Z\n")
        .expect("default schema parses timestamp-shaped strings");
    let yaml::NodeValue::Mapping(default_entries) = default.value else {
        panic!("expected default mapping");
    };
    assert_eq!(default_entries[0].1.as_str(), Some("2026-05-24"));
    assert!(default_entries[0].1.as_timestamp().is_none());
    assert!(!matches!(
        default_entries[0].1.value,
        yaml::NodeValue::Tagged(_)
    ));
    assert_eq!(default_entries[1].1.as_str(), Some("2026-05-24T12:34:56Z"));
    assert!(default_entries[1].1.as_timestamp().is_none());
    assert!(!matches!(
        default_entries[1].1.value,
        yaml::NodeValue::Tagged(_)
    ));

    let yaml11 = LoadOptions::yaml_1_1()
        .parse_str("date: 2026-05-24\ndatetime: 2026-05-24T12:34:56Z\n")
        .expect("YAML 1.1 schema parses timestamp-shaped strings");
    let yaml::NodeValue::Mapping(entries) = yaml11.value else {
        panic!("expected YAML 1.1 mapping");
    };
    for (value, source) in [
        (&entries[0].1, "2026-05-24"),
        (&entries[1].1, "2026-05-24T12:34:56Z"),
    ] {
        let yaml::NodeValue::Tagged(tagged) = &value.value else {
            panic!("expected timestamp tag");
        };
        assert_eq!(tagged.tag, yaml::Tag::new("!!timestamp"));
        assert_eq!(tagged.value.as_str(), Some(source));
        assert_eq!(
            value.as_timestamp(),
            yaml::Timestamp::parse_yaml_1_1(source)
        );
    }
}

#[test]
fn divergence_legacy_scalar_resolution_record_is_present() {
    let record = include_str!("fixtures/divergences/records/legacy-scalar-resolution.toml");
    assert!(record.contains("legacy-scalar-resolution"));
    assert!(record.contains("YAML 1.2 core schema"));
    assert!(record.contains("explicit YAML 1.1 scalar construction"));
    assert!(record.contains("exposed through the native yaml::Timestamp API"));
    assert!(
        record.contains("explicit !!binary values with whitespace are retained in trees")
            && record.contains("Malformed explicit !!binary payloads remain loadable")
    );
}

#[test]
fn divergence_github_actions_on_key_record_documents_legacy_collision() {
    let record = include_str!("fixtures/divergences/records/rw-github-actions-on-key.toml");
    assert!(record.contains("rw-github-actions-on-key"));
    assert!(record.contains("distinct string keys"));
    assert!(record.contains("collide"));
}

#[test]
fn divergence_multiline_quoted_flow_key_record_is_present() {
    let record = include_str!("fixtures/divergences/records/multiline-quoted-flow-key.toml");
    assert!(record.contains("multiline-quoted-flow-key"));
    assert!(record.contains("9SA2"));
    assert!(record.contains("yaml-rust2 and saphyr"));
}

#[test]
fn divergence_bare_document_streams_record_is_present() {
    let record = include_str!("fixtures/divergences/records/bare-document-streams.toml");
    assert!(record.contains("bare-document-streams"));
    assert!(record.contains("M7A3"));
    assert!(record.contains("serde_yaml"));
    assert!(record.contains("yaml-rust2"));
    assert!(record.contains("saphyr"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_directive_looking_flow_content_record_is_present() {
    let record = include_str!("fixtures/divergences/records/directive-looking-flow-content.toml");
    assert!(record.contains("directive-looking-flow-content"));
    assert!(record.contains("UT92"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("libyaml"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_colon_anchor_names_record_is_present() {
    let record = include_str!("fixtures/divergences/records/colon-anchor-names.toml");
    assert!(record.contains("colon-anchor-names"));
    assert!(record.contains("2SXE"));
    assert!(record.contains("&a:"));
    assert!(record.contains("*a:"));
    assert!(record.contains("libyaml"));
    assert!(record.contains("yaml-rust2"));
    assert!(record.contains("saphyr"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_empty_scalar_anchors_record_is_present() {
    let record = include_str!("fixtures/divergences/records/empty-scalar-anchors.toml");
    assert!(record.contains("empty-scalar-anchors"));
    assert!(record.contains("PW8X"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("empty strings"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_alias_graph_identity_record_is_present() {
    let record = include_str!("fixtures/divergences/records/alias-graph-identity.toml");
    assert!(record.contains("alias-graph-identity"));
    assert!(record.contains("shared Ruby object identity"));
    assert!(record.contains("LosslessStream"));
    assert!(record.contains("semantic loaders value-oriented"));
    assert!(record.contains("final contract"));
    assert!(record.contains("post-edit graph parity"));
    assert!(record.contains("graph-aware layer"));

    let stream = parse_lossless("base: &base\n  count: 1\na: *base\nb: *base\n")
        .expect("lossless alias graph parses");
    let aliases = stream.aliases();
    assert_eq!(aliases.len(), 2);
    assert_eq!(aliases[0].target(), aliases[1].target());
}

#[test]
fn divergence_complex_key_duplicate_policy_record_is_present() {
    let record = include_str!("fixtures/divergences/records/complex-key-duplicate-policy.toml");
    assert!(record.contains("complex-key-duplicate-policy"));
    assert!(record.contains("X38W"));
    assert!(record.contains("order-insensitive mapping-key equality"));
    assert!(record.contains("yaml-rust2"));
    assert!(record.contains("saphyr"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_byte_deserialization_record_is_present() {
    let record = include_str!("fixtures/divergences/records/byte-deserialization.toml");
    assert!(record.contains("byte-deserialization"));
    assert!(record.contains("deserialize_bytes"));
    assert!(record.contains("deserialize_byte_buf"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("!!binary"));
    assert!(record.contains("plain YAML strings as raw byte buffers"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_duplicate_scalar_keys_record_is_present() {
    let record = include_str!("fixtures/divergences/records/duplicate-scalar-keys.toml");
    assert!(record.contains("duplicate-scalar-keys"));
    assert!(record.contains("env.FEATURE_FLAG"));
    assert!(record.contains("services.web.image"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("last-wins"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_numeric_key_identity_record_is_present() {
    let record = include_str!("fixtures/divergences/records/numeric-key-identity.toml");
    assert!(record.contains("numeric-key-identity"));
    assert!(record.contains("nonnegative signed and unsigned integer keys"));
    assert!(record.contains("0.0 and -0.0 collide"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_explicit_core_tags_record_is_present() {
    let record = include_str!("fixtures/divergences/records/explicit-core-tags.toml");
    assert!(record.contains("explicit-core-tags"));
    assert!(record.contains("!!timestamp"));
    assert!(record.contains("!!float .inf"));
    assert!(record.contains("!!str null"));
    assert!(record.contains("!!bool ON"));
    assert!(record.contains("!!null null"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("libyaml 0.2.1"));
    assert!(record.contains("Date"));
    assert!(record.contains("Infinity"));
    assert!(record.contains("BadValue"));
    assert!(record.contains("typed byte targets decode explicit !!binary"));
    assert!(record.contains("canonical tag:yaml.org,2002:* forms"));
    assert!(record.contains("explicit !!str values override implicit scalar resolution"));
    assert!(record.contains("explicit !!bool and !!null typed reads"));
    assert!(record.contains("explicit !!timestamp yaml::Timestamp reads"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_explicit_core_tags_reference_matrix_matches_record() {
    let record = include_str!("fixtures/divergences/records/explicit-core-tags.toml");
    assert!(record.contains("serde_yaml 0.9.34 drops the explicit tag metadata"));
    assert!(record.contains("yaml-rust2 0.11.0 keeps !!binary and !!timestamp as string values"));
    assert!(
        record.contains("saphyr 0.0.6 reports !!binary, !!int 0x7B, and !!timestamp as BadValue")
    );

    let input = "payload: !!binary SGVsbG8=\nvalue: !!int 0x7B\ndate: !!timestamp 2026-05-24\ninf: !!float .inf\nstring_null: !!str null\nbool_true: !!bool true\nnull_value: !!null null\n";

    let ours: yaml::Value = yaml::from_str(input).expect("ours explicit core tags");
    assert_eq!(
        ours["payload"].as_tagged().expect("payload tag").tag,
        yaml::Tag::new("!!binary")
    );
    assert_eq!(
        ours["value"].as_tagged().expect("int tag").tag,
        yaml::Tag::new("!!int")
    );
    assert_eq!(
        ours["date"].as_tagged().expect("timestamp tag").tag,
        yaml::Tag::new("!!timestamp")
    );
    assert_eq!(
        ours["date"].as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-24")
    );
    assert_eq!(
        ours["inf"].as_tagged().expect("float tag").tag,
        yaml::Tag::new("!!float")
    );
    assert_eq!(ours["string_null"].as_str(), Some("null"));
    assert_eq!(ours["bool_true"].as_bool(), Some(true));
    assert_eq!(ours["null_value"].as_null(), Some(()));
    let ours_bool_on: bool =
        yaml::from_str("!!bool ON\n").expect("ours explicit YAML 1.1 bool tag");
    assert!(ours_bool_on);
    let canonical_bool_on: bool = yaml::from_str("!<tag:yaml.org,2002:bool> ON\n")
        .expect("ours canonical explicit YAML 1.1 bool tag");
    assert!(canonical_bool_on);
    let canonical_value: yaml::Value =
        yaml::from_str("!<tag:yaml.org,2002:int> 0x7B\n").expect("canonical int value");
    assert_eq!(canonical_value.as_i64(), Some(123));

    let serde_yaml: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml explicit core tags");
    assert_eq!(serde_yaml["payload"].as_str(), Some("SGVsbG8="));
    assert_eq!(serde_yaml["value"].as_i64(), Some(123));
    assert_eq!(serde_yaml["date"].as_str(), Some("2026-05-24"));
    assert!(
        serde_yaml["inf"]
            .as_f64()
            .expect("serde_yaml inf")
            .is_infinite()
    );
    assert_eq!(serde_yaml["string_null"].as_str(), Some("null"));
    assert_eq!(serde_yaml["bool_true"].as_bool(), Some(true));
    assert!(serde_yaml["null_value"].is_null());
    let serde_yaml_bool_on =
        serde_yaml::from_str::<bool>("!!bool ON\n").expect_err("serde_yaml rejects !!bool ON");
    assert!(
        serde_yaml_bool_on
            .to_string()
            .contains("expected a boolean"),
        "{serde_yaml_bool_on}"
    );

    let yaml_rust2 =
        yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 explicit core tags");
    let yaml_rust2 = &yaml_rust2[0];
    assert_eq!(yaml_rust2["payload"].as_str(), Some("SGVsbG8="));
    assert!(yaml_rust2["value"].is_badvalue());
    assert_eq!(yaml_rust2["date"].as_str(), Some("2026-05-24"));
    assert!(
        yaml_rust2["inf"]
            .as_f64()
            .expect("yaml-rust2 inf")
            .is_infinite()
    );

    let saphyr = saphyr::Yaml::load_from_str(input).expect("saphyr explicit core tags");
    let saphyr = &saphyr[0];
    assert!(saphyr["payload"].is_badvalue());
    assert!(saphyr["value"].is_badvalue());
    assert!(saphyr["date"].is_badvalue());
    assert!(
        saphyr["inf"]
            .as_floating_point()
            .expect("saphyr inf")
            .is_infinite()
    );
}

#[test]
fn divergence_yaml11_collection_tags_record_is_present() {
    let record = include_str!("fixtures/divergences/records/yaml11-collection-tags.toml");
    assert!(record.contains("yaml11-collection-tags"));
    assert!(record.contains("!!set"));
    assert!(record.contains("!!omap"));
    assert!(record.contains("!!pairs"));
    assert!(record.contains("tag:yaml.org,2002:*"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("Psych::Set"));
    assert!(record.contains("Psych::Omap"));
    assert!(record.contains("duplicate keys are preserved"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_yaml11_collection_tags_reference_matrix_matches_record() {
    use std::collections::{BTreeMap, BTreeSet};

    let input = "\
set: !!set {alpha: null, beta: null}
omap: !!omap [{first: 1}, {second: 2}]
pairs: !!pairs [{repeat: 1}, {repeat: 2}]
";

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Typed {
        set: BTreeSet<String>,
        omap: BTreeMap<String, i64>,
        pairs: Vec<(String, i64)>,
    }

    let ours: Typed = yaml::from_str(input).expect("ours YAML 1.1 collection tags");
    assert_eq!(
        ours.set,
        BTreeSet::from(["alpha".to_string(), "beta".to_string()])
    );
    assert_eq!(
        ours.omap,
        BTreeMap::from([("first".to_string(), 1), ("second".to_string(), 2)])
    );
    assert_eq!(
        ours.pairs,
        vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)]
    );

    let retained: yaml::Value = yaml::from_str(input).expect("ours retained collection tags");
    assert_eq!(
        retained["set"].as_tagged().expect("set tag").tag,
        yaml::Tag::new("!!set")
    );
    assert_eq!(
        retained["omap"].as_tagged().expect("omap tag").tag,
        yaml::Tag::new("!!omap")
    );
    assert_eq!(
        retained["pairs"].as_tagged().expect("pairs tag").tag,
        yaml::Tag::new("!!pairs")
    );

    let serde_yaml: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml YAML 1.1 collection tags");
    assert!(serde_yaml["set"].is_mapping());
    assert!(serde_yaml["omap"].is_sequence());
    assert!(serde_yaml["pairs"].is_sequence());

    let yaml_rust2 =
        yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 collection tags");
    assert!(yaml_rust2[0]["set"].is_hash());
    assert!(yaml_rust2[0]["omap"].is_array());
    assert!(yaml_rust2[0]["pairs"].is_array());
}

#[test]
fn divergence_yaml11_core_structural_tags_record_is_present() {
    let record = include_str!("fixtures/divergences/records/yaml11-core-structural-tags.toml");
    assert!(record.contains("yaml11-core-structural-tags"));
    assert!(record.contains("!!seq"));
    assert!(record.contains("!!map"));
    assert!(record.contains("!!value"));
    assert!(record.contains("tag:yaml.org,2002:*"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_yaml11_core_structural_tags_reference_matrix_matches_record() {
    use std::collections::BTreeMap;

    let input = "\
%TAG !yaml! tag:yaml.org,2002:
---
seq: !!seq [1, 2]
map: !!map {a: 1, b: 2}
value: !!value =
canonical_seq: !<tag:yaml.org,2002:seq> [left, right]
canonical_map: !<tag:yaml.org,2002:map> {limit: 10}
resolved_seq: !yaml!seq [alpha, beta]
resolved_map: !yaml!map {retries: 3}
";

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Typed {
        seq: Vec<i64>,
        map: BTreeMap<String, i64>,
        value: String,
        canonical_seq: Vec<String>,
        canonical_map: BTreeMap<String, i64>,
        resolved_seq: Vec<String>,
        resolved_map: BTreeMap<String, i64>,
    }

    let ours: Typed = yaml::from_str(input).expect("ours core structural tags");
    assert_eq!(ours.seq, vec![1, 2]);
    assert_eq!(
        ours.map,
        BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)])
    );
    assert_eq!(ours.value, "=");
    assert_eq!(
        ours.canonical_seq,
        vec!["left".to_string(), "right".to_string()]
    );
    assert_eq!(
        ours.canonical_map,
        BTreeMap::from([("limit".to_string(), 10)])
    );
    assert_eq!(
        ours.resolved_seq,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(
        ours.resolved_map,
        BTreeMap::from([("retries".to_string(), 3)])
    );

    let retained: yaml::Value = yaml::from_str(input).expect("ours retained structural tags");
    assert_eq!(
        retained["seq"].as_tagged().expect("seq tag").tag,
        yaml::Tag::new("!!seq")
    );
    assert_eq!(
        retained["map"].as_tagged().expect("map tag").tag,
        yaml::Tag::new("!!map")
    );
    assert_eq!(
        retained["value"].as_tagged().expect("value tag").tag,
        yaml::Tag::new("!!value")
    );
    assert_eq!(
        retained["resolved_seq"]
            .as_tagged()
            .expect("resolved seq tag")
            .tag,
        yaml::Tag::new("!<tag:yaml.org,2002:seq>")
    );

    let serde_yaml: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml structural tags");
    assert!(serde_yaml["seq"].is_sequence());
    assert!(serde_yaml["map"].is_mapping());
    assert!(serde_yaml["value"].is_string());

    let yaml_rust2 =
        yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 structural tags");
    assert!(yaml_rust2[0]["seq"].is_array());
    assert!(yaml_rust2[0]["map"].is_hash());
    assert_eq!(yaml_rust2[0]["value"].as_str(), Some("="));

    let saphyr = saphyr::Yaml::load_from_str(input).expect("saphyr structural tags");
    assert!(saphyr_is_sequence(&saphyr[0]["seq"]));
    assert!(saphyr_is_mapping(&saphyr[0]["map"]));
}

fn saphyr_is_sequence(value: &saphyr::Yaml<'_>) -> bool {
    match value {
        saphyr::Yaml::Sequence(_) => true,
        saphyr::Yaml::Tagged(_, value) => saphyr_is_sequence(value),
        _ => false,
    }
}

fn saphyr_is_mapping(value: &saphyr::Yaml<'_>) -> bool {
    match value {
        saphyr::Yaml::Mapping(_) => true,
        saphyr::Yaml::Tagged(_, value) => saphyr_is_mapping(value),
        _ => false,
    }
}

#[test]
fn divergence_adjacent_flow_mapping_scalars_record_is_present() {
    let record = include_str!("fixtures/divergences/records/adjacent-flow-mapping-scalars.toml");
    assert!(record.contains("adjacent-flow-mapping-scalars"));
    assert!(record.contains("C2DT"));
    assert!(record.contains("5MUD"));
    assert!(record.contains("5T43"));
    assert!(record.contains("58MP"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_flow_collection_comment_separation_record_is_present() {
    let record =
        include_str!("fixtures/divergences/records/flow-collection-comment-separation.toml");
    assert!(record.contains("flow-collection-comment-separation"));
    assert!(record.contains("9JBA"));
    assert!(record.contains("CVW2"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_flow_plain_dash_rejections_record_is_present() {
    let record = include_str!("fixtures/divergences/records/flow-plain-dash-rejections.toml");
    assert!(record.contains("flow-plain-dash-rejections"));
    assert!(record.contains("YJV2"));
    assert!(record.contains("G5U8"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_block_scalar_indentation_rejections_record_is_present() {
    let record =
        include_str!("fixtures/divergences/records/block-scalar-indentation-rejections.toml");
    assert!(record.contains("block-scalar-indentation-rejections"));
    assert!(record.contains("5LLU"));
    assert!(record.contains("S98Z"));
    assert!(record.contains("W9L4"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_scalar_comment_separation_rejections_record_is_present() {
    let record =
        include_str!("fixtures/divergences/records/scalar-comment-separation-rejections.toml");
    assert!(record.contains("scalar-comment-separation-rejections"));
    assert!(record.contains("SU5Z"));
    assert!(record.contains("X4QW"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_tab_token_separation_record_is_present() {
    let record = include_str!("fixtures/divergences/records/tab-token-separation.toml");
    assert!(record.contains("tab-token-separation"));
    for case in [
        "6BCT", "6CA3", "Q5MG", "Y79Y/001", "Y79Y/010", "DK95/00", "DK95/02", "DK95/03", "DK95/04",
        "DK95/05", "DK95/07", "DK95/08", "R4YG",
    ] {
        assert!(record.contains(case));
    }
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("libyaml 0.2.1"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_quoted_scalar_continuation_indentation_record_is_present() {
    let record =
        include_str!("fixtures/divergences/records/quoted-scalar-continuation-indentation.toml");
    assert!(record.contains("quoted-scalar-continuation-indentation"));
    assert!(record.contains("QB6E"));
    assert!(record.contains("DK95/01"));
    assert!(record.contains("DK95/06"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_flow_collection_comment_separation_rejects_yaml_suite_error() {
    for (name, input) in [
        (
            "9JBA",
            include_str!("fixtures/yaml-test-suite/data/9JBA/in.yaml"),
        ),
        (
            "CVW2",
            include_str!("fixtures/yaml-test-suite/data/CVW2/in.yaml"),
        ),
    ] {
        if parse_str(input).is_ok() {
            panic!("prototype rejects adjacent flow comment in {name}");
        }
        if yaml::parse_events(input).is_ok() {
            panic!("event parser rejects adjacent flow comment in {name}");
        }

        let serde_value: serde_yaml::Value =
            serde_yaml::from_str(input).expect("serde_yaml accepts adjacent flow comment");
        assert_eq!(
            serde_value
                .as_sequence()
                .expect("serde_yaml sequence")
                .len(),
            3,
            "{name}"
        );

        assert!(
            yaml_rust2::YamlLoader::load_from_str(input).is_err(),
            "yaml-rust2 rejects YAML-suite {name}"
        );
        assert!(
            saphyr::Yaml::load_from_str(input).is_err(),
            "saphyr rejects YAML-suite {name}"
        );
    }
}

#[test]
fn divergence_null_like_string_targets_record_is_present() {
    let record = include_str!("fixtures/divergences/records/null-like-string-targets.toml");
    assert!(record.contains("null-like-string-targets"));
    assert!(record.contains("BTreeMap<String, String>"));
    assert!(record.contains("yaml::Value"));
    assert!(record.contains("Option<T>"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_source_backed_string_targets_record_is_present() {
    let record = include_str!("fixtures/divergences/records/source-backed-string-targets.toml");
    assert!(record.contains("source-backed-string-targets"));
    assert!(record.contains("BTreeMap<String, String>"));
    assert!(record.contains("source spelling"));
    assert!(record.contains("yaml::Value"));
    assert!(record.contains("i128/u128"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_complex_collection_duplicate_key_policy_matches_documented_split() {
    let input = include_str!("fixtures/yaml-test-suite/data/X38W/in.yaml");
    let error = parse_str(input).expect_err("prototype rejects alias-expanded duplicate keys");
    let display = error.to_string();
    assert!(display.contains("duplicate mapping key"));
    assert!(display.contains("[a, b]"));
    assert!(
        !error.diagnostic().related.is_empty(),
        "duplicate key diagnostic points back to the first collection key"
    );
    assert!(
        yaml_rust2::YamlLoader::load_from_str(input).is_err(),
        "yaml-rust2 rejects alias-expanded duplicate sequence keys"
    );
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts YAML-suite X38W");
    yaml::parse_events(input).expect("raw event parser preserves X38W alias events");

    let permuted = "root: {? {a: 1, b: 2}: first, ? {b: 2, a: 1}: second}\n";
    let error = parse_str(permuted).expect_err("prototype rejects permuted duplicate mapping keys");
    let display = error.to_string();
    assert!(display.contains("duplicate mapping key"));
    assert!(
        !error.diagnostic().related.is_empty(),
        "permuted duplicate key diagnostic points back to the first mapping key"
    );
    yaml::parse_events(permuted).expect("raw event parser preserves permuted duplicate keys");
}

#[test]
fn divergence_duplicate_scalar_key_policy_matches_config_safety_decision() {
    let input = "services:\n  web:\n    image: nginx\n    image: redis\n";

    let error = parse_str(input).expect_err("prototype rejects duplicate config keys");
    let display = error.to_string();
    assert!(display.contains("duplicate mapping key"));
    assert!(display.contains("image"));

    let serde_error = serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect_err("serde_yaml rejects duplicate config keys");
    let serde_display = serde_error.to_string();
    assert!(serde_display.contains("duplicate entry"));
    assert!(serde_display.contains("image"));

    yaml::parse_events(input).expect("raw event parser exposes duplicate keys");
}

#[test]
fn divergence_raw_event_records_are_present() {
    for (record, required) in [
        (
            include_str!("fixtures/divergences/records/raw-event-directives.toml"),
            "directive",
        ),
        (
            include_str!("fixtures/divergences/records/raw-event-document-markers.toml"),
            "explicitness",
        ),
        (
            include_str!("fixtures/divergences/records/raw-event-anchors-aliases.toml"),
            "Alias events",
        ),
        (
            include_str!("fixtures/divergences/records/raw-event-scalar-style.toml"),
            "scalar style",
        ),
        (
            include_str!("fixtures/divergences/records/raw-event-implicit-tag-flags.toml"),
            "implicit tag flags",
        ),
        (
            include_str!("fixtures/divergences/records/raw-event-collection-style.toml"),
            "collection style",
        ),
    ] {
        assert!(record.contains("raw-event"));
        assert!(record.contains(required));
        assert!(record.contains("libyaml"));
    }
}

#[test]
fn divergence_empty_implicit_key_record_is_present() {
    let record = include_str!("fixtures/divergences/records/empty-implicit-keys.toml");
    assert!(record.contains("empty-implicit-keys"));
    for case in ["S3PD", "CFD4", "M2N8/00", "UKK6/00"] {
        assert!(record.contains(case));
    }
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_empty_block_scalar_event_shape_record_is_present() {
    let record = include_str!("fixtures/divergences/records/empty-block-scalar-event-shape.toml");
    assert!(record.contains("empty-block-scalar-event-shape"));
    assert!(record.contains("K858"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_explicit_non_specific_tag_shape_record_is_present() {
    let record = include_str!("fixtures/divergences/records/explicit-non-specific-tag-shape.toml");
    assert!(record.contains("explicit-non-specific-tag-shape"));
    assert!(record.contains("UKK6/02"));
    assert!(record.contains("S4JQ"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_yaml_suite_final_parity_deferrals_record_is_present() {
    let record =
        include_str!("fixtures/divergences/records/yaml-suite-final-parity-deferrals.toml");
    assert!(record.contains("yaml-suite-final-parity-deferrals"));
    for case in [
        "5TYM", "7FWL", "Q9WF", "UGM3", "K54U", "WZ62", "4ABK", "4MUZ/00", "4MUZ/01", "4MUZ/02",
        "7Z25", "8XYN", "A2M4", "DBG4", "FRK4", "HM87/00", "HWV9", "K3WX", "NHX8", "NJ66", "NKF9",
        "QT73", "SM9W/01", "VJP3/01", "W5VH",
    ] {
        assert!(record.contains(case));
    }
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_yaml_suite_tagged_tree_deferrals_record_is_present() {
    let record = include_str!("fixtures/divergences/records/yaml-suite-tagged-tree-deferrals.toml");
    assert!(record.contains("yaml-suite-tagged-tree-deferrals"));
    for case in ["2AUY", "33X3", "74H7", "C4HZ", "F2C7", "FH7J", "L94M"] {
        assert!(record.contains(case));
    }
    assert!(record.contains("retained tag"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_yaml_suite_null_document_counts_record_is_present() {
    let record = include_str!("fixtures/divergences/records/yaml-suite-null-document-counts.toml");
    assert!(record.contains("yaml-suite-null-document-counts"));
    for case in ["AVM7", "8G76", "98YD"] {
        assert!(record.contains(case));
    }
    assert!(record.contains("one null document"));
    assert!(record.contains("zero documents"));
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_document_start_block_scalar_record_is_present() {
    let record = include_str!("fixtures/divergences/records/document-start-block-scalars.toml");
    assert!(record.contains("document-start-block-scalars"));
    for case in ["W4TN", "FP8R", "DK3J"] {
        assert!(record.contains(case));
    }
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("decision"));
}

#[test]
fn divergence_directive_milestone_records_are_present() {
    for (record, required) in [
        (
            include_str!("fixtures/divergences/records/yaml-version-directive-schema.toml"),
            "BEC7",
        ),
        (
            include_str!("fixtures/divergences/records/document-start-inline-node.toml"),
            "document-start",
        ),
        (
            include_str!(
                "fixtures/divergences/records/tag-directive-scope-and-undeclared-handles.toml"
            ),
            "undeclared",
        ),
    ] {
        assert!(record.contains(required));
        assert!(record.contains("libyaml"));
        assert!(record.contains("decision"));
    }
}

#[test]
fn divergence_typed_scalar_mapping_keys_are_distinct() {
    let node =
        parse_str("1: int\n\"1\": string\ntrue: bool\n\"true\": string\nnull: null-key\n\"null\": string-null\n")
            .expect("typed scalar keys remain distinct");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 6);
    assert!(matches!(
        entries[0].0.value,
        yaml::NodeValue::Number(yaml::Number::Integer(1))
    ));
    assert_eq!(entries[1].0.as_str(), Some("1"));
    assert!(matches!(entries[2].0.value, yaml::NodeValue::Bool(true)));
    assert_eq!(entries[3].0.as_str(), Some("true"));
    assert!(matches!(entries[4].0.value, yaml::NodeValue::Null));
    assert_eq!(entries[5].0.as_str(), Some("null"));
}

#[test]
fn divergence_numeric_forms_are_documented_policy() {
    let node = parse_str(
        "hex: 0x7B\noctal: 0o77\nbinary: 0b1010\nplus_hex: +0xF\nneg_hex: -0xF\ninf: .inf\nnan: .NaN\nunderscored: 1_000\nfloat_underscored: 1_2.3_4\n",
    )
    .expect("parse numeric divergences");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].1.as_str(), Some("0x7B"));
    assert_eq!(entries[1].1.as_str(), Some("0o77"));
    assert_eq!(entries[2].1.as_str(), Some("0b1010"));
    assert_eq!(entries[3].1.as_str(), Some("+0xF"));
    assert_eq!(entries[4].1.as_str(), Some("-0xF"));
    assert!(matches!(
        entries[5].1.value,
        yaml::NodeValue::Number(yaml::Number::Float(value))
            if value == f64::INFINITY
    ));
    assert!(matches!(
        entries[6].1.value,
        yaml::NodeValue::Number(yaml::Number::Float(value)) if value.is_nan()
    ));
    assert!(matches!(
        entries[7].1.value,
        yaml::NodeValue::Number(yaml::Number::Integer(1000))
    ));
    assert!(matches!(
        entries[8].1.value,
        yaml::NodeValue::Number(yaml::Number::Float(value)) if value == 12.34
    ));
}

#[test]
fn divergence_supported_directives_are_syntax_only_and_reserved_directives_are_ignored() {
    let yaml = parse_str("%YAML 1.2\n---\nkey: value\n").expect("YAML 1.2 directive");
    let yaml::NodeValue::Mapping(entries) = yaml.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("key"));

    let yaml_1_1 =
        parse_str("%YAML 1.1\n---\non: off\nyes: no\n").expect("YAML 1.1 directive syntax");
    let yaml::NodeValue::Mapping(entries) = yaml_1_1.value else {
        panic!("expected YAML 1.1 mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("off"));
    assert_eq!(entries[1].0.as_str(), Some("yes"));
    assert_eq!(entries[1].1.as_str(), Some("no"));

    let tagged =
        parse_str("%TAG !e! tag:example.com,2026:\n---\nvalue: !e!Thing x\n").expect("TAG");
    let yaml::NodeValue::Mapping(entries) = tagged.value else {
        panic!("expected mapping");
    };
    let yaml::NodeValue::Tagged(tagged) = &entries[0].1.value else {
        panic!("expected tagged value");
    };
    assert_eq!(tagged.tag.handle, "!");
    assert_eq!(tagged.tag.suffix, "tag:example.com,2026:Thing");

    let reserved =
        parse_str(include_str!("fixtures/yaml-test-suite/data/6LVF/in.yaml")).expect("6LVF");
    assert_eq!(reserved.as_str(), Some("foo"));
    assert!(
        serde_yaml::from_str::<serde_yaml::Value>(include_str!(
            "fixtures/yaml-test-suite/data/6LVF/in.yaml"
        ))
        .is_err(),
        "serde_yaml rejects reserved directive fixture"
    );

    for input in [
        "%YAML 1.1#...\n---\nkey: value\n",
        "%FOO bar\nkey: value\n",
        "%TAG !e! tag:example.com,2026:\nkey: value\n",
    ] {
        let error = parse_str(input).expect_err("unsupported directive form rejected");
        assert!(
            error.to_string().contains("unsupported YAML directive")
                || error.to_string().contains("invalid YAML directive")
                || error.to_string().contains("explicit document start")
        );
    }
}

#[test]
fn divergence_undeclared_named_tag_handles_are_rejected() {
    for input in [
        "!h!Thing value\n",
        "root: !h!Thing value\n",
        "root: [!h!Thing value]\n",
    ] {
        let tree_error = parse_str(input).expect_err("tree rejects undeclared handle");
        assert!(
            tree_error
                .to_string()
                .contains("undeclared TAG directive handle")
        );

        let event_error = yaml::parse_events(input).expect_err("events reject undeclared handle");
        assert!(
            event_error
                .to_string()
                .contains("undeclared TAG directive handle")
        );
    }
}

#[test]
fn divergence_binary_and_core_tags_are_preserved_in_retained_trees() {
    let node = parse_str(
        "payload: !!binary SGVsbG8=\nvalue: !!int 0x7B\ndate: !!timestamp 2026-05-24\ninf: !!float .inf\n",
    )
    .expect("parse tags");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected mapping");
    };
    let yaml::NodeValue::Tagged(binary) = &entries[0].1.value else {
        panic!("expected binary tag");
    };
    assert_eq!(binary.tag, yaml::Tag::new("!!binary"));
    assert_eq!(binary.value.as_str(), Some("SGVsbG8="));
    let decoded: Vec<u8> =
        yaml::from_str("!!binary SGVsbG8=\n").expect("typed byte target decodes explicit binary");
    assert_eq!(decoded, b"Hello");
    let explicit_timestamp: yaml::Timestamp =
        yaml::from_str("!!timestamp 2026-05-24\n").expect("explicit timestamp typed read");
    assert_eq!(
        explicit_timestamp,
        yaml::Timestamp::parse_yaml_1_1("2026-05-24").expect("timestamp")
    );

    let yaml::NodeValue::Tagged(integer) = &entries[1].1.value else {
        panic!("expected int tag");
    };
    assert_eq!(integer.tag, yaml::Tag::new("!!int"));
    assert_eq!(integer.value.as_str(), Some("0x7B"));

    let yaml::NodeValue::Tagged(timestamp) = &entries[2].1.value else {
        panic!("expected timestamp tag");
    };
    assert_eq!(timestamp.tag, yaml::Tag::new("!!timestamp"));
    assert_eq!(timestamp.value.as_str(), Some("2026-05-24"));
    assert_eq!(
        entries[2].1.as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-24")
    );

    let yaml::NodeValue::Tagged(float) = &entries[3].1.value else {
        panic!("expected float tag");
    };
    assert_eq!(float.tag, yaml::Tag::new("!!float"));
    assert_eq!(float.value.as_str(), Some(".inf"));
}
