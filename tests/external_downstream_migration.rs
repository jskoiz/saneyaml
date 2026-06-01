use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use yaml::{Tag, Value};

const SOURCE: &str = include_str!("fixtures/downstream/SOURCE.toml");
const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/downstream");
const STACKABLE_OPERATOR_CRDS: &[&str] = &[
    "stackable-operator/AuthenticationClass.yaml",
    "stackable-operator/DummyCluster.yaml",
    "stackable-operator/Listener.yaml",
    "stackable-operator/ListenerClass.yaml",
    "stackable-operator/PodListeners.yaml",
    "stackable-operator/S3Bucket.yaml",
    "stackable-operator/S3Connection.yaml",
    "stackable-operator/Scaler.yaml",
];

#[derive(Debug, Deserialize)]
struct DownstreamManifest {
    fixture: Vec<DownstreamFixture>,
}

#[derive(Debug, Deserialize)]
struct DownstreamFixture {
    project: String,
    #[serde(rename = "crate")]
    crate_name: String,
    version: String,
    repo: String,
    commit: String,
    license: String,
    source_path: String,
    local_path: String,
    yaml_surface: String,
    reduction_notes: String,
}

#[test]
fn external_downstream_manifest_records_provenance_and_files() {
    let manifest = downstream_manifest();
    assert_eq!(manifest.fixture.len(), 20);

    let projects: BTreeSet<_> = manifest
        .fixture
        .iter()
        .map(|fixture| fixture.project.as_str())
        .collect();
    assert_eq!(
        projects,
        BTreeSet::from([
            "aws-cloudformation/cloudformation-guard",
            "cloudflare/pingora",
            "denisidoro/navi",
            "longbridge/rust-i18n",
            "stackabletech/operator-rs",
        ])
    );

    for fixture in manifest.fixture {
        for (name, value) in [
            ("crate", &fixture.crate_name),
            ("version", &fixture.version),
            ("repo", &fixture.repo),
            ("commit", &fixture.commit),
            ("license", &fixture.license),
            ("source_path", &fixture.source_path),
            ("local_path", &fixture.local_path),
            ("yaml_surface", &fixture.yaml_surface),
            ("reduction_notes", &fixture.reduction_notes),
        ] {
            assert!(
                !value.trim().is_empty(),
                "{} fixture must record non-empty {name}",
                fixture.project
            );
        }
        assert!(
            matches!(fixture.license.as_str(), "Apache-2.0" | "MIT"),
            "{} must be permissively licensed",
            fixture.local_path
        );
        assert!(
            Path::new(FIXTURE_ROOT).join(&fixture.local_path).is_file(),
            "{} must exist",
            fixture.local_path
        );
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct PingoraServerConf {
    version: usize,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    pid_file: Option<PathBuf>,
    #[serde(default)]
    error_log: Option<PathBuf>,
    #[serde(default)]
    upgrade_sock: Option<PathBuf>,
    #[serde(default)]
    max_retries: Option<usize>,
    #[serde(default)]
    ca_file: Option<PathBuf>,
    #[serde(default)]
    client_bind_to_ipv4: Vec<Ipv4Addr>,
}

#[test]
fn external_pingora_server_configs_match_serde_yaml() {
    for path in [
        "pingora/pingora-core-pingora_conf.yaml",
        "pingora/pingora-proxy-pingora_conf.yaml",
        "pingora/pingora-proxy-example-conf.yaml",
    ] {
        let input = read_fixture(path);
        let parsed: PingoraServerConf = assert_yaml_matches_serde(&input);
        assert_eq!(parsed.version, 1, "{path}");

        let value = yaml::to_value(&parsed).expect("yaml to_value pingora config");
        let reference_value = serde_yaml::to_value(&parsed).expect("serde_yaml to_value pingora");
        assert_eq!(
            value["version"].as_u64(),
            reference_value["version"].as_u64(),
            "{path}"
        );

        let output = yaml::to_string(&parsed).expect("yaml to_string pingora config");
        let reparsed: PingoraServerConf =
            yaml::from_str(&output).expect("yaml pingora output reparses");
        assert_eq!(reparsed, parsed, "{path}");
        assert_typed_byte_writer_matches_serde(path, &parsed);
    }
}

#[test]
fn external_pingora_config_fields_cover_paths_ips_and_optional_scalars() {
    let core: PingoraServerConf = assert_yaml_matches_serde(include_str!(
        "fixtures/downstream/pingora/pingora-core-pingora_conf.yaml"
    ));
    assert_eq!(core.client_bind_to_ipv4, [Ipv4Addr::new(127, 0, 0, 2)]);
    assert_eq!(
        core.ca_file.as_deref(),
        Some(Path::new("tests/keys/server.crt"))
    );

    let example: PingoraServerConf = assert_yaml_matches_serde(include_str!(
        "fixtures/downstream/pingora/pingora-proxy-example-conf.yaml"
    ));
    assert_eq!(example.threads, Some(2));
    assert_eq!(example.max_retries, Some(5));
    assert_eq!(
        example.pid_file.as_deref(),
        Some(Path::new("/tmp/load_balancer.pid"))
    );
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum LocaleValue {
    String(String),
    Integer(i64),
    Mapping(BTreeMap<String, LocaleValue>),
}

#[test]
fn external_rust_i18n_locale_files_match_serde_yaml() {
    for path in [
        "rust-i18n/app-en.yml",
        "rust-i18n/app-fr.yml",
        "rust-i18n/user.en.yaml",
        "rust-i18n/v2.yml",
    ] {
        let input = read_fixture(path);
        let locale: BTreeMap<String, LocaleValue> = assert_yaml_matches_serde(&input);
        assert!(!locale.is_empty(), "{path}");
    }
}

#[test]
fn external_rust_i18n_unicode_and_placeholders_survive_migration() {
    let app_en: BTreeMap<String, LocaleValue> =
        assert_yaml_matches_serde(include_str!("fixtures/downstream/rust-i18n/app-en.yml"));
    assert_eq!(
        app_en.get("hello"),
        Some(&LocaleValue::String("Hello, %{name}!".to_owned()))
    );

    let user: BTreeMap<String, LocaleValue> =
        assert_yaml_matches_serde(include_str!("fixtures/downstream/rust-i18n/user.en.yaml"));
    let messages = match user.get("messages") {
        Some(LocaleValue::Mapping(messages)) => messages,
        other => panic!("messages must be a locale mapping: {other:?}"),
    };
    assert!(matches!(
        messages.get("user"),
        Some(LocaleValue::Mapping(_))
    ));

    let v2: BTreeMap<String, LocaleValue> =
        assert_yaml_matches_serde(include_str!("fixtures/downstream/rust-i18n/v2.yml"));
    let hello = lookup_locale_path(&v2, &["nested_locale_test", "hello", "ja"]);
    assert_eq!(hello, Some("こんにちは test2"));
    let message = lookup_locale_path(&v2, &["t_kmFrQ2nnJsvUh3Ckxmki0", "zh-CN"]);
    assert_eq!(message, Some("你好，%{name}。这是你的消息：%{msg}"));
}

#[test]
fn external_cfn_guard_cloudformation_template_matches_serde_yaml() {
    let value = assert_value_matches_serde(include_str!(
        "fixtures/downstream/cfn-guard/cfn-lambda.yaml"
    ));

    let service_token = &value["Resources"]["AllSecurityGroups"]["Properties"]["ServiceToken"];
    assert_tagged_scalar(
        service_token,
        "GetAtt",
        "AppendItemToListFunction.Arn",
        "CloudFormation service token",
    );

    let zip_file = &value["Resources"]["AppendItemToListFunction"]["Properties"]["Code"]["ZipFile"];
    let tagged = assert_tagged_scalar(zip_file, "Sub", "", "CloudFormation inline Lambda");
    assert!(
        tagged
            .value
            .as_str()
            .is_some_and(|source| source.contains("exports.handler = function(event, context)")),
        "inline Lambda body survives tagged block scalar"
    );

    let security_group_ids = &value["Resources"]["MyEC2Instance"]["Properties"]["SecurityGroupIds"];
    assert_tagged_scalar(
        security_group_ids,
        "GetAtt",
        "AllSecurityGroups.Value",
        "CloudFormation security group id lookup",
    );

    assert_value_writer_replays("cfn-guard/cfn-lambda.yaml", &value);
}

#[test]
fn external_cfn_guard_rule_test_specs_match_serde_yaml() {
    let test_spec = assert_value_matches_serde(include_str!(
        "fixtures/downstream/cfn-guard/test-command-test.yaml"
    ));
    assert_eq!(
        test_spec[0]["name"].as_str(),
        Some("CodeBuild project with safe environment variables, PASS")
    );
    assert_eq!(
        test_spec[0]["expectations"]["rules"]["REDSHIFT_ENCRYPTED_CMK"].as_str(),
        Some("PASS")
    );
    assert_tagged_scalar(
        &test_spec[0]["input"]["Resources"]["myCluster"]["Properties"]["KmsKeyId"]["Fn::ImportValue"],
        "Sub",
        "${pSecretKmsKey}",
        "nested CloudFormation import value",
    );

    let s3_spec = assert_value_matches_serde(include_str!(
        "fixtures/downstream/cfn-guard/s3-bucket-logging-enabled-tests.yaml"
    ));
    assert_eq!(
        s3_spec[0]["expectations"]["rules"]["S3_BUCKET_LOGGING_ENABLED"].as_str(),
        Some("SKIP")
    );
    assert_eq!(
        s3_spec[2]["expectations"]["rules"]["S3_BUCKET_LOGGING_ENABLED"].as_str(),
        Some("PASS")
    );
    assert_eq!(
        s3_spec[3]["expectations"]["rules"]["S3_BUCKET_LOGGING_ENABLED"].as_str(),
        Some("FAIL")
    );
    assert_tagged_scalar(
        &s3_spec[2]["input"]["Resources"]["ExampleS3"]["Properties"]["LoggingConfiguration"]["DestinationBucketName"],
        "Ref",
        "LoggingBucket",
        "S3 logging destination bucket",
    );

    assert_value_writer_replays("cfn-guard/test-command-test.yaml", &test_spec);
    assert_value_writer_replays("cfn-guard/s3-bucket-logging-enabled-tests.yaml", &s3_spec);
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
struct NaviColorWidth {
    color: String,
    width_percentage: u16,
    min_width: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
struct NaviStyle {
    tag: NaviColorWidth,
    comment: NaviColorWidth,
    snippet: NaviColorWidth,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
struct NaviFinder {
    command: String,
    overrides: Option<String>,
    overrides_var: Option<String>,
    delimiter_var: Option<String>,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(default)]
struct NaviCheats {
    path: Option<String>,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(default)]
struct NaviSearch {
    tags: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
struct NaviShell {
    command: String,
    finder_command: Option<String>,
    forward_slash_path: bool,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(default)]
struct NaviClient {
    tealdeer: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
struct NaviConfig {
    style: NaviStyle,
    finder: NaviFinder,
    cheats: NaviCheats,
    search: NaviSearch,
    shell: NaviShell,
    client: NaviClient,
    source: String,
}

#[test]
fn external_navi_config_files_match_serde_yaml_and_reader_paths() {
    for path in ["navi/config-example.yaml", "navi/tests-config.yaml"] {
        let input = read_fixture(path);
        let parsed: NaviConfig = assert_yaml_matches_serde(&input);
        assert_eq!(parsed.finder.command, "fzf", "{path}");
        assert_eq!(parsed.style.tag.color, "cyan", "{path}");
        assert_eq!(parsed.style.snippet.color, "white", "{path}");

        let reader_parsed: NaviConfig = assert_yaml_reader_matches_serde(&input);
        assert_eq!(reader_parsed, parsed, "{path}");
    }

    let example: NaviConfig =
        assert_yaml_matches_serde(include_str!("fixtures/downstream/navi/config-example.yaml"));
    assert_eq!(example.shell.command, "bash");
    assert_eq!(example.style.comment.width_percentage, 42);
    assert!(!example.client.tealdeer);

    let test_config: NaviConfig = assert_yaml_reader_matches_serde(include_str!(
        "fixtures/downstream/navi/tests-config.yaml"
    ));
    assert_eq!(test_config.shell.finder_command.as_deref(), Some("bash"));
    assert_eq!(test_config.style.comment.color, "yellow");
    assert!(
        test_config
            .shell
            .command
            .contains("BASH_ENV=\"${NAVI_HOME}/tests/helpers.sh\"")
    );
}

#[test]
fn external_stackable_operator_crds_match_serde_yaml() {
    for path in STACKABLE_OPERATOR_CRDS {
        let value = assert_value_matches_serde(&read_fixture(path));
        assert_stackable_crd_header(&value, path);

        let schema = &value["spec"]["versions"][0]["schema"]["openAPIV3Schema"];
        assert_eq!(schema["type"].as_str(), Some("object"), "{path}");
        assert!(
            schema["properties"]["spec"].as_mapping().is_some(),
            "{path} must expose a spec OpenAPI object"
        );
        assert_value_writer_replays(path, &value);
    }
}

#[test]
fn external_stackable_operator_crds_cover_openapi_extensions() {
    let listener_class = assert_value_matches_serde(include_str!(
        "fixtures/downstream/stackable-operator/ListenerClass.yaml"
    ));
    assert_eq!(
        listener_class["spec"]["group"].as_str(),
        Some("listeners.stackable.tech")
    );
    assert_eq!(
        listener_class["spec"]["names"]["kind"].as_str(),
        Some("ListenerClass")
    );
    assert_eq!(
        listener_class["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"]
            ["properties"]["serviceOverrides"]["x-kubernetes-preserve-unknown-fields"]
            .as_bool(),
        Some(true)
    );

    let authentication_class = assert_value_matches_serde(include_str!(
        "fixtures/downstream/stackable-operator/AuthenticationClass.yaml"
    ));
    assert_eq!(
        authentication_class["spec"]["group"].as_str(),
        Some("authentication.stackable.tech")
    );
    assert!(
        authentication_class["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]
            ["spec"]["properties"]["provider"]["oneOf"]
            .as_sequence()
            .is_some_and(|variants| variants.len() >= 5),
        "AuthenticationClass provider oneOf variants survive migration"
    );
}

fn downstream_manifest() -> DownstreamManifest {
    toml::from_str(SOURCE).expect("downstream SOURCE.toml parses")
}

fn read_fixture(path: &str) -> String {
    fs::read_to_string(Path::new(FIXTURE_ROOT).join(path))
        .unwrap_or_else(|error| panic!("read downstream fixture {path}: {error}"))
}

fn assert_yaml_matches_serde<T>(input: &str) -> T
where
    T: DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let parsed: T = yaml::from_str(input).expect("yaml downstream parse");
    let reference: T = serde_yaml::from_str(input).expect("serde_yaml downstream parse");
    assert_eq!(parsed, reference);
    parsed
}

fn assert_yaml_reader_matches_serde<T>(input: &str) -> T
where
    T: DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let parsed: T =
        yaml::from_reader(Cursor::new(input.as_bytes())).expect("yaml downstream reader parse");
    let reference: T = serde_yaml::from_reader(Cursor::new(input.as_bytes()))
        .expect("serde_yaml downstream reader parse");
    assert_eq!(parsed, reference);
    parsed
}

fn assert_value_matches_serde(input: &str) -> Value {
    let parsed: Value = yaml::from_str(input).expect("yaml downstream value parse");
    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml downstream value parse");
    let reference = yaml::to_value(reference).expect("serde_yaml value converts to yaml::Value");
    assert!(parsed.equivalent(&reference));
    parsed
}

fn assert_value_writer_replays(path: &str, value: &Value) {
    let emitted = yaml::to_string(value)
        .unwrap_or_else(|error| panic!("yaml writes downstream value {path}: {error}"));
    let reparsed: Value = yaml::from_str(&emitted)
        .unwrap_or_else(|error| panic!("yaml reparses emitted downstream value {path}: {error}"));
    assert!(
        reparsed.equivalent(value),
        "emitted downstream value must reparse equivalently for {path}"
    );

    let mut written = Vec::new();
    yaml::to_writer(&mut written, value)
        .unwrap_or_else(|error| panic!("yaml writes downstream value to writer {path}: {error}"));
    assert_eq!(written, emitted.as_bytes(), "{path}");

    let byte_compatible = yaml::to_string_with_options(value, yaml::EmitOptions::byte_compatible())
        .unwrap_or_else(|error| {
            panic!("yaml writes downstream value in byte-compatible mode {path}: {error}")
        });
    let byte_reparsed: Value = yaml::from_str(&byte_compatible).unwrap_or_else(|error| {
        panic!("yaml reparses byte-compatible downstream value {path}: {error}")
    });
    assert!(
        byte_reparsed.equivalent(value),
        "byte-compatible downstream value must reparse equivalently for {path}"
    );

    let mut byte_written = Vec::new();
    yaml::to_writer_with_options(
        &mut byte_written,
        value,
        yaml::EmitOptions::byte_compatible(),
    )
    .unwrap_or_else(|error| {
        panic!("yaml writes byte-compatible downstream value to writer {path}: {error}")
    });
    assert_eq!(byte_written, byte_compatible.as_bytes(), "{path}");

    let mut stream = yaml::Serializer::new(Vec::new());
    value
        .serialize(&mut stream)
        .unwrap_or_else(|error| panic!("yaml streams first downstream value {path}: {error}"));
    value
        .serialize(&mut stream)
        .unwrap_or_else(|error| panic!("yaml streams second downstream value {path}: {error}"));
    let stream_output = String::from_utf8(stream.into_inner().expect("stream into inner"))
        .expect("serializer output is UTF-8");
    let docs = yaml::Deserializer::from_str(&stream_output)
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("yaml reparses streamed downstream value {path}: {error}"));
    assert_eq!(docs.len(), 2, "{path}");
    assert!(docs[0].equivalent(value), "{path}");
    assert!(docs[1].equivalent(value), "{path}");

    let mut byte_stream =
        yaml::Serializer::with_options(Vec::new(), yaml::EmitOptions::byte_compatible());
    value.serialize(&mut byte_stream).unwrap_or_else(|error| {
        panic!("yaml streams first byte-compatible downstream value {path}: {error}")
    });
    value.serialize(&mut byte_stream).unwrap_or_else(|error| {
        panic!("yaml streams second byte-compatible downstream value {path}: {error}")
    });
    let byte_stream_output =
        String::from_utf8(byte_stream.into_inner().expect("byte stream into inner"))
            .expect("byte-compatible serializer output is UTF-8");
    let byte_docs = yaml::Deserializer::from_str(&byte_stream_output)
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            panic!("yaml reparses byte-compatible streamed downstream value {path}: {error}")
        });
    assert_eq!(byte_docs.len(), 2, "{path}");
    assert!(byte_docs[0].equivalent(value), "{path}");
    assert!(byte_docs[1].equivalent(value), "{path}");
}

fn assert_typed_byte_writer_matches_serde<T>(path: &str, value: &T)
where
    T: Serialize,
{
    let byte_compatible = yaml::to_string_with_options(value, yaml::EmitOptions::byte_compatible())
        .unwrap_or_else(|error| {
            panic!("yaml writes typed value in byte-compatible mode {path}: {error}")
        });
    let reference = serde_yaml::to_string(value)
        .unwrap_or_else(|error| panic!("serde_yaml writes typed value {path}: {error}"));
    assert_eq!(byte_compatible, reference, "{path}");

    let mut byte_written = Vec::new();
    yaml::to_writer_with_options(
        &mut byte_written,
        value,
        yaml::EmitOptions::byte_compatible(),
    )
    .unwrap_or_else(|error| {
        panic!("yaml writes typed byte-compatible value to writer {path}: {error}")
    });
    assert_eq!(byte_written, reference.as_bytes(), "{path}");

    let mut byte_stream =
        yaml::Serializer::with_options(Vec::new(), yaml::EmitOptions::byte_compatible());
    value
        .serialize(&mut byte_stream)
        .unwrap_or_else(|error| panic!("yaml streams typed byte-compatible value {path}: {error}"));
    let byte_stream_output = String::from_utf8(
        byte_stream
            .into_inner()
            .expect("typed byte stream into inner"),
    )
    .expect("typed byte-compatible stream is UTF-8");
    assert_eq!(byte_stream_output, reference, "{path}");
}

impl Default for NaviColorWidth {
    fn default() -> Self {
        Self {
            color: "blue".to_owned(),
            width_percentage: 26,
            min_width: 20,
        }
    }
}

impl Default for NaviStyle {
    fn default() -> Self {
        Self {
            tag: NaviColorWidth {
                color: "cyan".to_owned(),
                width_percentage: 26,
                min_width: 20,
            },
            comment: NaviColorWidth {
                color: "blue".to_owned(),
                width_percentage: 42,
                min_width: 45,
            },
            snippet: NaviColorWidth::default(),
        }
    }
}

impl Default for NaviFinder {
    fn default() -> Self {
        Self {
            command: "fzf".to_owned(),
            overrides: None,
            overrides_var: None,
            delimiter_var: None,
        }
    }
}

impl Default for NaviShell {
    fn default() -> Self {
        Self {
            command: "bash".to_owned(),
            finder_command: None,
            forward_slash_path: false,
        }
    }
}

impl Default for NaviConfig {
    fn default() -> Self {
        Self {
            style: NaviStyle::default(),
            finder: NaviFinder::default(),
            cheats: NaviCheats::default(),
            search: NaviSearch::default(),
            shell: NaviShell::default(),
            client: NaviClient::default(),
            source: "BUILT-IN".to_owned(),
        }
    }
}

fn assert_stackable_crd_header(value: &Value, path: &str) {
    assert_eq!(
        value["apiVersion"].as_str(),
        Some("apiextensions.k8s.io/v1"),
        "{path}"
    );
    assert_eq!(
        value["kind"].as_str(),
        Some("CustomResourceDefinition"),
        "{path}"
    );
    assert_eq!(value["spec"]["versions"][0]["served"].as_bool(), Some(true));
    assert_eq!(
        value["spec"]["versions"][0]["storage"].as_bool(),
        Some(true)
    );
}

fn assert_tagged_scalar<'a>(
    value: &'a Value,
    expected_tag: &str,
    expected_scalar: &str,
    context: &str,
) -> &'a yaml::TaggedValue {
    let tagged = value
        .as_tagged()
        .unwrap_or_else(|| panic!("{context} must be tagged"));
    assert_eq!(tagged.tag, Tag::new(expected_tag), "{context}");
    if !expected_scalar.is_empty() {
        assert_eq!(tagged.value.as_str(), Some(expected_scalar), "{context}");
    }
    tagged
}

fn lookup_locale_path<'a>(
    root: &'a BTreeMap<String, LocaleValue>,
    path: &[&str],
) -> Option<&'a str> {
    let mut current = root.get(path.first().copied()?)?;
    for segment in &path[1..] {
        current = match current {
            LocaleValue::Mapping(map) => map.get(*segment)?,
            _ => return None,
        };
    }
    match current {
        LocaleValue::String(value) => Some(value),
        _ => None,
    }
}
