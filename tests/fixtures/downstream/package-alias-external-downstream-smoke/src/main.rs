#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

const STACKABLE_OPERATOR_CRDS: &[(&str, &str)] = &[
    (
        "fixtures/stackable-operator/AuthenticationClass.yaml",
        include_str!("../fixtures/stackable-operator/AuthenticationClass.yaml"),
    ),
    (
        "fixtures/stackable-operator/DummyCluster.yaml",
        include_str!("../fixtures/stackable-operator/DummyCluster.yaml"),
    ),
    (
        "fixtures/stackable-operator/Listener.yaml",
        include_str!("../fixtures/stackable-operator/Listener.yaml"),
    ),
    (
        "fixtures/stackable-operator/ListenerClass.yaml",
        include_str!("../fixtures/stackable-operator/ListenerClass.yaml"),
    ),
    (
        "fixtures/stackable-operator/PodListeners.yaml",
        include_str!("../fixtures/stackable-operator/PodListeners.yaml"),
    ),
    (
        "fixtures/stackable-operator/S3Bucket.yaml",
        include_str!("../fixtures/stackable-operator/S3Bucket.yaml"),
    ),
    (
        "fixtures/stackable-operator/S3Connection.yaml",
        include_str!("../fixtures/stackable-operator/S3Connection.yaml"),
    ),
    (
        "fixtures/stackable-operator/Scaler.yaml",
        include_str!("../fixtures/stackable-operator/Scaler.yaml"),
    ),
];

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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum LocaleValue {
    String(String),
    Integer(i64),
    Mapping(BTreeMap<String, LocaleValue>),
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

fn main() {
    pingora_configs_use_package_alias();
    rust_i18n_locales_use_package_alias();
    cfn_guard_templates_use_package_alias();
    navi_configs_use_package_alias();
    stackable_crds_use_package_alias();
    mapping_api_denominator_uses_package_alias();
}

fn pingora_configs_use_package_alias() {
    for (path, input) in [
        (
            "fixtures/pingora/pingora-core-pingora_conf.yaml",
            include_str!("../fixtures/pingora/pingora-core-pingora_conf.yaml"),
        ),
        (
            "fixtures/pingora/pingora-proxy-pingora_conf.yaml",
            include_str!("../fixtures/pingora/pingora-proxy-pingora_conf.yaml"),
        ),
        (
            "fixtures/pingora/pingora-proxy-example-conf.yaml",
            include_str!("../fixtures/pingora/pingora-proxy-example-conf.yaml"),
        ),
    ] {
        let parsed: PingoraServerConf = serde_yaml::from_str(input).expect("pingora parses");
        assert_eq!(parsed.version, 1, "{path}");

        let emitted = serde_yaml::to_string(&parsed).expect("pingora emits");
        let reparsed: PingoraServerConf =
            serde_yaml::from_str(&emitted).expect("pingora emitted YAML reparses");
        assert_eq!(reparsed, parsed, "{path}");
    }

    let core: PingoraServerConf = serde_yaml::from_str(include_str!(
        "../fixtures/pingora/pingora-core-pingora_conf.yaml"
    ))
    .expect("core config parses");
    assert_eq!(core.client_bind_to_ipv4, [Ipv4Addr::new(127, 0, 0, 2)]);
    assert_eq!(
        core.ca_file.as_deref(),
        Some(Path::new("tests/keys/server.crt"))
    );

    let example: PingoraServerConf = serde_yaml::from_str(include_str!(
        "../fixtures/pingora/pingora-proxy-example-conf.yaml"
    ))
    .expect("example config parses");
    assert_eq!(example.threads, Some(2));
    assert_eq!(example.max_retries, Some(5));
    assert_eq!(
        example.pid_file.as_deref(),
        Some(Path::new("/tmp/load_balancer.pid"))
    );
}

fn rust_i18n_locales_use_package_alias() {
    for (path, input) in [
        (
            "fixtures/rust-i18n/app-en.yml",
            include_str!("../fixtures/rust-i18n/app-en.yml"),
        ),
        (
            "fixtures/rust-i18n/app-fr.yml",
            include_str!("../fixtures/rust-i18n/app-fr.yml"),
        ),
        (
            "fixtures/rust-i18n/user.en.yaml",
            include_str!("../fixtures/rust-i18n/user.en.yaml"),
        ),
        (
            "fixtures/rust-i18n/v2.yml",
            include_str!("../fixtures/rust-i18n/v2.yml"),
        ),
    ] {
        let locale: BTreeMap<String, LocaleValue> =
            serde_yaml::from_str(input).expect("rust-i18n locale parses");
        assert!(!locale.is_empty(), "{path}");
    }

    let app_en: BTreeMap<String, LocaleValue> =
        serde_yaml::from_str(include_str!("../fixtures/rust-i18n/app-en.yml"))
            .expect("app-en locale parses");
    assert_eq!(
        app_en.get("hello"),
        Some(&LocaleValue::String("Hello, %{name}!".to_owned()))
    );

    let user: BTreeMap<String, LocaleValue> =
        serde_yaml::from_str(include_str!("../fixtures/rust-i18n/user.en.yaml"))
            .expect("user locale parses");
    assert!(matches!(
        user.get("messages"),
        Some(LocaleValue::Mapping(messages)) if messages.contains_key("user")
    ));

    let v2: BTreeMap<String, LocaleValue> =
        serde_yaml::from_str(include_str!("../fixtures/rust-i18n/v2.yml"))
            .expect("v2 locale parses");
    let hello = lookup_locale_path(&v2, &["nested_locale_test", "hello", "ja"]);
    assert!(hello.is_some_and(|value| value.ends_with(" test2")));
    let message = lookup_locale_path(&v2, &["t_kmFrQ2nnJsvUh3Ckxmki0", "zh-CN"]);
    assert!(message.is_some_and(|value| value.contains("%{name}") && value.contains("%{msg}")));
}

fn cfn_guard_templates_use_package_alias() {
    let value: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../fixtures/cfn-guard/cfn-lambda.yaml"))
            .expect("cfn-lambda parses");

    let service_token = &value["Resources"]["AllSecurityGroups"]["Properties"]["ServiceToken"];
    assert_tagged_scalar(
        service_token,
        "GetAtt",
        "AppendItemToListFunction.Arn",
        "CloudFormation service token",
    );

    let zip_file = &value["Resources"]["AppendItemToListFunction"]["Properties"]["Code"]
        ["ZipFile"];
    let tagged = assert_tagged_scalar(zip_file, "Sub", "", "CloudFormation inline Lambda");
    assert!(
        tagged
            .value
            .as_str()
            .is_some_and(|source| source.contains("exports.handler = function(event, context)")),
        "inline Lambda body survives tagged block scalar"
    );

    let security_group_ids =
        &value["Resources"]["MyEC2Instance"]["Properties"]["SecurityGroupIds"];
    assert_tagged_scalar(
        security_group_ids,
        "GetAtt",
        "AllSecurityGroups.Value",
        "CloudFormation security group id lookup",
    );
    assert_value_writer_replays("fixtures/cfn-guard/cfn-lambda.yaml", &value);

    let test_spec: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../fixtures/cfn-guard/test-command-test.yaml"))
            .expect("cfn-guard test spec parses");
    assert_eq!(
        test_spec[0]["expectations"]["rules"]["REDSHIFT_ENCRYPTED_CMK"].as_str(),
        Some("PASS")
    );
    assert_tagged_scalar(
        &test_spec[0]["input"]["Resources"]["myCluster"]["Properties"]["KmsKeyId"]
            ["Fn::ImportValue"],
        "Sub",
        "${pSecretKmsKey}",
        "nested CloudFormation import value",
    );
    assert_value_writer_replays("fixtures/cfn-guard/test-command-test.yaml", &test_spec);

    let s3_spec: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../fixtures/cfn-guard/s3-bucket-logging-enabled-tests.yaml"
    ))
    .expect("s3 logging spec parses");
    assert_eq!(
        s3_spec[0]["expectations"]["rules"]["S3_BUCKET_LOGGING_ENABLED"].as_str(),
        Some("SKIP")
    );
    assert_eq!(
        s3_spec[2]["expectations"]["rules"]["S3_BUCKET_LOGGING_ENABLED"].as_str(),
        Some("PASS")
    );
    assert_tagged_scalar(
        &s3_spec[2]["input"]["Resources"]["ExampleS3"]["Properties"]["LoggingConfiguration"]
            ["DestinationBucketName"],
        "Ref",
        "LoggingBucket",
        "S3 logging destination bucket",
    );
    assert_value_writer_replays(
        "fixtures/cfn-guard/s3-bucket-logging-enabled-tests.yaml",
        &s3_spec,
    );
}

fn navi_configs_use_package_alias() {
    for (path, input) in [
        (
            "fixtures/navi/config-example.yaml",
            include_str!("../fixtures/navi/config-example.yaml"),
        ),
        (
            "fixtures/navi/tests-config.yaml",
            include_str!("../fixtures/navi/tests-config.yaml"),
        ),
    ] {
        let parsed: NaviConfig = serde_yaml::from_str(input).expect("navi config parses");
        let reader_parsed: NaviConfig =
            serde_yaml::from_reader(Cursor::new(input.as_bytes())).expect("navi reader parses");
        assert_eq!(reader_parsed, parsed, "{path}");
        assert_eq!(parsed.finder.command, "fzf", "{path}");
        assert_eq!(parsed.style.tag.color, "cyan", "{path}");
        assert_eq!(parsed.style.snippet.color, "white", "{path}");
    }

    let example: NaviConfig =
        serde_yaml::from_str(include_str!("../fixtures/navi/config-example.yaml"))
            .expect("navi example config parses");
    assert_eq!(example.shell.command, "bash");
    assert_eq!(example.style.comment.width_percentage, 42);
    assert!(!example.client.tealdeer);

    let test_config: NaviConfig =
        serde_yaml::from_reader(Cursor::new(include_str!("../fixtures/navi/tests-config.yaml")))
            .expect("navi test config parses through reader");
    assert_eq!(test_config.shell.finder_command.as_deref(), Some("bash"));
    assert_eq!(test_config.style.comment.color, "yellow");
    assert!(
        test_config
            .shell
            .command
            .contains("BASH_ENV=\"${NAVI_HOME}/tests/helpers.sh\"")
    );
}

fn stackable_crds_use_package_alias() {
    for (path, input) in STACKABLE_OPERATOR_CRDS {
        let value: serde_yaml::Value = serde_yaml::from_str(input).expect("stackable CRD parses");
        assert_stackable_crd_header(&value, path);

        let schema = &value["spec"]["versions"][0]["schema"]["openAPIV3Schema"];
        assert_eq!(schema["type"].as_str(), Some("object"), "{path}");
        assert!(
            schema["properties"]["spec"].as_mapping().is_some(),
            "{path} must expose a spec OpenAPI object"
        );
        assert_value_writer_replays(path, &value);
    }

    let listener_class: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../fixtures/stackable-operator/ListenerClass.yaml"
    ))
    .expect("ListenerClass parses");
    assert_eq!(
        listener_class["spec"]["group"].as_str(),
        Some("listeners.stackable.tech")
    );
    assert_eq!(
        listener_class["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"]
            ["properties"]["serviceOverrides"]["x-kubernetes-preserve-unknown-fields"]
            .as_bool(),
        Some(true)
    );

    let authentication_class: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../fixtures/stackable-operator/AuthenticationClass.yaml"
    ))
    .expect("AuthenticationClass parses");
    assert_eq!(
        authentication_class["spec"]["group"].as_str(),
        Some("authentication.stackable.tech")
    );
    assert!(
        authentication_class["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]
            ["spec"]["properties"]["provider"]["oneOf"]
            .as_sequence()
            .is_some_and(|variants| variants.len() >= 5),
        "AuthenticationClass provider oneOf variants survive package alias smoke"
    );
}

fn assert_stackable_crd_header(value: &serde_yaml::Value, path: &str) {
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

fn assert_value_writer_replays(path: &str, value: &serde_yaml::Value) {
    let emitted = serde_yaml::to_string(value)
        .unwrap_or_else(|error| panic!("package alias writes downstream value {path}: {error}"));
    let reparsed: serde_yaml::Value = serde_yaml::from_str(&emitted)
        .unwrap_or_else(|error| panic!("package alias reparses emitted value {path}: {error}"));
    assert!(
        reparsed.equivalent(value),
        "package alias emitted value must reparse equivalently for {path}"
    );

    let mut written = Vec::new();
    serde_yaml::to_writer(&mut written, value)
        .unwrap_or_else(|error| panic!("package alias writes value to writer {path}: {error}"));
    assert_eq!(written, emitted.as_bytes(), "{path}");

    let byte_emitted =
        serde_yaml::to_string_with_options(value, serde_yaml::EmitOptions::ByteCompatible)
            .unwrap_or_else(|error| {
                panic!("package alias writes byte-compatible value {path}: {error}")
            });
    let byte_reparsed: serde_yaml::Value = serde_yaml::from_str(&byte_emitted).unwrap_or_else(
        |error| panic!("package alias reparses byte-compatible emitted value {path}: {error}"),
    );
    assert!(
        byte_reparsed.equivalent(value),
        "package alias byte-compatible output must reparse equivalently for {path}"
    );

    let mut byte_written = Vec::new();
    serde_yaml::to_writer_with_options(
        &mut byte_written,
        value,
        serde_yaml::EmitOptions::ByteCompatible,
    )
    .unwrap_or_else(|error| {
        panic!("package alias writes byte-compatible value to writer {path}: {error}")
    });
    assert_eq!(byte_written, byte_emitted.as_bytes(), "{path}");

    let mut stream = serde_yaml::Serializer::new(Vec::new());
    value
        .serialize(&mut stream)
        .unwrap_or_else(|error| panic!("package alias streams first value {path}: {error}"));
    value
        .serialize(&mut stream)
        .unwrap_or_else(|error| panic!("package alias streams second value {path}: {error}"));
    let stream_output = String::from_utf8(stream.into_inner().expect("stream into inner"))
        .expect("serializer output is UTF-8");
    let docs = serde_yaml::Deserializer::from_str(&stream_output)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("package alias reparses stream {path}: {error}"));
    assert_eq!(docs.len(), 2, "{path}");
    assert!(docs[0].equivalent(value), "{path}");
    assert!(docs[1].equivalent(value), "{path}");

    let mut byte_stream =
        serde_yaml::Serializer::with_options(Vec::new(), serde_yaml::EmitOptions::ByteCompatible);
    value
        .serialize(&mut byte_stream)
        .unwrap_or_else(|error| panic!("package alias streams first byte-compatible value {path}: {error}"));
    value
        .serialize(&mut byte_stream)
        .unwrap_or_else(|error| panic!("package alias streams second byte-compatible value {path}: {error}"));
    let byte_stream_output =
        String::from_utf8(byte_stream.into_inner().expect("byte stream into inner"))
            .expect("byte-compatible serializer output is UTF-8");
    let byte_docs = serde_yaml::Deserializer::from_str(&byte_stream_output)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("package alias reparses byte stream {path}: {error}"));
    assert_eq!(byte_docs.len(), 2, "{path}");
    assert!(byte_docs[0].equivalent(value), "{path}");
    assert!(byte_docs[1].equivalent(value), "{path}");
}

fn assert_tagged_scalar<'a>(
    value: &'a serde_yaml::Value,
    expected_tag: &str,
    expected_scalar: &str,
    context: &str,
) -> &'a serde_yaml::TaggedValue {
    let tagged = value
        .as_tagged()
        .unwrap_or_else(|| panic!("{context} must be tagged"));
    assert_eq!(
        tagged.tag,
        serde_yaml::Tag::new(expected_tag),
        "{context}"
    );
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

fn mapping_api_denominator_uses_package_alias() {
    let mut mapping = package_alias_mapping_from_pairs();
    assert_eq!(
        mapping
            .remove("b")
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("beta".to_owned())
    );
    let removed = mapping.remove_entry("a").expect("remove a");
    assert_eq!(removed.0.as_str(), Some("a"));
    assert_eq!(removed.1.as_str(), Some("alpha"));
    assert_eq!(
        mapping
            .swap_remove(String::from("e"))
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("epsilon".to_owned())
    );
    let removed = mapping
        .swap_remove_entry(serde_yaml::Value::from("c"))
        .expect("swap remove c");
    assert_eq!(removed.0.as_str(), Some("c"));
    assert_eq!(removed.1.as_str(), Some("gamma"));
    assert!(mapping.remove("missing").is_none());
    assert_eq!(
        package_alias_mapping_pairs(&mapping),
        package_alias_expected_pairs(&[("d", "delta")])
    );

    let mut retained = package_alias_mapping_from_pairs();
    for (_, value) in retained.iter_mut() {
        if value.as_str() == Some("beta") {
            *value = serde_yaml::Value::from("BETA");
        }
    }
    for value in retained.values_mut() {
        if value.as_str() == Some("delta") {
            *value = serde_yaml::Value::from("DELTA");
        }
    }
    retained.retain(|key, value| {
        if key.as_str() == Some("c") {
            *value = serde_yaml::Value::from("GAMMA");
        }
        key.as_str() != Some("a")
    });
    assert_eq!(
        package_alias_mapping_pairs(&retained),
        package_alias_expected_pairs(&[
            ("b", "BETA"),
            ("c", "GAMMA"),
            ("d", "DELTA"),
            ("e", "epsilon"),
        ])
    );

    let mut ordered = package_alias_mapping_from_pairs();
    ordered.shift_remove("b").expect("shift remove b");
    ordered
        .shift_remove_entry(serde_yaml::Value::from("d"))
        .expect("shift remove d");
    assert_eq!(
        package_alias_mapping_pairs(&ordered),
        package_alias_expected_pairs(&[("a", "alpha"), ("c", "gamma"), ("e", "epsilon")])
    );

    let keys = package_alias_mapping_from_pairs()
        .into_keys()
        .map(|key| key.as_str().expect("string key").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "b", "c", "d", "e"]);
    let values = package_alias_mapping_from_pairs()
        .into_values()
        .map(|value| value.as_str().expect("string value").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(values, ["alpha", "beta", "gamma", "delta", "epsilon"]);

    retained.clear();
    assert!(retained.is_empty());

    let mut value = serde_yaml::Value::Null;
    value["services"]["api"]["image"] = serde_yaml::Value::from("nginx");
    value["services"]["api"]["ports"] = serde_yaml::from_str("[80, 443]").unwrap();
    value["services"]["api"]["ports"][0] = serde_yaml::Value::from(8080_u64);
    value["services"]["api"][0] = serde_yaml::Value::from("numeric-key");
    assert_eq!(value["services"]["api"]["image"].as_str(), Some("nginx"));
    assert_eq!(value["services"]["api"]["ports"][0].as_u64(), Some(8080));
    assert_eq!(value["services"]["api"][0].as_str(), Some("numeric-key"));
    assert!(value["services"]["api"]["missing"].is_null());
}

fn package_alias_mapping_from_pairs() -> serde_yaml::Mapping {
    [
        ("a", "alpha"),
        ("b", "beta"),
        ("c", "gamma"),
        ("d", "delta"),
        ("e", "epsilon"),
    ]
    .into_iter()
    .map(|(key, value)| (serde_yaml::Value::from(key), serde_yaml::Value::from(value)))
    .collect()
}

fn package_alias_mapping_pairs(mapping: &serde_yaml::Mapping) -> Vec<(String, String)> {
    mapping
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().expect("string key").to_owned(),
                value.as_str().expect("string value").to_owned(),
            )
        })
        .collect()
}

fn package_alias_expected_pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
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
