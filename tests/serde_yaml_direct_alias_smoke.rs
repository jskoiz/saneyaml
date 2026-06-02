use saneyaml as serde_yaml;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct AppConfig {
    name: String,
    ports: Vec<u16>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Action {
    Unit,
    Newtype(String),
    Tuple(u8, u8),
    Shell { run: String },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct SingletonConfig {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: Action,
}

fn alias_value_get<I>(value: &serde_yaml::Value, index: I) -> Option<&serde_yaml::Value>
where
    I: serde_yaml::Index,
{
    value.get(index)
}

fn alias_mapping_get<I>(mapping: &serde_yaml::Mapping, index: I) -> Option<&serde_yaml::Value>
where
    I: serde_yaml::mapping::Index,
{
    mapping.get(index)
}

#[test]
fn direct_alias_common_entrypoints_and_value_surface() -> serde_yaml::Result<()> {
    let input = "name: api\nports: [80, 443]\nenv:\n  RUST_LOG: info\n";
    let from_str: AppConfig = serde_yaml::from_str(input)?;
    let from_slice: AppConfig = serde_yaml::from_slice(input.as_bytes())?;
    let from_reader: AppConfig = serde_yaml::from_reader(Cursor::new(input.as_bytes()))?;
    let direct: AppConfig = AppConfig::deserialize(serde_yaml::Deserializer::from_str(input))?;

    assert_eq!(from_str, from_slice);
    assert_eq!(from_str, from_reader);
    assert_eq!(from_str, direct);
    assert_eq!(from_str.env["RUST_LOG"], "info");

    let mut value: serde_yaml::Value = serde_yaml::from_str(input)?;
    value["ports"][0] = serde_yaml::Value::from(8080_u64);
    value["env"]["RUST_LOG"] = serde_yaml::Value::from("debug");
    value["env"][0] = serde_yaml::Value::from("numeric-key");

    assert_eq!(
        alias_value_get(&value, "name").and_then(|v| v.as_str()),
        Some("api")
    );
    assert_eq!(value["ports"][0].as_u64(), Some(8080));
    assert_eq!(value["env"]["RUST_LOG"].as_str(), Some("debug"));
    assert_eq!(value["env"][0].as_str(), Some("numeric-key"));

    let sequence: serde_yaml::Sequence = vec!["api".into(), "worker".into()];
    assert_eq!(
        serde_yaml::Value::Sequence(sequence)[1].as_str(),
        Some("worker")
    );

    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert("image".into(), "nginx".into());
    mapping.entry("replicas".into()).or_insert(2_u64.into());
    mapping["replicas"] = serde_yaml::Value::Number(serde_yaml::Number::from(3_u64));
    assert_eq!(
        alias_mapping_get(&mapping, "image").and_then(|v| v.as_str()),
        Some("nginx")
    );
    assert_eq!(mapping["replicas"].as_u64(), Some(3));
    assert!(serde_yaml::Number::from(f64::INFINITY).is_infinite());

    let value_from_root = serde_yaml::to_value(&from_str)?;
    let value_from_module = serde_yaml::value::to_value(&from_str)?;
    let value_from_serializer = from_str.serialize(serde_yaml::value::Serializer)?;
    assert_eq!(value_from_root, value_from_module);
    assert_eq!(value_from_root, value_from_serializer);
    assert_eq!(
        serde_yaml::value::from_value::<AppConfig>(value_from_module)?,
        from_str
    );
    assert_eq!(
        serde_yaml::from_value::<AppConfig>(value_from_root)?,
        from_str
    );

    Ok(())
}

#[test]
fn direct_alias_tagged_enums_and_singleton_helpers() -> serde_yaml::Result<()> {
    let actions: Vec<Action> = serde_yaml::from_str(
        "- Unit\n- !Unit\n- !Newtype deploy\n- !Tuple [4, 2]\n- !Shell {run: cargo test}\n",
    )?;
    assert_eq!(actions[0], Action::Unit);
    assert_eq!(actions[1], Action::Unit);
    assert_eq!(actions[2], Action::Newtype("deploy".to_owned()));
    assert_eq!(actions[3], Action::Tuple(4, 2));
    assert_eq!(
        actions[4],
        Action::Shell {
            run: "cargo test".to_owned()
        }
    );
    assert!(
        serde_yaml::to_string(&Action::Shell {
            run: "cargo test".to_owned(),
        })?
        .contains("!Shell")
    );

    let config: SingletonConfig = serde_yaml::from_str("action:\n  Shell:\n    run: cargo test\n")?;
    assert_eq!(
        config.action,
        Action::Shell {
            run: "cargo test".to_owned()
        }
    );
    assert_eq!(
        serde_yaml::to_value(&config)?["action"]["Shell"]["run"].as_str(),
        Some("cargo test")
    );
    assert!(
        serde_yaml::from_str::<SingletonConfig>("action: !Shell\n  run: cargo test\n").is_err()
    );

    let recursive = vec![
        Action::Shell {
            run: "cargo test".to_owned(),
        },
        Action::Unit,
    ];
    let mut output = Vec::new();
    {
        let mut serializer = serde_yaml::Serializer::new(&mut output);
        serde_yaml::with::singleton_map_recursive::serialize(&recursive, &mut serializer)?;
    }
    let deserializer = serde_yaml::Deserializer::from_slice(&output);
    let roundtrip: Vec<Action> =
        serde_yaml::with::singleton_map_recursive::deserialize(deserializer)?;
    assert_eq!(roundtrip, recursive);

    Ok(())
}

#[test]
fn direct_alias_multi_doc_and_error_location() -> serde_yaml::Result<()> {
    let stream = "---\nname: api\nports: [80]\n---\nname: worker\nports: [8080]\n";
    let services = serde_yaml::Deserializer::from_str(stream)
        .map(AppConfig::deserialize)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(services.len(), 2);
    assert_eq!(services[1].name, "worker");

    let via_helper: Vec<AppConfig> = serde_yaml::from_documents_str(stream)?;
    assert_eq!(via_helper, services);

    let output = serde_yaml::to_string(&services[0])?;
    let reparsed: AppConfig = serde_yaml::from_str(&output)?;
    assert_eq!(reparsed, services[0]);

    let mut writer = Vec::new();
    serde_yaml::to_writer(&mut writer, &services[1])?;
    let writer_reparsed: AppConfig = serde_yaml::from_slice(&writer)?;
    assert_eq!(writer_reparsed, services[1]);

    let error: serde_yaml::Error = serde_yaml::from_str::<AppConfig>("name: [").unwrap_err();
    let location: serde_yaml::Location = error.location().expect("syntax location");
    assert_eq!(location.index(), 7);
    assert_eq!(location.line(), 1);
    assert_eq!(location.column(), 8);
    assert_eq!(error.line(), Some(1));
    assert_eq!(error.column(), Some(8));

    Ok(())
}
