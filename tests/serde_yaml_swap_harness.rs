use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;
use yaml::{Mapping, Number, Value};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct AppConfig {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
    env: BTreeMap<String, String>,
    optional: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ServiceName {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct DefaultedCollections {
    #[serde(default)]
    ports: Vec<u16>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Action {
    Unit,
    Shell { run: String },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct SingletonActionConfig {
    #[serde(with = "yaml::with::singleton_map")]
    action: Action,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct ReferenceSingletonActionConfig {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: Action,
}

struct BytePayload(&'static [u8]);

impl Serialize for BytePayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.0)
    }
}

fn app_config_input() -> &'static str {
    "name: api\nports: [80, 443]\nenabled: true\nenv:\n  RUST_LOG: info\n  CARGO_TERM_COLOR: always\noptional: null\n"
}

fn expected_app_config() -> AppConfig {
    AppConfig {
        name: "api".to_string(),
        ports: vec![80, 443],
        enabled: true,
        env: BTreeMap::from([
            ("CARGO_TERM_COLOR".to_string(), "always".to_string()),
            ("RUST_LOG".to_string(), "info".to_string()),
        ]),
        optional: None,
    }
}

#[test]
fn swap_harness_typed_config_entrypoints_match_serde_yaml() {
    let input = app_config_input();
    let expected = expected_app_config();
    let reference: AppConfig = serde_yaml::from_str(input).expect("serde_yaml from_str");

    let from_str: AppConfig = yaml::from_str(input).expect("yaml from_str");
    let from_slice: AppConfig = yaml::from_slice(input.as_bytes()).expect("yaml from_slice");
    let from_reader: AppConfig =
        yaml::from_reader(Cursor::new(input.as_bytes())).expect("yaml from_reader");
    let direct: AppConfig = AppConfig::deserialize(yaml::Deserializer::from_str(input))
        .expect("yaml direct deserializer");
    let direct_slice: AppConfig =
        AppConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("yaml direct slice deserializer");

    assert_eq!(from_str, expected);
    assert_eq!(from_str, reference);
    assert_eq!(from_slice, reference);
    assert_eq!(from_reader, reference);
    assert_eq!(direct, reference);
    assert_eq!(direct_slice, reference);
}

#[test]
fn swap_harness_stream_deserializer_matches_serde_yaml_document_iteration() {
    let input = "---\nname: api\n---\nname: worker\n";
    let parsed = yaml::Deserializer::from_str(input)
        .map(ServiceName::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("yaml stream");
    let reference = serde_yaml::Deserializer::from_str(input)
        .map(ServiceName::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml stream");

    assert_eq!(parsed, reference);
    assert_eq!(
        parsed,
        vec![
            ServiceName {
                name: "api".to_string()
            },
            ServiceName {
                name: "worker".to_string()
            }
        ]
    );
}

#[test]
fn swap_harness_value_mapping_and_number_surface_covers_common_patches() {
    let input = "services:\n  api:\n    replicas: 2\n    image: example/api:1\n";
    let mut value: Value = yaml::from_str(input).expect("yaml value");
    let reference: serde_yaml::Value = serde_yaml::from_str(input).expect("serde_yaml value");

    assert_eq!(
        value["services"]["api"]["replicas"].as_u64(),
        reference["services"]["api"]["replicas"].as_u64()
    );
    assert_eq!(
        value["services"]["api"]["image"].as_str(),
        reference["services"]["api"]["image"].as_str()
    );

    value["services"]["api"]["replicas"] = Value::from(3u64);
    assert_eq!(value["services"]["api"]["replicas"].as_u64(), Some(3));

    let mut mapping = Mapping::new();
    assert_eq!(mapping.insert("name".into(), "api".into()), None);
    assert_eq!(mapping.insert("replicas".into(), 2u64.into()), None);
    assert_eq!(
        mapping.insert("replicas".into(), 3u64.into()),
        Some(2u64.into())
    );
    assert_eq!(mapping.get("name").and_then(Value::as_str), Some("api"));
    assert_eq!(mapping.get("replicas").and_then(Value::as_u64), Some(3));
    assert!(mapping.contains_key("name"));
    assert_eq!(mapping.len(), 2);

    let number = Number::from(42u64);
    let reference_number = serde_yaml::Number::from(42u64);
    assert_eq!(number.as_u64(), reference_number.as_u64());
    assert_eq!(number.to_string(), reference_number.to_string());
}

#[test]
fn swap_harness_writer_paths_are_structural_migration_surfaces() {
    let config = expected_app_config();
    let value = yaml::to_value(&config).expect("yaml to_value");
    let reference = serde_yaml::to_value(&config).expect("serde_yaml to_value");

    assert_eq!(value["name"].as_str(), reference["name"].as_str());
    assert_eq!(value["ports"][0].as_u64(), reference["ports"][0].as_u64());
    assert_eq!(
        value["env"]["CARGO_TERM_COLOR"].as_str(),
        reference["env"]["CARGO_TERM_COLOR"].as_str()
    );
    assert!(value["optional"].is_null());

    let output = yaml::to_string(&config).expect("yaml to_string");
    let reparsed: AppConfig = yaml::from_str(&output).expect("yaml output reparses");
    assert_eq!(reparsed, config);

    let mut buffer = Vec::new();
    yaml::to_writer(&mut buffer, &config).expect("yaml to_writer");
    let from_writer_output: AppConfig =
        yaml::from_slice(&buffer).expect("yaml writer output reparses");
    assert_eq!(from_writer_output, config);

    let reference_output = serde_yaml::to_string(&config).expect("serde_yaml to_string");
    let reference_reparsed: AppConfig =
        serde_yaml::from_str(&reference_output).expect("serde_yaml output reparses");
    assert_eq!(reference_reparsed, config);
}

#[test]
fn swap_harness_singleton_map_helpers_cover_serde_yaml_with_paths() {
    let input = "action:\n  Shell:\n    run: cargo test\n";
    let parsed: SingletonActionConfig = yaml::from_str(input).expect("yaml singleton map");
    let reference: ReferenceSingletonActionConfig =
        serde_yaml::from_str(input).expect("serde_yaml singleton map");
    assert_eq!(parsed.action, reference.action);

    let value = yaml::to_value(&parsed).expect("yaml singleton to_value");
    let reference_value = serde_yaml::to_value(&reference).expect("serde_yaml singleton to_value");
    assert_eq!(
        value["action"]["Shell"]["run"].as_str(),
        reference_value["action"]["Shell"]["run"].as_str()
    );
}

#[test]
fn swap_harness_merge_null_bytes_and_empty_input_decisions_are_explicit() {
    let null_input = "ports:\nlabels:\n";
    let parsed: DefaultedCollections = yaml::from_str(null_input).expect("yaml empty nodes");
    let reference: DefaultedCollections =
        serde_yaml::from_str(null_input).expect("serde_yaml empty nodes");
    assert_eq!(parsed, reference);
    assert!(parsed.ports.is_empty());
    assert!(parsed.labels.is_empty());

    let mut value: Value = yaml::from_str(
        "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
    )
    .expect("yaml merge value");
    let mut reference: serde_yaml::Value = serde_yaml::from_str(
        "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
    )
    .expect("serde_yaml merge value");
    value.apply_merge().expect("yaml apply_merge");
    reference.apply_merge().expect("serde_yaml apply_merge");
    assert_eq!(
        value["job"]["retries"].as_u64(),
        reference["job"]["retries"].as_u64()
    );
    assert_eq!(
        value["job"]["name"].as_str(),
        reference["job"]["name"].as_str()
    );

    let bytes = BytePayload(&[1, 2, 255]);
    let value_bytes = yaml::to_value(&bytes).expect("yaml value bytes");
    let reference_bytes = serde_yaml::to_value(&bytes).expect("serde_yaml value bytes");
    assert_eq!(value_bytes.as_sequence().map(Vec::len), Some(3));
    assert_eq!(value_bytes[2].as_u64(), reference_bytes[2].as_u64());
    assert!(yaml::to_string(&bytes).is_err());
    assert!(serde_yaml::to_string(&bytes).is_err());

    let empty_value: Value = yaml::from_str("").expect("yaml empty input value");
    let reference_empty: serde_yaml::Value =
        serde_yaml::from_str("").expect("serde_yaml empty input value");
    assert!(empty_value.is_null());
    assert!(reference_empty.is_null());
    assert_eq!(yaml::Deserializer::from_str("").count(), 0);
    assert_eq!(serde_yaml::Deserializer::from_str("").count(), 1);
    let direct_empty: Value =
        Value::deserialize(yaml::Deserializer::from_str("")).expect("yaml direct empty value");
    let reference_direct_empty =
        serde_yaml::Value::deserialize(serde_yaml::Deserializer::from_str(""))
            .expect("serde_yaml direct empty value");
    assert!(direct_empty.is_null());
    assert!(reference_direct_empty.is_null());
}

#[test]
fn swap_harness_real_world_fixtures_match_serde_yaml_on_migration_fields() {
    let github_actions = include_str!("fixtures/real-world/github-actions/matrix-ci.yaml");
    let yaml_workflow: Value = yaml::from_str(github_actions).expect("yaml GitHub Actions value");
    let serde_workflow: serde_yaml::Value =
        serde_yaml::from_str(github_actions).expect("serde_yaml GitHub Actions value");
    assert_eq!(
        yaml_workflow["on"]["push"]["branches"][1].as_str(),
        serde_workflow["on"]["push"]["branches"][1].as_str()
    );
    assert_eq!(
        yaml_workflow["jobs"]["test"]["strategy"]["matrix"]["node-version"][1].as_u64(),
        serde_workflow["jobs"]["test"]["strategy"]["matrix"]["node-version"][1].as_u64()
    );

    let compose = include_str!("fixtures/real-world/docker-compose/compose-polymorphic.yaml");
    let yaml_compose: Value = yaml::from_str(compose).expect("yaml compose value");
    let serde_compose: serde_yaml::Value =
        serde_yaml::from_str(compose).expect("serde_yaml compose value");
    assert_eq!(
        yaml_compose["services"]["api"]["environment"]["RUST_LOG"].as_str(),
        serde_compose["services"]["api"]["environment"]["RUST_LOG"].as_str()
    );

    let kubernetes = include_str!("fixtures/real-world/kubernetes/multi-doc.yaml");
    let yaml_docs: Vec<Value> = yaml::from_documents_str(kubernetes).expect("yaml k8s docs");
    let serde_docs = serde_yaml::Deserializer::from_str(kubernetes)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml k8s docs");
    assert_eq!(yaml_docs.len(), serde_docs.len());
    assert_eq!(
        yaml_docs[0]["kind"].as_str(),
        serde_docs[0]["kind"].as_str()
    );
    assert_eq!(
        yaml_docs[1]["metadata"]["name"].as_str(),
        serde_docs[1]["metadata"]["name"].as_str()
    );

    let wrangler = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let yaml_wrangler: Value = yaml::from_str(wrangler).expect("yaml wrangler value");
    let serde_wrangler: serde_yaml::Value =
        serde_yaml::from_str(wrangler).expect("serde_yaml wrangler value");
    assert_eq!(
        yaml_wrangler["name"].as_str(),
        serde_wrangler["name"].as_str()
    );
}
