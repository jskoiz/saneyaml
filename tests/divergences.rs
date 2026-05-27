use saphyr::LoadableYamlNode;
use yaml::parse_str;

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
fn divergence_merge_keys_are_preserved_literally_instead_of_expanded() {
    let node =
        parse_str("defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n")
            .expect("merge key preserved literally");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[1].1.value else {
        panic!("expected job mapping");
    };
    assert_eq!(job[0].0.as_str(), Some("<<"));
    assert!(matches!(job[0].1.value, yaml::NodeValue::Mapping(_)));
    assert_eq!(job[1].0.as_str(), Some("name"));
    assert!(
        job.iter().all(|(key, _)| key.as_str() != Some("retries")),
        "merge key is not expanded into job.retries"
    );
}

#[test]
fn divergence_merge_list_is_preserved_literally_instead_of_expanded() {
    let node = parse_str(
        "base1: &base1 {a: 1, b: 1, shared: first}\nbase2: &base2 {b: 2, c: 2, shared: second}\nmerged:\n  <<: [*base1, *base2]\n  b: explicit\n",
    )
    .expect("merge-list key preserved literally");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(merged) = &entries[2].1.value else {
        panic!("expected merged mapping");
    };
    assert_eq!(merged[0].0.as_str(), Some("<<"));
    assert!(matches!(merged[0].1.value, yaml::NodeValue::Sequence(_)));
    assert_eq!(merged[1].0.as_str(), Some("b"));
    assert_eq!(merged[1].1.as_str(), Some("explicit"));
    assert!(
        merged
            .iter()
            .all(|(key, _)| !matches!(key.as_str(), Some("a" | "c" | "shared"))),
        "merge-list keys are not expanded into the target mapping"
    );
}

#[test]
fn divergence_merge_key_record_documents_default_and_opt_in_policy() {
    let record = include_str!("fixtures/divergences/records/merge-keys.toml");
    assert!(record.contains("literal keys by default"));
    assert!(record.contains("opt in with yaml::Value::apply_merge()"));
    assert!(record.contains("serde_yaml::Value::apply_merge"));
    assert!(record.contains("Psych 3.1.0/libyaml 0.2.1"));
    assert!(record.contains("earlier merge-list mappings"));
    assert!(record.contains("explicit target keys override"));
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
fn divergence_block_merge_key_is_a_literal_key() {
    let node = parse_str("job:\n  <<: {retries: 3}\n").expect("parse literal merge key");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[0].1.value else {
        panic!("expected job mapping");
    };
    assert_eq!(job[0].0.as_str(), Some("<<"));
}

#[test]
fn divergence_flow_merge_key_is_a_literal_key() {
    let node = parse_str("job: {<<: {retries: 3}, name: deploy}\n").expect("parse flow merge key");
    let yaml::NodeValue::Mapping(entries) = node.value else {
        panic!("expected top-level mapping");
    };
    let yaml::NodeValue::Mapping(job) = &entries[0].1.value else {
        panic!("expected job mapping");
    };
    assert_eq!(job[0].0.as_str(), Some("<<"));
    assert_eq!(job[1].0.as_str(), Some("name"));
}

#[test]
fn divergence_yaml_1_1_dates_octal_and_sexagesimal_are_not_legacy_typed() {
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
fn divergence_legacy_scalar_resolution_record_is_present() {
    let record = include_str!("fixtures/divergences/records/legacy-scalar-resolution.toml");
    assert!(record.contains("legacy-scalar-resolution"));
    assert!(record.contains("YAML 1.2 core schema"));
    assert!(record.contains("YAML 1.1 implicit typing"));
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
    assert!(record.contains("serde_yaml 0.9.34"));
    assert!(record.contains("yaml-rust2 0.11.0"));
    assert!(record.contains("saphyr 0.0.6"));
    assert!(record.contains("libyaml 0.2.1"));
    assert!(record.contains("Date"));
    assert!(record.contains("Infinity"));
    assert!(record.contains("BadValue"));
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

    let input = "payload: !!binary SGVsbG8=\nvalue: !!int 0x7B\ndate: !!timestamp 2026-05-24\ninf: !!float .inf\n";

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
        ours["inf"].as_tagged().expect("float tag").tag,
        yaml::Tag::new("!!float")
    );

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
fn divergence_tab_token_separation_record_is_present() {
    let record = include_str!("fixtures/divergences/records/tab-token-separation.toml");
    assert!(record.contains("tab-token-separation"));
    assert!(record.contains("6BCT"));
    assert!(record.contains("R4YG"));
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
fn divergence_directive_milestone_records_are_present() {
    for (record, required) in [
        (
            include_str!("fixtures/divergences/records/yaml-version-directive-schema.toml"),
            "YAML 1.2",
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
fn divergence_binary_and_core_tags_are_preserved_without_coercion() {
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

    let yaml::NodeValue::Tagged(float) = &entries[3].1.value else {
        panic!("expected float tag");
    };
    assert_eq!(float.tag, yaml::Tag::new("!!float"));
    assert_eq!(float.value.as_str(), Some(".inf"));
}
