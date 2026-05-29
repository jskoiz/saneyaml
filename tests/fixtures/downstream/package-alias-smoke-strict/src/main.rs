use serde::{Deserialize, Serialize, de::IgnoredAny};
use std::{collections::BTreeMap, io::Cursor};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Config {
    name: String,
    ports: Vec<u16>,
    env: BTreeMap<String, String>,
    optional: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Action {
    Unit,
    Shell { run: String },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct SingletonConfig {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: Action,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct RecursiveSingletonConfig {
    #[serde(with = "serde_yaml::with::singleton_map_recursive")]
    actions: Vec<Action>,
}

#[derive(Debug, Deserialize, PartialEq)]
enum TaggedAction {
    Thing(String),
}

fn main() {
    let input = "name: api\nports: [80, 443]\nenv:\n  RUST_LOG: info\noptional: null\n";
    let config: Config = serde_yaml::from_str(input).unwrap();
    assert_eq!(config.name, "api");
    assert_eq!(config.ports, [80, 443]);
    assert_eq!(config.env["RUST_LOG"], "info");
    assert_eq!(config.optional, None);

    let from_slice: Config =
        serde_yaml::from_slice(b"name: worker\nports: [8080]\nenv: {}\noptional:\n").unwrap();
    assert_eq!(from_slice.name, "worker");

    let from_reader: Config = serde_yaml::from_reader(Cursor::new(
        b"name: reader\nports: [9000]\nenv: {MODE: test}\noptional: value\n",
    ))
    .unwrap();
    assert_eq!(from_reader.ports, [9000]);
    assert_eq!(from_reader.optional.as_deref(), Some("value"));

    let direct_from_str = Config::deserialize(serde_yaml::Deserializer::from_str(
        "name: direct\nports: [7000]\nenv: {}\noptional: null\n",
    ))
    .unwrap();
    assert_eq!(direct_from_str.ports, [7000]);

    let stream_docs = serde_yaml::Deserializer::from_str(
        "---\nname: first\nports: [1]\nenv: {}\noptional: null\n---\nname: second\nports: [2]\nenv: {}\noptional: null\n",
    )
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(stream_docs.len(), 2);
    assert_eq!(stream_docs[1].name, "second");

    let reader_stream_docs = serde_yaml::Deserializer::from_reader(Cursor::new(
        b"---\nname: first-reader\nports: [3]\nenv: {}\noptional: null\n---\nname: second-reader\nports: [4]\nenv: {}\noptional: null\n",
    ))
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(reader_stream_docs[0].ports, [3]);
    assert_eq!(reader_stream_docs[1].ports, [4]);

    let value: serde_yaml::Value = serde_yaml::from_str(input).unwrap();
    assert_eq!(value["name"].as_str(), Some("api"));
    assert_eq!(value["ports"][1].as_u64(), Some(443));
    assert!(value["optional"].is_null());

    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert("name".into(), "api".into());
    mapping.insert(
        "replicas".into(),
        serde_yaml::Value::Number(serde_yaml::Number::from(2_u64)),
    );
    match mapping.entry("image".into()) {
        serde_yaml::mapping::Entry::Vacant(entry) => {
            assert_eq!(entry.key().as_str(), Some("image"));
            entry.insert("nginx".into());
        }
        serde_yaml::mapping::Entry::Occupied(_) => panic!("image should start vacant"),
    }
    match mapping.entry("image".into()) {
        serde_yaml::mapping::Entry::Occupied(mut entry) => {
            assert_eq!(entry.get().as_str(), Some("nginx"));
            assert_eq!(entry.insert("nginx:latest".into()).as_str(), Some("nginx"));
        }
        serde_yaml::mapping::Entry::Vacant(_) => panic!("image should start occupied"),
    }
    let mapped = serde_yaml::Value::Mapping(mapping);
    assert_eq!(mapped["replicas"].as_u64(), Some(2));
    assert_eq!(mapped["image"].as_str(), Some("nginx:latest"));
    mapping_api_denominator_uses_package_alias();

    let sequence: serde_yaml::Sequence = vec!["api".into(), "worker".into()];
    let sequenced = serde_yaml::Value::Sequence(sequence);
    assert_eq!(sequenced[0].as_str(), Some("api"));
    assert_eq!(sequenced[1].as_str(), Some("worker"));

    let signed = serde_yaml::Number::from(-7_i64);
    let unsigned = serde_yaml::value::Number::from(7_u64);
    let finite = serde_yaml::Number::from(1.25_f64);
    let infinity = serde_yaml::Number::from(f64::INFINITY);
    let nan = serde_yaml::Number::from(f64::NAN);
    assert!(signed.is_i64());
    assert_eq!(signed.as_i64(), Some(-7));
    assert!(!signed.is_u64());
    assert!(unsigned.is_u64());
    assert_eq!(unsigned.as_u64(), Some(7));
    assert_eq!(unsigned.as_f64(), Some(7.0));
    assert!(finite.is_f64());
    assert_eq!(finite.as_f64(), Some(1.25));
    assert!(infinity.is_infinite());
    assert!(!infinity.is_finite());
    assert!(nan.is_nan());
    assert_eq!(nan, serde_yaml::Number::from(f64::NAN));

    let tag = serde_yaml::value::Tag::new("Thing");
    assert_eq!(tag, "Thing");
    assert_eq!(tag, "!Thing");
    assert_eq!(tag, serde_yaml::value::Tag::new("!Thing"));
    assert_eq!(tag.to_string(), "!Thing");

    let tagged_values: BTreeMap<String, serde_yaml::value::TaggedValue> =
        serde_yaml::from_str("scalar: !Thing x\nsequence: !Thing [first]\n").unwrap();
    assert_eq!(tagged_values["scalar"].tag, "Thing");
    assert_eq!(tagged_values["scalar"].value.as_str(), Some("x"));
    assert_eq!(tagged_values["sequence"].tag, "!Thing");
    assert_eq!(tagged_values["sequence"].value[0].as_str(), Some("first"));

    let tagged = serde_yaml::value::TaggedValue {
        tag: serde_yaml::value::Tag::new("Thing"),
        value: serde_yaml::Value::String("x".to_owned()),
    };
    let tagged_yaml = serde_yaml::to_string(&tagged).unwrap();
    assert!(tagged_yaml.contains("!Thing"));
    let tagged_roundtrip: serde_yaml::value::TaggedValue =
        serde_yaml::from_str(&tagged_yaml).unwrap();
    assert_eq!(tagged_roundtrip.tag, "Thing");
    assert_eq!(tagged_roundtrip.value.as_str(), Some("x"));
    let tagged_enum: TaggedAction = serde_yaml::from_str("!Thing enum-value\n").unwrap();
    assert_eq!(tagged_enum, TaggedAction::Thing("enum-value".to_owned()));
    let tagged_enum_from_owned: TaggedAction = TaggedAction::deserialize(tagged.clone()).unwrap();
    let tagged_enum_from_ref: TaggedAction = TaggedAction::deserialize(&tagged).unwrap();
    assert_eq!(
        tagged_enum_from_owned,
        TaggedAction::Thing("x".to_owned())
    );
    assert_eq!(tagged_enum_from_ref, tagged_enum_from_owned);
    IgnoredAny::deserialize(tagged.clone()).unwrap();
    IgnoredAny::deserialize(&tagged).unwrap();

    let value_from_root = serde_yaml::to_value(&config).unwrap();
    let value_from_module = serde_yaml::value::to_value(&config).unwrap();
    let value_from_serializer = config.serialize(serde_yaml::value::Serializer).unwrap();
    let config_from_value: Config = serde_yaml::from_value(value_from_root.clone()).unwrap();
    let config_from_value_module: Config =
        serde_yaml::value::from_value(value_from_module.clone()).unwrap();
    assert_eq!(value_from_root["name"].as_str(), Some("api"));
    assert_eq!(value_from_module["ports"][0].as_u64(), Some(80));
    assert_eq!(value_from_serializer["env"]["RUST_LOG"].as_str(), Some("info"));
    assert_eq!(config_from_value, config);
    assert_eq!(config_from_value_module, config);

    let singleton: SingletonConfig =
        serde_yaml::from_str("action:\n  Shell:\n    run: cargo test\n").unwrap();
    assert_eq!(
        singleton.action,
        Action::Shell {
            run: "cargo test".to_owned(),
        }
    );
    let singleton_value = serde_yaml::to_value(&singleton).unwrap();
    assert_eq!(
        singleton_value["action"]["Shell"]["run"].as_str(),
        Some("cargo test")
    );
    assert!(
        serde_yaml::from_str::<SingletonConfig>("action: !Shell\n  run: cargo test\n").is_err(),
        "singleton_map helper must reject tag-style enum shorthand"
    );

    let recursive: RecursiveSingletonConfig =
        serde_yaml::from_str("actions:\n  - Shell:\n      run: cargo test\n  - Unit\n").unwrap();
    assert_eq!(
        recursive.actions,
        vec![
            Action::Shell {
                run: "cargo test".to_owned(),
            },
            Action::Unit,
        ]
    );
    assert!(
        serde_yaml::from_str::<RecursiveSingletonConfig>(
            "actions:\n  - !Shell\n    run: cargo test\n"
        )
        .is_err(),
        "recursive singleton_map helper must reject nested tag-style enum shorthand"
    );

    let emitted = serde_yaml::to_string(&config).unwrap();
    let reparsed: Config = serde_yaml::from_str(&emitted).unwrap();
    assert_eq!(reparsed, config);

    let mut writer = Vec::new();
    serde_yaml::to_writer(&mut writer, &config).unwrap();
    let writer_reparsed: Config = serde_yaml::from_slice(&writer).unwrap();
    assert_eq!(writer_reparsed, config);

    let mut stream = Vec::new();
    {
        let mut serializer = serde_yaml::Serializer::new(&mut stream);
        config.serialize(&mut serializer).unwrap();
        singleton.serialize(&mut serializer).unwrap();
    }
    let stream_output = String::from_utf8(stream).unwrap();
    assert!(stream_output.contains("---\n"));

    let error: serde_yaml::Error = serde_yaml::from_str::<Config>("name: [").unwrap_err();
    let location: serde_yaml::Location = error.location().unwrap();
    assert_eq!(location.line(), 1);
    assert!(location.column() > 0);
}

fn mapping_api_denominator_uses_package_alias() {
    let mut mapping = package_alias_mapping_from_pairs();
    assert_eq!(
        mapping
            .remove("b")
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("beta".to_owned())
    );
    let removed = mapping.remove_entry("a").expect("remove a");
    assert_eq!(removed.0.as_str(), Some("a"));
    assert_eq!(removed.1.as_str(), Some("alpha"));
    assert_eq!(
        mapping
            .swap_remove(String::from("e"))
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("epsilon".to_owned())
    );
    let removed = mapping
        .swap_remove_entry(serde_yaml::Value::from("c"))
        .expect("swap remove c");
    assert_eq!(removed.0.as_str(), Some("c"));
    assert_eq!(removed.1.as_str(), Some("gamma"));
    assert!(mapping.remove("missing").is_none());
    assert_eq!(
        package_alias_mapping_pairs(&mapping),
        package_alias_expected_pairs(&[("d", "delta")])
    );

    let mut ordered = package_alias_mapping_from_pairs();
    assert_eq!(
        ordered
            .shift_remove("b")
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("beta".to_owned())
    );
    let removed = ordered
        .shift_remove_entry(serde_yaml::Value::from("d"))
        .expect("shift remove d");
    assert_eq!(removed.0.as_str(), Some("d"));
    assert_eq!(removed.1.as_str(), Some("delta"));
    assert!(ordered.shift_remove("missing").is_none());
    assert_eq!(
        package_alias_mapping_pairs(&ordered),
        package_alias_expected_pairs(&[("a", "alpha"), ("c", "gamma"), ("e", "epsilon")])
    );

    let mut retained = package_alias_mapping_from_pairs();
    for (_, value) in retained.iter_mut() {
        if value.as_str() == Some("beta") {
            *value = serde_yaml::Value::from("BETA");
        }
    }
    for value in retained.values_mut() {
        if value.as_str() == Some("delta") {
            *value = serde_yaml::Value::from("DELTA");
        }
    }
    retained.retain(|key, value| {
        if key.as_str() == Some("c") {
            *value = serde_yaml::Value::from("GAMMA");
        }
        key.as_str() != Some("a")
    });
    assert_eq!(
        package_alias_mapping_pairs(&retained),
        package_alias_expected_pairs(&[
            ("b", "BETA"),
            ("c", "GAMMA"),
            ("d", "DELTA"),
            ("e", "epsilon"),
        ])
    );

    let keys = package_alias_mapping_from_pairs()
        .into_keys()
        .map(|key| key.as_str().expect("string key").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "b", "c", "d", "e"]);
    let values = package_alias_mapping_from_pairs()
        .into_values()
        .map(|value| value.as_str().expect("string value").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(values, ["alpha", "beta", "gamma", "delta", "epsilon"]);

    retained.clear();
    assert!(retained.is_empty());
    assert_eq!(retained.len(), 0);

    let mut value = serde_yaml::Value::Null;
    value["services"]["api"]["image"] = serde_yaml::Value::from("nginx");
    value["services"]["api"]["ports"] = serde_yaml::from_str("[80, 443]").unwrap();
    value["services"]["api"]["ports"][0] = serde_yaml::Value::from(8080_u64);
    value["services"]["api"][0] = serde_yaml::Value::from("numeric-key");
    assert_eq!(value["services"]["api"]["image"].as_str(), Some("nginx"));
    assert_eq!(value["services"]["api"]["ports"][0].as_u64(), Some(8080));
    assert_eq!(value["services"]["api"][0].as_str(), Some("numeric-key"));
    assert!(value["services"]["api"]["missing"].is_null());
}

fn package_alias_mapping_from_pairs() -> serde_yaml::Mapping {
    [
        ("a", "alpha"),
        ("b", "beta"),
        ("c", "gamma"),
        ("d", "delta"),
        ("e", "epsilon"),
    ]
    .into_iter()
    .map(|(key, value)| (serde_yaml::Value::from(key), serde_yaml::Value::from(value)))
    .collect()
}

fn package_alias_mapping_pairs(mapping: &serde_yaml::Mapping) -> Vec<(String, String)> {
    mapping
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().expect("string key").to_owned(),
                value.as_str().expect("string value").to_owned(),
            )
        })
        .collect()
}

fn package_alias_expected_pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}
