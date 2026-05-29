use serde::{Deserialize, Serialize};
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

    let sequence: serde_yaml::Sequence = vec!["api".into(), "worker".into()];
    let sequenced = serde_yaml::Value::Sequence(sequence);
    assert_eq!(sequenced[0].as_str(), Some("api"));
    assert_eq!(sequenced[1].as_str(), Some("worker"));

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
