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

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyDocument {
    flag: bool,
    octal: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MergeRoot {
    job: MergeJob,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MergeJob {
    retries: u64,
    timeout: u64,
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

    let document_values: Vec<Config> =
        serde_yaml::from_documents_str("---\nname: root-one\nports: [5]\n---\nname: root-two\nports: [6]\n")
            .unwrap();
    let document_values_from_slice: Vec<Config> = serde_yaml::from_documents_slice(
        b"---\nname: slice-one\nports: [7]\n---\nname: slice-two\nports: [8]\n",
    )
    .unwrap();
    let document_values_from_reader: Vec<Config> = serde_yaml::from_documents_reader(Cursor::new(
        b"---\nname: reader-one\nports: [9]\n---\nname: reader-two\nports: [10]\n",
    ))
    .unwrap();
    assert_eq!(document_values[1].name, "root-two");
    assert_eq!(document_values_from_slice[0].ports, [7]);
    assert_eq!(document_values_from_reader[1].ports, [10]);

    let legacy_docs: Vec<LegacyDocument> = serde_yaml::LoadOptions::yaml_version_directive()
        .from_documents_str(
            "%YAML 1.1\n---\nflag: ON\noctal: 012\n---\nflag: true\noctal: 12\n",
        )
        .unwrap();
    assert_eq!(
        legacy_docs,
        vec![
            LegacyDocument {
                flag: true,
                octal: 10,
            },
            LegacyDocument {
                flag: true,
                octal: 12,
            },
        ]
    );

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

    let mut entry_mapping = serde_yaml::Mapping::new();
    entry_mapping.insert("image".into(), "nginx".into());
    match entry_mapping.entry("replicas".into()) {
        serde_yaml::mapping::Entry::Vacant(entry) => {
            assert_eq!(entry.key().as_str(), Some("replicas"));
            entry.insert(serde_yaml::Value::from(2u64));
        }
        serde_yaml::mapping::Entry::Occupied(_) => panic!("replicas should start vacant"),
    }
    match entry_mapping.entry("image".into()) {
        serde_yaml::mapping::Entry::Occupied(mut entry) => {
            assert_eq!(entry.key().as_str(), Some("image"));
            assert_eq!(entry.get().as_str(), Some("nginx"));
            assert_eq!(entry.insert("nginx:latest".into()).as_str(), Some("nginx"));
        }
        serde_yaml::mapping::Entry::Vacant(_) => panic!("image should start occupied"),
    }
    assert_eq!(entry_mapping["image"].as_str(), Some("nginx:latest"));
    assert_eq!(entry_mapping.get("replicas").and_then(|value| value.as_u64()), Some(2));
    let keys = entry_mapping
        .keys()
        .filter_map(|key| key.as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"image"));
    assert!(keys.contains(&"replicas"));

    let sequence: serde_yaml::Sequence = vec!["api".into(), "worker".into()];
    assert_eq!(sequence[1].as_str(), Some("worker"));
    let sequenced = serde_yaml::Value::Sequence(sequence);
    assert_eq!(sequenced[0].as_str(), Some("api"));

    let mut merge_source = serde_yaml::Mapping::new();
    merge_source.insert("retries".into(), 3u64.into());
    merge_source.insert("timeout".into(), 30u64.into());
    let mut caller_built_job = serde_yaml::Mapping::new();
    caller_built_job.insert("<<".into(), serde_yaml::Value::Mapping(merge_source));
    caller_built_job.insert("timeout".into(), 10u64.into());
    let caller_built = serde_yaml::Value::Mapping(
        [("job".into(), serde_yaml::Value::Mapping(caller_built_job))]
            .into_iter()
            .collect(),
    );
    let decoded_merge: MergeRoot = serde_yaml::from_value(caller_built.clone()).unwrap();
    let decoded_merge_module: MergeRoot =
        serde_yaml::value::from_value(caller_built.clone()).unwrap();
    let decoded_merge_ref = MergeRoot::deserialize(&caller_built).unwrap();
    assert_eq!(decoded_merge.job.retries, 3);
    assert_eq!(decoded_merge.job.timeout, 10);
    assert_eq!(decoded_merge_module, decoded_merge);
    assert_eq!(decoded_merge_ref, decoded_merge);
    assert!(caller_built["job"]["<<"].is_mapping());

    let mut explicit_merge = caller_built.clone();
    explicit_merge.apply_merge().unwrap();
    assert_eq!(explicit_merge["job"]["retries"].as_u64(), Some(3));
    assert_eq!(explicit_merge["job"]["timeout"].as_u64(), Some(10));
    assert!(explicit_merge["job"]["<<"].is_null());

    let lossless = serde_yaml::parse_lossless("base: &base\n  item: 1\ncopy: *base\n").unwrap();
    assert_eq!(lossless.aliases().len(), 1);
    let alias = &lossless.aliases()[0];
    let target = lossless.anchor(alias.target()).unwrap();
    assert_eq!(alias.name(), "base");
    assert_eq!(target.name(), "base");
    assert!(matches!(
        lossless.node(alias.node()).unwrap().kind(),
        serde_yaml::LosslessNodeKind::Alias { target: alias_target, .. }
            if *alias_target == target.id()
    ));

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
    let location: serde_yaml::Location = error.location().unwrap();
    assert_eq!(location.line(), 1);
    assert_eq!(location.column(), 8);
    assert_eq!(location.index(), 7);
}
