use saneyaml::{Value, parse_documents};

#[test]
fn block_scalar_content_allows_indicator_tab_lines() {
    let value: Value = saneyaml::from_str("key: |-\n  ?\tx\n  :\t-\n  -\t- \n  y\n")
        .expect("indicator-tab block scalar content parses");

    assert_eq!(value["key"].as_str(), Some("?\tx\n:\t-\n-\t- \ny"));
}

#[test]
fn emitted_multiline_indicator_tab_string_round_trips() {
    let value = Value::String("?\tx\ny".to_string());
    let emitted = saneyaml::to_string(&value).expect("emit multiline string");
    let reparsed: Value = saneyaml::from_str(&emitted).expect("parse emitted block scalar");

    assert_eq!(reparsed, value);
}

#[test]
fn block_scalar_content_allows_indented_document_marker_text() {
    let docs = parse_documents("|\n  ---\n").expect("document marker text stays in scalar");
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].as_str(), Some("---\n"));

    let value: Value =
        saneyaml::from_str("k: |\n  ... \n").expect("document end marker text stays in scalar");
    assert_eq!(value["k"].as_str(), Some("... \n"));
}

#[test]
fn block_plain_mapping_keys_may_contain_flow_indicator_characters() {
    for (input, key, expected) in [
        ("a[: x\n", "a[", "x"),
        ("x{: y\n", "x{", "y"),
        ("a]: x\n", "a]", "x"),
        ("a}: y\n", "a}", "y"),
    ] {
        let value: Value = saneyaml::from_str(input)
            .unwrap_or_else(|error| panic!("{input:?} should parse as mapping: {error}"));
        let mapping = value
            .as_mapping()
            .unwrap_or_else(|| panic!("{input:?} should produce a mapping, got {value:?}"));

        assert_eq!(
            mapping.get(key).and_then(Value::as_str),
            Some(expected),
            "{input:?}"
        );
    }
}

#[test]
fn invalid_plain_scalars_cannot_start_with_flow_close_or_explicit_key_indicators() {
    for input in [
        "]a: x\n", "}a: x\n", ",a: x\n", "- }x\n", "]x\n", "k: ? x\n",
    ] {
        let error = match saneyaml::from_str::<Value>(input) {
            Ok(value) => panic!("{input:?} should reject, got {value:?}"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("plain scalar cannot start"),
            "{input:?}: {error}"
        );
    }
}

#[test]
fn inline_explicit_key_in_block_value_position_builds_nested_mapping() {
    let value: Value =
        saneyaml::from_str("? a\n: ? b\n").expect("inline explicit key with null value parses");
    assert!(value["a"]["b"].is_null());

    let value: Value =
        saneyaml::from_str("? a\n: ? b\n  : c\n").expect("inline explicit key value parses");
    assert_eq!(value["a"]["b"].as_str(), Some("c"));
}

#[test]
fn inline_explicit_key_in_flow_sequence_item_builds_mapping_item() {
    let value: Value = saneyaml::from_str("[? a]\n").expect("flow explicit key item parses");
    let items = value.as_sequence().expect("sequence");
    assert_eq!(items.len(), 1);
    assert!(items[0]["a"].is_null());

    let value: Value = saneyaml::from_str("[? a, c]\n").expect("flow explicit key item parses");
    let items = value.as_sequence().expect("sequence");
    assert_eq!(items.len(), 2);
    assert!(items[0]["a"].is_null());
    assert_eq!(items[1].as_str(), Some("c"));
}
