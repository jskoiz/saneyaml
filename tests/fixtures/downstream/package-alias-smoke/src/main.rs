use serde::{Deserialize, Serialize};

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

fn main() {
    let config: Config = serde_yaml::from_str("name: api\nports: [80, 443]\n").unwrap();
    assert_eq!(config.name, "api");
    assert_eq!(config.ports, [80, 443]);

    let from_slice: Config = serde_yaml::from_slice(b"name: worker\nports: [8080]\n").unwrap();
    assert_eq!(from_slice.name, "worker");

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
    assert_eq!(value_from_root["name"].as_str(), Some("api"));
    assert_eq!(value_from_module["ports"][1].as_u64(), Some(443));
    assert_eq!(value_from_serializer["name"].as_str(), Some("api"));

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
