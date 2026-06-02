use std::borrow::Cow;

fn mapping_value<'a, 'de>(
    node: &'a saneyaml::BorrowedNode<'de>,
    key: &str,
) -> &'a saneyaml::BorrowedNode<'de> {
    let saneyaml::BorrowedNodeValue::Mapping(entries) = &node.value else {
        panic!("expected mapping, got {:?}", node.value);
    };
    entries
        .iter()
        .find_map(|(entry_key, value)| (entry_key.as_str() == Some(key)).then_some(value))
        .unwrap_or_else(|| panic!("missing key {key:?}"))
}

fn assert_borrowed_string(node: &saneyaml::BorrowedNode<'_>, expected: &str) {
    let saneyaml::BorrowedNodeValue::String(Cow::Borrowed(actual)) = &node.value else {
        panic!("expected borrowed string, got {:?}", node.value);
    };
    assert_eq!(*actual, expected);
}

fn assert_owned_string(node: &saneyaml::BorrowedNode<'_>, expected: &str) {
    let saneyaml::BorrowedNodeValue::String(Cow::Owned(actual)) = &node.value else {
        panic!("expected owned string, got {:?}", node.value);
    };
    assert_eq!(actual, expected);
}

#[test]
fn borrowed_documents_borrow_only_sliceable_scalars() {
    let input = concat!(
        "plain: value\n",
        "double: \"quoted\"\n",
        "single: 'literal'\n",
        "escaped: \"line\\nbreak\"\n",
        "single_escape: 'it''s'\n",
        "block: |\n  value\n",
    );

    let docs = saneyaml::parse_borrowed_documents(input).expect("borrowed documents");
    let doc = &docs[0];

    assert_borrowed_string(mapping_value(doc, "plain"), "value");
    assert_borrowed_string(mapping_value(doc, "double"), "quoted");
    assert_borrowed_string(mapping_value(doc, "single"), "literal");
    assert_owned_string(mapping_value(doc, "escaped"), "line\nbreak");
    assert_owned_string(mapping_value(doc, "single_escape"), "it's");
    assert_owned_string(mapping_value(doc, "block"), "value\n");
}

#[test]
fn borrowed_documents_match_owned_tree_after_merges() {
    let input = concat!(
        "base: &base\n",
        "  inherited: value\n",
        "merged:\n",
        "  <<: *base\n",
        "  local: \"quoted\"\n",
    );

    let owned = saneyaml::parse_documents(input).expect("owned documents");
    let borrowed = saneyaml::parse_borrowed_documents(input).expect("borrowed documents");

    assert_eq!(borrowed.len(), owned.len());
    assert!(
        borrowed[0]
            .clone()
            .into_owned_value()
            .equivalent(&saneyaml::Value::from(&owned[0]))
    );
    let inherited = mapping_value(mapping_value(&borrowed[0], "merged"), "inherited");
    assert_borrowed_string(inherited, "value");
}

#[test]
fn borrowed_documents_respect_load_options() {
    let input = "%YAML 1.1\n---\nflag: YES\n";
    let docs = saneyaml::LoadOptions::yaml_version_directive()
        .parse_borrowed_documents(input)
        .expect("directive-driven borrowed documents");
    let flag = mapping_value(&docs[0], "flag");
    assert!(matches!(
        flag.value,
        saneyaml::BorrowedNodeValue::Bool(true)
    ));
}

#[test]
fn borrowed_documents_keep_parser_diagnostics() {
    let input = "a: 1\na: 2\n";
    let owned_error = saneyaml::parse_documents(input).expect_err("owned duplicate key");
    let borrowed_error =
        saneyaml::parse_borrowed_documents(input).expect_err("borrowed duplicate key");

    assert_eq!(borrowed_error.to_string(), owned_error.to_string());
    assert_eq!(borrowed_error.span(), owned_error.span());
}
