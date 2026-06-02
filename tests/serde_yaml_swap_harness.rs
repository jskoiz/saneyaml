use saneyaml::{LoadOptions, Mapping, Number, Timestamp, Value};
use serde::{Deserialize, Serialize, de::IgnoredAny};
use std::collections::BTreeMap;
use std::io::Cursor;

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

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyYaml11Migration {
    flag: bool,
    truthy: bool,
    hex: i64,
    octal: i64,
    clock: i64,
    date: Timestamp,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
enum Action {
    Unit,
    Shell { run: String },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct SingletonActionConfig {
    #[serde(with = "saneyaml::with::singleton_map")]
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

    let from_str: AppConfig = saneyaml::from_str(input).expect("yaml from_str");
    let from_slice: AppConfig = saneyaml::from_slice(input.as_bytes()).expect("yaml from_slice");
    let from_reader: AppConfig =
        saneyaml::from_reader(Cursor::new(input.as_bytes())).expect("yaml from_reader");
    let direct: AppConfig = AppConfig::deserialize(saneyaml::Deserializer::from_str(input))
        .expect("yaml direct deserializer");
    let direct_slice: AppConfig =
        AppConfig::deserialize(saneyaml::Deserializer::from_slice(input.as_bytes()))
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
    let parsed = saneyaml::Deserializer::from_str(input)
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
fn swap_harness_ignored_any_entrypoints_validate_like_serde_yaml() {
    let valid = "name: api\n";
    IgnoredAny::deserialize(saneyaml::Deserializer::from_str(valid)).expect("yaml ignored valid");
    IgnoredAny::deserialize(serde_yaml::Deserializer::from_str(valid))
        .expect("serde_yaml ignored valid");

    let malformed = "[ok]\ntrailing: bad\n";
    let yaml_error = IgnoredAny::deserialize(saneyaml::Deserializer::from_str(malformed))
        .expect_err("yaml ignored malformed");
    let reference_error = IgnoredAny::deserialize(serde_yaml::Deserializer::from_str(malformed))
        .expect_err("serde_yaml ignored malformed");
    assert!(!yaml_error.to_string().is_empty());
    assert!(!reference_error.to_string().is_empty());

    let stream = "---\nname: api\n---\nname: worker\n";
    let yaml_error = IgnoredAny::deserialize(saneyaml::Deserializer::from_str(stream))
        .expect_err("yaml ignored stream");
    let reference_error = IgnoredAny::deserialize(serde_yaml::Deserializer::from_str(stream))
        .expect_err("serde_yaml ignored stream");
    assert!(yaml_error.to_string().contains("single YAML document"));
    assert!(!reference_error.to_string().is_empty());
}

#[test]
fn swap_harness_value_mapping_and_number_surface_covers_common_patches() {
    let input = "services:\n  api:\n    replicas: 2\n    image: example/api:1\n";
    let mut value: Value = saneyaml::from_str(input).expect("yaml value");
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
fn swap_harness_writer_paths_cover_structural_and_byte_compatible_surfaces() {
    let config = expected_app_config();
    let value = saneyaml::to_value(&config).expect("yaml to_value");
    let reference = serde_yaml::to_value(&config).expect("serde_yaml to_value");

    assert_eq!(value["name"].as_str(), reference["name"].as_str());
    assert_eq!(value["ports"][0].as_u64(), reference["ports"][0].as_u64());
    assert_eq!(
        value["env"]["CARGO_TERM_COLOR"].as_str(),
        reference["env"]["CARGO_TERM_COLOR"].as_str()
    );
    assert!(value["optional"].is_null());

    let output = saneyaml::to_string(&config).expect("yaml to_string");
    let reparsed: AppConfig = saneyaml::from_str(&output).expect("yaml output reparses");
    assert_eq!(reparsed, config);

    let mut buffer = Vec::new();
    saneyaml::to_writer(&mut buffer, &config).expect("yaml to_writer");
    let from_writer_output: AppConfig =
        saneyaml::from_slice(&buffer).expect("yaml writer output reparses");
    assert_eq!(from_writer_output, config);

    let reference_output = serde_yaml::to_string(&config).expect("serde_yaml to_string");
    let reference_reparsed: AppConfig =
        serde_yaml::from_str(&reference_output).expect("serde_yaml output reparses");
    assert_eq!(reference_reparsed, config);

    let byte_output =
        saneyaml::to_string_with_options(&config, saneyaml::EmitOptions::byte_compatible())
            .expect("yaml byte-compatible to_string");
    assert_eq!(byte_output, reference_output);

    let mut byte_buffer = Vec::new();
    saneyaml::to_writer_with_options(
        &mut byte_buffer,
        &config,
        saneyaml::EmitOptions::byte_compatible(),
    )
    .expect("yaml byte-compatible to_writer");
    assert_eq!(byte_buffer, reference_output.as_bytes());

    let mut stream = Vec::new();
    {
        let mut serializer = saneyaml::Serializer::with_options(
            &mut stream,
            saneyaml::EmitOptions::byte_compatible(),
        );
        config
            .serialize(&mut serializer)
            .expect("yaml byte-compatible stream");
    }
    assert_eq!(
        String::from_utf8(stream).expect("utf8 stream"),
        reference_output
    );
}

#[test]
fn swap_harness_singleton_map_helpers_cover_serde_yaml_with_paths() {
    let input = "action:\n  Shell:\n    run: cargo test\n";
    let parsed: SingletonActionConfig = saneyaml::from_str(input).expect("yaml singleton map");
    let reference: ReferenceSingletonActionConfig =
        serde_yaml::from_str(input).expect("serde_yaml singleton map");
    assert_eq!(parsed.action, reference.action);

    let value = saneyaml::to_value(&parsed).expect("yaml singleton to_value");
    let reference_value = serde_yaml::to_value(&reference).expect("serde_yaml singleton to_value");
    assert_eq!(
        value["action"]["Shell"]["run"].as_str(),
        reference_value["action"]["Shell"]["run"].as_str()
    );
}

#[test]
fn swap_harness_singleton_map_helpers_reject_tagged_shorthand_like_serde_yaml() {
    let input = "action: !Shell\n  run: cargo test\n";
    let yaml_error = saneyaml::from_str::<SingletonActionConfig>(input)
        .expect_err("yaml tagged helper rejection");
    let reference_error = serde_yaml::from_str::<ReferenceSingletonActionConfig>(input)
        .expect_err("serde_yaml tagged helper rejection");
    assert!(yaml_error.to_string().contains("invalid type"));
    assert!(reference_error.to_string().contains("invalid type"));
}

#[test]
fn swap_harness_merge_null_bytes_and_empty_input_decisions_are_explicit() {
    let null_input = "ports:\nlabels:\n";
    let parsed: DefaultedCollections = saneyaml::from_str(null_input).expect("yaml empty nodes");
    let reference: DefaultedCollections =
        serde_yaml::from_str(null_input).expect("serde_yaml empty nodes");
    assert_eq!(parsed, reference);
    assert!(parsed.ports.is_empty());
    assert!(parsed.labels.is_empty());

    let mut value: Value = saneyaml::from_str(
        "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
    )
    .expect("yaml merge value");
    let mut reference: serde_yaml::Value = serde_yaml::from_str(
        "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
    )
    .expect("serde_yaml merge value");
    assert!(value["job"]["<<"].is_null());
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));
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
    let value_bytes = saneyaml::to_value(&bytes).expect("yaml value bytes");
    let reference_bytes = serde_yaml::to_value(&bytes).expect("serde_yaml value bytes");
    assert_eq!(value_bytes.as_sequence().map(Vec::len), Some(3));
    assert_eq!(value_bytes[2].as_u64(), reference_bytes[2].as_u64());
    assert!(saneyaml::to_string(&bytes).is_err());
    assert!(serde_yaml::to_string(&bytes).is_err());

    let empty_value: Value = saneyaml::from_str("").expect("yaml empty input value");
    let reference_empty: serde_yaml::Value =
        serde_yaml::from_str("").expect("serde_yaml empty input value");
    assert!(empty_value.is_null());
    assert!(reference_empty.is_null());
    assert_eq!(saneyaml::Deserializer::from_str("").count(), 1);
    assert_eq!(serde_yaml::Deserializer::from_str("").count(), 1);
    let empty_stream: Vec<Value> = saneyaml::Deserializer::from_str("")
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("yaml empty stream");
    let reference_empty_stream = serde_yaml::Deserializer::from_str("")
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml empty stream");
    assert_eq!(empty_stream.len(), reference_empty_stream.len());
    assert!(empty_stream[0].is_null());
    assert!(reference_empty_stream[0].is_null());
    let direct_empty: Value =
        Value::deserialize(saneyaml::Deserializer::from_str("")).expect("yaml direct empty value");
    let reference_direct_empty =
        serde_yaml::Value::deserialize(serde_yaml::Deserializer::from_str(""))
            .expect("serde_yaml direct empty value");
    assert!(direct_empty.is_null());
    assert!(reference_direct_empty.is_null());
}

#[test]
fn swap_harness_default_merge_expansion_is_a_migration_decision() {
    let input = "\
defaults: &defaults
  retries: 3
  timeout: 10
job:
  <<: *defaults
  name: deploy
";
    let parsed: Value = saneyaml::from_str(input).expect("yaml merge-expanded value");
    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml literal merge value");

    assert_eq!(parsed["job"]["retries"].as_u64(), Some(3));
    assert_eq!(parsed["job"]["timeout"].as_u64(), Some(10));
    assert!(parsed["job"]["<<"].is_null());

    assert!(reference["job"]["retries"].is_null());
    assert_eq!(reference["job"]["<<"]["retries"].as_u64(), Some(3));
    reference.apply_merge().expect("serde_yaml apply_merge");
    assert_eq!(reference["job"]["retries"].as_u64(), Some(3));
    assert_eq!(reference["job"]["timeout"].as_u64(), Some(10));

    let mut merge_source = Mapping::new();
    merge_source.insert(Value::from("retries"), Value::from(3u64));
    merge_source.insert(Value::from("timeout"), Value::from(30u64));
    let mut caller_job = Mapping::new();
    caller_job.insert(Value::from("<<"), Value::Mapping(merge_source));
    caller_job.insert(Value::from("timeout"), Value::from(10u64));
    let mut caller_root = Mapping::new();
    caller_root.insert(Value::from("job"), Value::Mapping(caller_job));
    let caller_built = Value::Mapping(caller_root);

    let from_value: BTreeMap<String, BTreeMap<String, u64>> =
        saneyaml::from_value(caller_built.clone()).expect("caller-built from_value merge");
    let by_ref: BTreeMap<String, BTreeMap<String, u64>> =
        BTreeMap::deserialize(&caller_built).expect("caller-built &Value merge");
    assert_eq!(from_value["job"]["retries"], 3);
    assert_eq!(from_value["job"]["timeout"], 10);
    assert_eq!(by_ref, from_value);
    assert!(caller_built["job"]["<<"].is_mapping());
}

#[test]
fn swap_harness_yaml_11_schema_mode_is_an_explicit_migration_choice() {
    let input = "\
%YAML 1.1
---
flag: ON
truthy: yes
hex: 0x10
octal: 0123
clock: 1:20
date: 2026-05-24
";
    let default: Value = saneyaml::from_str(input).expect("default YAML 1.2-oriented value");
    assert_eq!(default["flag"].as_str(), Some("ON"));
    assert_eq!(default["truthy"].as_str(), Some("yes"));
    assert_eq!(default["hex"].as_str(), Some("0x10"));
    assert_eq!(default["octal"].as_i64(), Some(123));
    assert_eq!(default["clock"].as_str(), Some("1:20"));
    assert_eq!(default["date"].as_str(), Some("2026-05-24"));
    assert!(default["date"].as_timestamp().is_none());

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml partial legacy value");
    assert_eq!(reference["flag"].as_str(), Some("ON"));
    assert_eq!(reference["truthy"].as_str(), Some("yes"));
    assert_eq!(reference["hex"].as_i64(), Some(16));
    assert_eq!(reference["octal"].as_str(), Some("0123"));
    assert_eq!(reference["clock"].as_str(), Some("1:20"));
    assert_eq!(reference["date"].as_str(), Some("2026-05-24"));

    let expected = LegacyYaml11Migration {
        flag: true,
        truthy: true,
        hex: 16,
        octal: 83,
        clock: 4800,
        date: Timestamp::parse_yaml_1_1("2026-05-24").expect("date timestamp"),
    };
    let directive: Value = LoadOptions::yaml_version_directive()
        .from_str(input)
        .expect("directive-driven YAML 1.1 value");
    assert_eq!(directive["flag"].as_bool(), Some(true));
    assert_eq!(directive["truthy"].as_bool(), Some(true));
    assert_eq!(directive["hex"].as_i64(), Some(16));
    assert_eq!(directive["octal"].as_i64(), Some(83));
    assert_eq!(directive["clock"].as_i64(), Some(4800));
    assert_eq!(directive["date"].as_timestamp(), Some(expected.date));

    let typed: LegacyYaml11Migration = LoadOptions::yaml_version_directive()
        .from_str(input)
        .expect("directive-driven typed YAML 1.1 config");
    let direct = LegacyYaml11Migration::deserialize(
        LoadOptions::yaml_version_directive().deserializer_from_str(input),
    )
    .expect("direct directive-driven typed YAML 1.1 config");
    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
}

#[test]
fn swap_harness_real_world_fixtures_match_serde_yaml_on_migration_fields() {
    let github_actions = include_str!("fixtures/real-world/github-actions/matrix-ci.yaml");
    let yaml_workflow: Value =
        saneyaml::from_str(github_actions).expect("yaml GitHub Actions value");
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
    let yaml_compose: Value = saneyaml::from_str(compose).expect("yaml compose value");
    let serde_compose: serde_yaml::Value =
        serde_yaml::from_str(compose).expect("serde_yaml compose value");
    assert_eq!(
        yaml_compose["services"]["api"]["environment"]["RUST_LOG"].as_str(),
        serde_compose["services"]["api"]["environment"]["RUST_LOG"].as_str()
    );

    let kubernetes = include_str!("fixtures/real-world/kubernetes/multi-doc.yaml");
    let yaml_docs: Vec<Value> = saneyaml::from_documents_str(kubernetes).expect("yaml k8s docs");
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

    let helm = include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml");
    let yaml_helm: Value = saneyaml::from_str(helm).expect("yaml Helm value");
    let serde_helm: serde_yaml::Value = serde_yaml::from_str(helm).expect("serde_yaml Helm value");
    assert_eq!(
        yaml_helm["apiVersion"].as_str(),
        serde_helm["apiVersion"].as_str()
    );
    assert_eq!(
        yaml_helm["appVersion"].as_str(),
        serde_helm["appVersion"].as_str()
    );

    let openapi = include_str!("fixtures/real-world/openapi/upstream-petstore.yaml");
    let yaml_openapi: Value = saneyaml::from_str(openapi).expect("yaml OpenAPI value");
    let serde_openapi: serde_yaml::Value =
        serde_yaml::from_str(openapi).expect("serde_yaml OpenAPI value");
    assert_eq!(
        yaml_openapi["paths"]["/pets"]["get"]["operationId"].as_str(),
        serde_openapi["paths"]["/pets"]["get"]["operationId"].as_str()
    );
    assert_eq!(
        yaml_openapi["components"]["schemas"]["Pets"]["maxItems"].as_u64(),
        serde_openapi["components"]["schemas"]["Pets"]["maxItems"].as_u64()
    );

    let wrangler = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let yaml_wrangler: Value = saneyaml::from_str(wrangler).expect("yaml wrangler value");
    let serde_wrangler: serde_yaml::Value =
        serde_yaml::from_str(wrangler).expect("serde_yaml wrangler value");
    assert_eq!(
        yaml_wrangler["name"].as_str(),
        serde_wrangler["name"].as_str()
    );

    let adapted_wrangler =
        include_str!("fixtures/real-world/cloudflare/adapted-durable-objects-wrangler.yaml");
    let yaml_adapted_wrangler: Value =
        saneyaml::from_str(adapted_wrangler).expect("yaml adapted wrangler value");
    let serde_adapted_wrangler: serde_yaml::Value =
        serde_yaml::from_str(adapted_wrangler).expect("serde_yaml adapted wrangler value");
    assert_eq!(
        yaml_adapted_wrangler["durable_objects"]["bindings"][1]["class_name"].as_str(),
        serde_adapted_wrangler["durable_objects"]["bindings"][1]["class_name"].as_str()
    );

    let ansible = include_str!("fixtures/real-world/ansible/upstream-lamp-simple-site.yml");
    let yaml_ansible: Value = saneyaml::from_str(ansible).expect("yaml Ansible value");
    let serde_ansible: serde_yaml::Value =
        serde_yaml::from_str(ansible).expect("serde_yaml Ansible value");
    assert_eq!(yaml_ansible.as_sequence().map(Vec::len), Some(3));
    assert_eq!(
        yaml_ansible[2]["roles"][0].as_str(),
        serde_ansible[2]["roles"][0].as_str()
    );
}
