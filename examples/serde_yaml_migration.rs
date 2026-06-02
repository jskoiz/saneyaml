use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use yaml as serde_yaml;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct AppConfig {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Action {
    Unit,
    Shell { run: String },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct ActionConfig {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: Action,
}

fn main() -> serde_yaml::Result<()> {
    let input = "\
name: api
ports: [80, 443]
enabled: true
env:
  RUST_LOG: info
";

    let config: AppConfig = serde_yaml::from_str(input)?;
    assert_eq!(config.name.as_str(), "api");

    let mut value: serde_yaml::Value = serde_yaml::from_str(input)?;
    value["enabled"] = serde_yaml::Value::from(false);
    value["env"]["RUST_LOG"] = serde_yaml::Value::from("debug");

    let sequence: serde_yaml::Sequence = vec!["api".into(), "worker".into()];
    assert_eq!(
        serde_yaml::Value::Sequence(sequence)[1].as_str(),
        Some("worker")
    );

    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert(
        "replicas".into(),
        serde_yaml::Value::Number(serde_yaml::Number::from(3_u64)),
    );
    assert_eq!(mapping["replicas"].as_u64(), Some(3));

    let action: ActionConfig = serde_yaml::from_str("action:\n  Shell:\n    run: cargo test\n")?;
    assert_eq!(
        action.action,
        Action::Shell {
            run: "cargo test".to_owned()
        }
    );

    let stream = "---\nname: api\nports: [80]\nenabled: true\n---\nname: worker\nports: [8080]\nenabled: false\n";
    let services: Vec<AppConfig> = serde_yaml::from_documents_str(stream)?;
    assert_eq!(services.len(), 2);

    let output = serde_yaml::to_string(&config)?;
    let reparsed: AppConfig = serde_yaml::from_str(&output)?;
    assert_eq!(reparsed, config);

    let mut writer_output = Vec::new();
    serde_yaml::to_writer(&mut writer_output, &config)?;
    let from_writer: AppConfig = serde_yaml::from_slice(&writer_output)?;
    assert_eq!(from_writer, config);

    if let Err(error) = serde_yaml::from_str::<AppConfig>("name: [") {
        let error: serde_yaml::Error = error;
        let location: Option<serde_yaml::Location> = error.location();
        if let Some(location) = error.location() {
            eprintln!(
                "YAML error at line {}, column {}: {}",
                location.line(),
                location.column(),
                error
            );
        }
        assert!(location.is_some());
    }

    Ok(())
}
