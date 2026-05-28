use serde::Deserialize;
use yaml::{LoadOptions, Schema, Value};

#[test]
fn schema_mode_defaults_to_yaml_12_config_behavior() {
    let input = "%YAML 1.1\n---\non: push\nyes: deploy\nflag: ON\nhex: 0x7B\nsex: 1:20\n";
    let value: Value = yaml::from_str(input).expect("default schema parses");

    assert_eq!(value["on"].as_str(), Some("push"));
    assert_eq!(value["yes"].as_str(), Some("deploy"));
    assert_eq!(value["flag"].as_str(), Some("ON"));
    assert_eq!(value["hex"].as_str(), Some("0x7B"));
    assert_eq!(value["sex"].as_str(), Some("1:20"));
}

#[test]
fn yaml_11_schema_resolves_legacy_boolean_aliases() {
    let input = "flags: [y, Y, yes, Yes, YES, n, N, no, No, NO, on, On, ON, off, Off, OFF]\n";
    let value: Value = LoadOptions::yaml_1_1()
        .from_str(input)
        .expect("YAML 1.1 booleans parse");
    let flags = value["flags"].as_sequence().expect("flags sequence");
    let resolved = flags
        .iter()
        .map(|value| value.as_bool().expect("boolean alias"))
        .collect::<Vec<_>>();
    assert_eq!(
        resolved,
        vec![
            true, true, true, true, true, false, false, false, false, false, true, true, true,
            false, false, false
        ]
    );
}

#[test]
fn yaml_11_schema_resolves_legacy_numeric_forms_that_fit_value_model() {
    let input = "\
octal: 0123
negative_octal: -0123
invalid_octal: 09
hex: 0x7B
binary: 0b1010
sexagesimal: 1:20:30
underscored: 1_000
";
    let value: Value = LoadOptions::new()
        .schema(Schema::Yaml11)
        .from_str(input)
        .expect("YAML 1.1 numerics parse");

    assert_eq!(value["octal"].as_i64(), Some(83));
    assert_eq!(value["negative_octal"].as_i64(), Some(-83));
    assert_eq!(value["invalid_octal"].as_str(), Some("09"));
    assert_eq!(value["hex"].as_i64(), Some(123));
    assert_eq!(value["binary"].as_i64(), Some(10));
    assert_eq!(value["sexagesimal"].as_i64(), Some(4830));
    assert_eq!(value["underscored"].as_i64(), Some(1000));
}

#[test]
fn yaml_11_schema_keeps_timestamps_as_strings_until_public_type_exists() {
    let value: Value = LoadOptions::yaml_1_1()
        .from_str("date: 2026-05-24\ndatetime: 2026-05-24T12:34:56Z\n")
        .expect("timestamp-shaped scalars parse");

    assert_eq!(value["date"].as_str(), Some("2026-05-24"));
    assert_eq!(value["datetime"].as_str(), Some("2026-05-24T12:34:56Z"));
}

#[test]
fn yaml_11_schema_reports_duplicate_key_collisions_with_spans() {
    let error = LoadOptions::yaml_1_1()
        .parse_str("on: push\nyes: deploy\n")
        .expect_err("YAML 1.1 boolean aliases collide");

    assert!(error.to_string().contains("duplicate mapping key `true`"));
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 1);
    let related = &error.diagnostic().related;
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].span.line, 1);
    assert_eq!(related[0].span.column, 1);
}

#[test]
fn yaml_11_schema_options_cover_streaming_deserializer_and_documents() {
    let input = "---\nflag: ON\n---\ncount: 0x10\n";
    let options = LoadOptions::yaml_1_1();
    let docs: Vec<Value> = options
        .from_documents_str(input)
        .expect("YAML 1.1 document stream parses");

    assert_eq!(docs[0]["flag"].as_bool(), Some(true));
    assert_eq!(docs[1]["count"].as_i64(), Some(16));

    let streamed = options
        .deserializer_from_str(input)
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("streaming deserializer parses");
    assert_eq!(streamed, docs);
}

#[test]
fn yaml_11_schema_preserves_source_spelling_for_string_targets() {
    #[derive(Deserialize)]
    struct Config<'a> {
        flag: &'a str,
        count: &'a str,
    }

    let config: Config<'_> = LoadOptions::yaml_1_1()
        .from_str("flag: ON\ncount: 0x10\n")
        .expect("source-backed strings deserialize");
    assert_eq!(config.flag, "ON");
    assert_eq!(config.count, "0x10");
}
