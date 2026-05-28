use serde::Deserialize;
use yaml::{Date, LoadOptions, Schema, Tag, Time, TimeZoneOffset, Timestamp, Value};

#[test]
fn schema_mode_defaults_to_yaml_12_config_behavior() {
    let input =
        "%YAML 1.1\n---\non: push\nyes: deploy\nflag: ON\nhex: 0x7B\nsex: 1:20\ndate: 2026-05-24\n";
    let value: Value = yaml::from_str(input).expect("default schema parses");

    assert_eq!(value["on"].as_str(), Some("push"));
    assert_eq!(value["yes"].as_str(), Some("deploy"));
    assert_eq!(value["flag"].as_str(), Some("ON"));
    assert_eq!(value["hex"].as_str(), Some("0x7B"));
    assert_eq!(value["sex"].as_str(), Some("1:20"));
    assert_eq!(value["date"].as_str(), Some("2026-05-24"));
    assert!(value["date"].as_tagged().is_none());
}

#[test]
fn yaml_version_directive_schema_switches_each_document() {
    let input = "\
%YAML 1.1
---
flag: ON
count: 0x10
clock: 1:20
...
---
flag: ON
count: 0x10
clock: 1:20
...
%YAML 1.3
---
flag: ON
count: 0x10
clock: 1:20
";
    let options = LoadOptions::yaml_version_directive();
    let docs: Vec<Value> = options
        .from_documents_str(input)
        .expect("directive-driven documents deserialize");

    assert_eq!(docs.len(), 3);
    assert_eq!(docs[0]["flag"].as_bool(), Some(true));
    assert_eq!(docs[0]["count"].as_i64(), Some(16));
    assert_eq!(docs[0]["clock"].as_i64(), Some(4800));
    assert_eq!(docs[1]["flag"].as_str(), Some("ON"));
    assert_eq!(docs[1]["count"].as_str(), Some("0x10"));
    assert_eq!(docs[1]["clock"].as_str(), Some("1:20"));
    assert_eq!(docs[2]["flag"].as_str(), Some("ON"));
    assert_eq!(docs[2]["count"].as_str(), Some("0x10"));
    assert_eq!(docs[2]["clock"].as_str(), Some("1:20"));

    let streamed = options
        .deserializer_from_str(input)
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("directive-driven stream deserializes");
    assert_eq!(streamed, docs);

    let parsed = options
        .parse_documents(input)
        .expect("directive-driven parser documents");
    assert_eq!(Value::from(&parsed[0])["flag"].as_bool(), Some(true));
    assert_eq!(Value::from(&parsed[1])["flag"].as_str(), Some("ON"));
    assert_eq!(Value::from(&parsed[2])["flag"].as_str(), Some("ON"));
}

#[test]
fn yaml_version_directive_schema_supports_resolved_canonical_core_tags() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct ResolvedCoreTags {
        flag: bool,
        count: i64,
        clock: f64,
        date: Timestamp,
    }

    let input = "\
%YAML 1.1
%TAG !yaml! tag:yaml.org,2002:
---
flag: !yaml!bool ON
count: !yaml!int 0x10
clock: !yaml!float 1:20:30.5
date: !yaml!timestamp 2026-05-24
";
    let expected = ResolvedCoreTags {
        flag: true,
        count: 16,
        clock: 4830.5,
        date: Timestamp::parse_yaml_1_1("2026-05-24").expect("timestamp"),
    };

    let typed: ResolvedCoreTags = LoadOptions::yaml_version_directive()
        .from_str(input)
        .expect("directive-driven canonical core tags");
    let direct = ResolvedCoreTags::deserialize(
        LoadOptions::yaml_version_directive().deserializer_from_str(input),
    )
    .expect("direct directive-driven canonical core tags");
    let value: Value = LoadOptions::yaml_version_directive()
        .from_str(input)
        .expect("canonical core tag values");

    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
    assert_eq!(value["flag"].as_bool(), Some(true));
    assert_eq!(value["count"].as_i64(), Some(16));
    assert_eq!(value["clock"].as_f64(), Some(4830.5));
    assert_eq!(value["date"].as_timestamp(), Some(expected.date));

    let tagged = value["count"].as_tagged().expect("resolved int tag");
    assert_eq!(tagged.tag.handle, "!");
    assert_eq!(tagged.tag.suffix, "tag:yaml.org,2002:int");
}

#[test]
fn yaml_version_directive_schema_reports_legacy_duplicate_key_collisions() {
    let error = LoadOptions::yaml_version_directive()
        .parse_str("%YAML 1.1\n---\non: push\nyes: deploy\n")
        .expect_err("directive-driven YAML 1.1 keys collide");

    assert!(error.to_string().contains("duplicate mapping key `true`"));
    assert_eq!(error.span().line, 4);
    assert_eq!(error.span().column, 1);
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
short_sexagesimal: 1:20
negative_sexagesimal: -1:20
float_sexagesimal: 1:20.5
float_seconds: 1:20:30.5
invalid_sexagesimal: 1:60
too_many_sexagesimal: 1:20:30:40
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
    assert_eq!(value["short_sexagesimal"].as_i64(), Some(4800));
    assert_eq!(value["negative_sexagesimal"].as_i64(), Some(-2400));
    assert_eq!(value["float_sexagesimal"].as_f64(), Some(4830.0));
    assert_eq!(value["float_seconds"].as_f64(), Some(4830.5));
    assert_eq!(value["invalid_sexagesimal"].as_str(), Some("1:60"));
    assert_eq!(value["too_many_sexagesimal"].as_str(), Some("1:20:30:40"));
    assert_eq!(value["underscored"].as_i64(), Some(1000));
}

#[test]
fn yaml_11_schema_exposes_native_timestamp_api() {
    let default: Value = yaml::from_str("%YAML 1.1\n---\ndate: 2026-05-24\n")
        .expect("default schema accepts YAML 1.1 directive");
    assert_eq!(default["date"].as_str(), Some("2026-05-24"));
    assert!(default["date"].as_tagged().is_none());
    assert!(default["date"].as_timestamp().is_none());

    let value: Value = LoadOptions::yaml_1_1()
        .from_str(
            "\
date: 2026-05-24
short: 2026-5-4
datetime: 2026-05-24T12:34:56Z
spaced: 2026-05-24 12:34:56 -7
fractional: 2026-05-24t12:34:56.789+05:30
invalid_month: 2026-13-24
invalid_day: 2026-02-30
invalid_time: 2026-05-24T24:34:56Z
",
        )
        .expect("timestamp-shaped scalars parse");

    assert_yaml11_timestamp(
        &value["date"],
        "2026-05-24",
        Timestamp::new(Date::from_ymd(2026, 5, 24).expect("valid date"), None),
    );
    assert_yaml11_timestamp(
        &value["short"],
        "2026-5-4",
        Timestamp::new(Date::from_ymd(2026, 5, 4).expect("valid date"), None),
    );
    assert_yaml11_timestamp(
        &value["datetime"],
        "2026-05-24T12:34:56Z",
        Timestamp::new(
            Date::from_ymd(2026, 5, 24).expect("valid date"),
            Some(
                Time::from_hms_nano_offset(
                    12,
                    34,
                    56,
                    0,
                    Some(TimeZoneOffset::from_minutes(0).expect("valid offset")),
                )
                .expect("valid time"),
            ),
        ),
    );
    assert_yaml11_timestamp(
        &value["spaced"],
        "2026-05-24 12:34:56 -7",
        Timestamp::new(
            Date::from_ymd(2026, 5, 24).expect("valid date"),
            Some(
                Time::from_hms_nano_offset(
                    12,
                    34,
                    56,
                    0,
                    Some(TimeZoneOffset::from_minutes(-7 * 60).expect("valid offset")),
                )
                .expect("valid time"),
            ),
        ),
    );
    assert_yaml11_timestamp(
        &value["fractional"],
        "2026-05-24t12:34:56.789+05:30",
        Timestamp::new(
            Date::from_ymd(2026, 5, 24).expect("valid date"),
            Some(
                Time::from_hms_nano_offset(
                    12,
                    34,
                    56,
                    789_000_000,
                    Some(TimeZoneOffset::from_minutes(5 * 60 + 30).expect("valid offset")),
                )
                .expect("valid time"),
            ),
        ),
    );
    assert_eq!(value["invalid_month"].as_str(), Some("2026-13-24"));
    assert!(value["invalid_month"].as_tagged().is_none());
    assert_eq!(value["invalid_day"].as_str(), Some("2026-02-30"));
    assert!(value["invalid_day"].as_tagged().is_none());
    assert_eq!(value["invalid_time"].as_str(), Some("2026-05-24T24:34:56Z"));
    assert!(value["invalid_time"].as_tagged().is_none());

    #[derive(Deserialize)]
    struct Schedule {
        date: Timestamp,
        datetime: Timestamp,
    }
    let schedule: Schedule = LoadOptions::yaml_1_1()
        .from_str("date: 2026-05-24\ndatetime: 2026-05-24T12:34:56Z\n")
        .expect("typed timestamp fields deserialize");
    assert_eq!(
        schedule.date,
        value["date"].as_timestamp().expect("date timestamp")
    );
    assert_eq!(
        schedule.datetime,
        value["datetime"]
            .as_timestamp()
            .expect("datetime timestamp")
    );
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
        date: &'a str,
    }

    let config: Config<'_> = LoadOptions::yaml_1_1()
        .from_str("flag: ON\ncount: 0x10\ndate: 2026-05-24\n")
        .expect("source-backed strings deserialize");
    assert_eq!(config.flag, "ON");
    assert_eq!(config.count, "0x10");
    assert_eq!(config.date, "2026-05-24");
}

fn assert_yaml11_timestamp(value: &Value, expected: &str, timestamp: Timestamp) {
    assert_eq!(value.as_str(), Some(expected));
    assert_eq!(value.as_timestamp(), Some(timestamp));
    let tagged = value.as_tagged().expect("YAML 1.1 timestamp tag");
    assert_eq!(tagged.tag, Tag::new("!!timestamp"));
    assert_eq!(tagged.value.as_str(), Some(expected));
}
