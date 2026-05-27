use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct AppConfig {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn main() -> yaml::Result<()> {
    let input = "\
name: api
ports: [80, 443]
enabled: true
env:
  RUST_LOG: info
";

    let config: AppConfig = yaml::from_str(input)?;
    assert_eq!(config.name.as_str(), "api");

    let mut value: yaml::Value = yaml::from_str(input)?;
    value["enabled"] = yaml::Value::from(false);
    value["env"]["RUST_LOG"] = yaml::Value::from("debug");

    let stream = "---\nname: api\nports: [80]\nenabled: true\n---\nname: worker\nports: [8080]\nenabled: false\n";
    let services: Vec<AppConfig> = yaml::from_documents_str(stream)?;
    assert_eq!(services.len(), 2);

    let output = yaml::to_string(&config)?;
    let reparsed: AppConfig = yaml::from_str(&output)?;
    assert_eq!(reparsed, config);

    let mut writer_output = Vec::new();
    yaml::to_writer(&mut writer_output, &config)?;
    let from_writer: AppConfig = yaml::from_slice(&writer_output)?;
    assert_eq!(from_writer, config);

    if let Err(error) = yaml::from_str::<AppConfig>("name: [") {
        if let Some(location) = error.location() {
            eprintln!(
                "YAML error at line {}, column {}: {}",
                location.line(),
                location.column(),
                error
            );
        }
    }

    Ok(())
}
