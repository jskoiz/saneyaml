use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Config {
    name: String,
    ports: Vec<u16>,
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
    let config: Config = serde_yaml::from_str("name: api\nports: [80, 443]\n").unwrap();
    assert_eq!(config.name, "api");
    assert_eq!(config.ports, [80, 443]);

    let from_slice: Config = serde_yaml::from_slice(b"name: worker\nports: [8080]\n").unwrap();
    assert_eq!(from_slice.name, "worker");

    let from_reader: Config =
        serde_yaml::from_reader(Cursor::new(b"name: reader\nports: [9000]\n")).unwrap();
    assert_eq!(from_reader.ports, [9000]);

    let direct_from_str =
        Config::deserialize(serde_yaml::Deserializer::from_str("name: direct\nports: [7000]\n"))
            .unwrap();
    let direct_from_slice = Config::deserialize(serde_yaml::Deserializer::from_slice(
        b"name: slice-direct\nports: [7001]\n",
    ))
    .unwrap();
    let direct_from_reader = Config::deserialize(serde_yaml::Deserializer::from_reader(
        Cursor::new(b"name: reader-direct\nports: [7002]\n"),
    ))
    .unwrap();
    assert_eq!(direct_from_str.ports, [7000]);
    assert_eq!(direct_from_slice.ports, [7001]);
    assert_eq!(direct_from_reader.ports, [7002]);

    let stream_docs = serde_yaml::Deserializer::from_str(
        "---\nname: first\nports: [1]\n---\nname: second\nports: [2]\n",
    )
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(stream_docs.len(), 2);
    assert_eq!(stream_docs[1].name, "second");

    let reader_stream_docs = serde_yaml::Deserializer::from_reader(Cursor::new(
        b"---\nname: first-reader\nports: [3]\n---\nname: second-reader\nports: [4]\n",
    ))
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(reader_stream_docs[0].ports, [3]);
    assert_eq!(reader_stream_docs[1].ports, [4]);

    let value: serde_yaml::Value =
        serde_yaml::from_str("defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n")
            .unwrap();
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));

    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert(
        serde_yaml::Value::from("answer"),
        serde_yaml::Value::Number(serde_yaml::Number::from(42u64)),
    );
    let mapped = serde_yaml::Value::Mapping(mapping);
    assert_eq!(mapped["answer"].as_u64(), Some(42));

    let value_from_root = serde_yaml::to_value(&config).unwrap();
    let value_from_module = serde_yaml::value::to_value(&config).unwrap();
    let value_from_serializer = config.serialize(serde_yaml::value::Serializer).unwrap();
    let config_from_value: Config = serde_yaml::from_value(value_from_root.clone()).unwrap();
    let config_from_value_module: Config =
        serde_yaml::value::from_value(value_from_module.clone()).unwrap();
    assert_eq!(value_from_root["name"].as_str(), Some("api"));
    assert_eq!(value_from_module["ports"][1].as_u64(), Some(443));
    assert_eq!(value_from_serializer["name"].as_str(), Some("api"));
    assert_eq!(config_from_value, config);
    assert_eq!(config_from_value_module, config);

    let singleton: SingletonConfig =
        serde_yaml::from_str("action:\n  Shell:\n    run: cargo test\n").unwrap();
    assert_eq!(
        singleton.action,
        Action::Shell {
            run: "cargo test".to_string()
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
                run: "cargo test".to_string()
            },
            Action::Unit,
        ]
    );
    let recursive_value = serde_yaml::to_value(&recursive).unwrap();
    assert_eq!(
        recursive_value["actions"][0]["Shell"]["run"].as_str(),
        Some("cargo test")
    );

    let emitted = serde_yaml::to_string(&config).unwrap();
    assert!(emitted.contains("name: api"));
    let mut writer = Vec::new();
    serde_yaml::to_writer(&mut writer, &config).unwrap();
    assert!(String::from_utf8(writer).unwrap().contains("ports"));

    let mut stream = serde_yaml::Serializer::new(Vec::new());
    config.serialize(&mut stream).unwrap();
    singleton.serialize(&mut stream).unwrap();
    let stream_output = String::from_utf8(stream.into_inner().unwrap()).unwrap();
    assert!(stream_output.contains("---\n"));

    let error: serde_yaml::Error = serde_yaml::from_str::<Config>("name: [").unwrap_err();
    assert!(error.location().is_some());
}
