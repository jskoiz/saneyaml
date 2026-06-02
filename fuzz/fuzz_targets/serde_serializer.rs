#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::Serialize;
use std::collections::BTreeMap;
use saneyaml::{BlockScalarStyle, EmitCollectionStyle, EmitOptions, ScalarQuoteStyle, Value};

fuzz_target!(|input: &[u8]| {
    let Some((&selector, payload)) = input.split_first() else {
        return;
    };

    match selector {
        b's' => assert_writer_roundtrip(&service_config(payload)),
        b'k' => assert_writer_roundtrip(&kubernetes_deployment(payload)),
        b'o' => assert_writer_roundtrip(&openapi_document(payload)),
        b'e' => assert_writer_roundtrip(&enum_config(payload)),
        b'm' => assert_streaming_pair(&service_config(payload), &kubernetes_deployment(payload)),
        b'b' => assert_bytes_rejection(payload),
        b'n' => assert_writer_roundtrip(&nested_options(payload)),
        _ => match selector % 7 {
            0 => assert_writer_roundtrip(&service_config(payload)),
            1 => assert_writer_roundtrip(&kubernetes_deployment(payload)),
            2 => assert_writer_roundtrip(&openapi_document(payload)),
            3 => assert_writer_roundtrip(&enum_config(payload)),
            4 => assert_streaming_pair(&openapi_document(payload), &nested_options(payload)),
            5 => assert_bytes_rejection(payload),
            _ => assert_writer_roundtrip(&nested_options(payload)),
        },
    }
});

#[derive(Debug, Serialize)]
struct ServiceConfig {
    name: String,
    image: String,
    replicas: u16,
    ports: Vec<u16>,
    enabled: bool,
    env: BTreeMap<String, String>,
    action: Action,
}

#[derive(Debug, Serialize)]
struct KubernetesDeployment {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: DeploymentSpec,
}

#[derive(Clone, Debug, Serialize)]
struct Metadata {
    name: String,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct DeploymentSpec {
    replicas: u16,
    selector: LabelSelector,
    template: PodTemplate,
}

#[derive(Debug, Serialize)]
struct LabelSelector {
    #[serde(rename = "matchLabels")]
    match_labels: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct PodTemplate {
    metadata: Metadata,
    spec: PodSpec,
}

#[derive(Debug, Serialize)]
struct PodSpec {
    containers: Vec<Container>,
}

#[derive(Debug, Serialize)]
struct Container {
    name: String,
    image: String,
    ports: Vec<ContainerPort>,
    env: Vec<EnvVar>,
}

#[derive(Debug, Serialize)]
struct ContainerPort {
    #[serde(rename = "containerPort")]
    container_port: u16,
}

#[derive(Debug, Serialize)]
struct EnvVar {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct OpenApiDocument {
    openapi: String,
    info: Info,
    paths: BTreeMap<String, PathItem>,
    components: Components,
}

#[derive(Debug, Serialize)]
struct Info {
    title: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct PathItem {
    get: Operation,
}

#[derive(Debug, Serialize)]
struct Operation {
    #[serde(rename = "operationId")]
    operation_id: String,
    responses: BTreeMap<String, Response>,
}

#[derive(Debug, Serialize)]
struct Response {
    description: String,
}

#[derive(Debug, Serialize)]
struct Components {
    schemas: BTreeMap<String, Schema>,
}

#[derive(Debug, Serialize)]
struct Schema {
    #[serde(rename = "type")]
    schema_type: String,
    properties: BTreeMap<String, SchemaProperty>,
}

#[derive(Debug, Serialize)]
struct SchemaProperty {
    #[serde(rename = "type")]
    property_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Debug, Serialize)]
enum Action {
    Unit,
    Newtype(String),
    Tuple(String, u16),
    Struct { run: String, shell: String },
}

#[derive(Debug, Serialize)]
struct EnumConfig {
    primary: Action,
    fallback: Option<Action>,
    actions: Vec<Action>,
}

#[derive(Debug, Serialize)]
struct NestedOptions {
    name: Option<String>,
    matrix: BTreeMap<String, Vec<Option<String>>>,
    flags: Vec<bool>,
    limits: BTreeMap<String, Option<u16>>,
}

#[derive(Clone, Copy)]
struct BytesPayload<'a>(&'a [u8]);

impl Serialize for BytesPayload<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.0)
    }
}

fn assert_writer_roundtrip<T>(value: &T)
where
    T: Serialize,
{
    let expected = saneyaml::to_value(value).expect("value serializer accepts shape");
    let emitted = saneyaml::to_string(value).expect("document writer accepts shape");

    let mut written = Vec::new();
    saneyaml::to_writer(&mut written, value).expect("writer accepts shape");
    assert_eq!(written, emitted.as_bytes());

    let reparsed: Value = saneyaml::from_str(&emitted).expect("document writer output reparses");
    assert!(
        reparsed.equivalent(&expected),
        "document writer output changed value shape: {emitted}"
    );
    for options in emit_option_roundtrip_matrix() {
        assert_optioned_writer_roundtrip(value, &expected, options);
    }

    let reference = serde_yaml::to_string(value).expect("serde_yaml accepts shape");
    let byte_emitted =
        saneyaml::to_string_with_options(value, EmitOptions::byte_compatible())
            .expect("byte-compatible document writer accepts shape");
    assert_eq!(byte_emitted, reference);

    let mut byte_written = Vec::new();
    saneyaml::to_writer_with_options(&mut byte_written, value, EmitOptions::byte_compatible())
        .expect("byte-compatible writer accepts shape");
    assert_eq!(byte_written, reference.as_bytes());

    let reference_value: serde_yaml::Value =
        serde_yaml::from_str(&reference).expect("serde_yaml output reparses");
    let emitted_reference_value: serde_yaml::Value =
        serde_yaml::from_str(&emitted).expect("yaml output reparses with serde_yaml");
    assert_eq!(emitted_reference_value, reference_value);

    assert_streaming_pair(value, value);
}

fn emit_option_roundtrip_matrix() -> [EmitOptions; 4] {
    [
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::SingleQuoted),
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::DoubleQuoted),
        EmitOptions::structural().with_block_scalar_style(BlockScalarStyle::Folded),
        EmitOptions::structural().with_collection_style(EmitCollectionStyle::Flow),
    ]
}

fn assert_optioned_writer_roundtrip<T>(value: &T, expected: &Value, options: EmitOptions)
where
    T: Serialize,
{
    let emitted =
        saneyaml::to_string_with_options(value, options).expect("optioned document writer accepts shape");
    let mut written = Vec::new();
    saneyaml::to_writer_with_options(&mut written, value, options)
        .expect("optioned writer accepts shape");
    assert_eq!(written, emitted.as_bytes());
    let reparsed: Value =
        saneyaml::from_str(&emitted).expect("optioned document writer output reparses");
    assert!(
        reparsed.equivalent(expected),
        "optioned document writer output changed value shape: {emitted}"
    );
}

fn assert_streaming_pair<A, B>(first: &A, second: &B)
where
    A: Serialize,
    B: Serialize,
{
    let expected_first = saneyaml::to_value(first).expect("first value serializes");
    let expected_second = saneyaml::to_value(second).expect("second value serializes");

    let mut stream = saneyaml::Serializer::new(Vec::new());
    first.serialize(&mut stream).expect("stream first document");
    second.serialize(&mut stream).expect("stream second document");
    let output =
        String::from_utf8(stream.into_inner().expect("stream into inner")).expect("utf8 stream");
    let docs: Vec<Value> = saneyaml::from_documents_str(&output).expect("stream output reparses");
    assert_eq!(docs.len(), 2);
    assert!(docs[0].equivalent(&expected_first));
    assert!(docs[1].equivalent(&expected_second));

    let mut byte_stream = saneyaml::Serializer::with_options(Vec::new(), EmitOptions::byte_compatible());
    first
        .serialize(&mut byte_stream)
        .expect("byte-compatible stream first document");
    second
        .serialize(&mut byte_stream)
        .expect("byte-compatible stream second document");
    let byte_output =
        String::from_utf8(byte_stream.into_inner().expect("byte stream into inner"))
            .expect("byte stream output is utf8");

    let mut reference = Vec::new();
    {
        let mut serializer = serde_yaml::Serializer::new(&mut reference);
        first
            .serialize(&mut serializer)
            .expect("serde_yaml stream first document");
        second
            .serialize(&mut serializer)
            .expect("serde_yaml stream second document");
    }
    assert_eq!(byte_output.as_bytes(), reference.as_slice());
}

fn assert_bytes_rejection(input: &[u8]) {
    let payload = BytesPayload(&input[..input.len().min(128)]);
    let reference = serde_yaml::to_string(&payload).expect_err("serde_yaml rejects bytes");

    let error = saneyaml::to_string(&payload).expect_err("document writer rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());
    let byte_error = saneyaml::to_string_with_options(&payload, EmitOptions::byte_compatible())
        .expect_err("byte-compatible document writer rejects bytes");
    assert_eq!(byte_error.to_string(), reference.to_string());

    let mut written = Vec::new();
    let error = saneyaml::to_writer(&mut written, &payload).expect_err("writer rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());
    assert!(written.is_empty());
    let byte_error =
        saneyaml::to_writer_with_options(&mut written, &payload, EmitOptions::byte_compatible())
            .expect_err("byte-compatible writer rejects bytes");
    assert_eq!(byte_error.to_string(), reference.to_string());
    assert!(written.is_empty());

    let mut stream = saneyaml::Serializer::new(Vec::new());
    let error = payload
        .serialize(&mut stream)
        .expect_err("streaming writer rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());
    assert!(stream.into_inner().expect("stream into inner").is_empty());
    let mut byte_stream = saneyaml::Serializer::with_options(Vec::new(), EmitOptions::byte_compatible());
    let error = payload
        .serialize(&mut byte_stream)
        .expect_err("byte-compatible streaming writer rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());
    assert!(
        byte_stream
            .into_inner()
            .expect("byte stream into inner")
            .is_empty()
    );

    let nested = BTreeMap::from([("payload", payload)]);
    let nested_reference = serde_yaml::to_string(&nested).expect_err("serde_yaml rejects nested bytes");
    let nested_error = saneyaml::to_string(&nested).expect_err("document writer rejects nested bytes");
    assert_eq!(nested_error.to_string(), nested_reference.to_string());
    let nested_byte_error = saneyaml::to_string_with_options(&nested, EmitOptions::byte_compatible())
        .expect_err("byte-compatible document writer rejects nested bytes");
    assert_eq!(nested_byte_error.to_string(), nested_reference.to_string());
}

fn service_config(input: &[u8]) -> ServiceConfig {
    ServiceConfig {
        name: token(input, 0, "api"),
        image: format!("example/{}:{}", token(input, 8, "app"), number(input, 1, 99)),
        replicas: number(input, 2, 8).max(1),
        ports: vec![80, 443, 8000 + number(input, 3, 999)],
        enabled: flag(input, 4),
        env: map(input, "ENV"),
        action: action(input, 5),
    }
}

fn kubernetes_deployment(input: &[u8]) -> KubernetesDeployment {
    let labels = map(input, "app");
    let metadata = Metadata {
        name: token(input, 0, "workload"),
        labels: labels.clone(),
        annotations: map(input, "annotation"),
    };
    KubernetesDeployment {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
        metadata: metadata.clone(),
        spec: DeploymentSpec {
            replicas: number(input, 1, 6).max(1),
            selector: LabelSelector {
                match_labels: labels.clone(),
            },
            template: PodTemplate {
                metadata,
                spec: PodSpec {
                    containers: vec![Container {
                        name: token(input, 2, "server"),
                        image: format!("example/{}:{}", token(input, 3, "server"), number(input, 4, 99)),
                        ports: vec![ContainerPort {
                            container_port: 8000 + number(input, 5, 999),
                        }],
                        env: vec![EnvVar {
                            name: "RUST_LOG".to_string(),
                            value: token(input, 6, "info"),
                        }],
                    }],
                },
            },
        },
    }
}

fn openapi_document(input: &[u8]) -> OpenApiDocument {
    let mut responses = BTreeMap::new();
    responses.insert(
        "200".to_string(),
        Response {
            description: token(input, 0, "ok"),
        },
    );

    let mut paths = BTreeMap::new();
    paths.insert(
        format!("/{}", token(input, 1, "items")),
        PathItem {
            get: Operation {
                operation_id: token(input, 2, "listItems"),
                responses,
            },
        },
    );

    let mut properties = BTreeMap::new();
    properties.insert(
        "id".to_string(),
        SchemaProperty {
            property_type: "string".to_string(),
            format: Some("uuid".to_string()),
        },
    );
    properties.insert(
        token(input, 3, "name"),
        SchemaProperty {
            property_type: "string".to_string(),
            format: None,
        },
    );

    let mut schemas = BTreeMap::new();
    schemas.insert(
        token(input, 4, "Item"),
        Schema {
            schema_type: "object".to_string(),
            properties,
        },
    );

    OpenApiDocument {
        openapi: "3.1.0".to_string(),
        info: Info {
            title: token(input, 5, "Service"),
            version: format!("0.{}.0", number(input, 6, 99)),
        },
        paths,
        components: Components { schemas },
    }
}

fn enum_config(input: &[u8]) -> EnumConfig {
    EnumConfig {
        primary: action(input, 0),
        fallback: flag(input, 1).then(|| action(input, 2)),
        actions: vec![Action::Unit, action(input, 3), action(input, 4)],
    }
}

fn nested_options(input: &[u8]) -> NestedOptions {
    let mut matrix = BTreeMap::new();
    matrix.insert(
        token(input, 0, "os"),
        vec![Some(token(input, 1, "macos")), None, Some(token(input, 2, "linux"))],
    );
    matrix.insert(
        token(input, 3, "rust"),
        vec![Some(format!("1.{}", number(input, 4, 99)))],
    );

    let mut limits = BTreeMap::new();
    limits.insert("cpu".to_string(), Some(number(input, 5, 64)));
    limits.insert("memory".to_string(), flag(input, 6).then(|| number(input, 7, 128)));

    NestedOptions {
        name: flag(input, 8).then(|| token(input, 9, "matrix")),
        matrix,
        flags: vec![flag(input, 10), flag(input, 11), flag(input, 12)],
        limits,
    }
}

fn action(input: &[u8], offset: usize) -> Action {
    match input.get(offset).copied().unwrap_or(0) % 4 {
        0 => Action::Unit,
        1 => Action::Newtype(token(input, offset + 1, "deploy")),
        2 => Action::Tuple(token(input, offset + 2, "build"), number(input, offset + 3, 99)),
        _ => Action::Struct {
            run: format!("cargo {}", token(input, offset + 4, "test")),
            shell: token(input, offset + 5, "bash"),
        },
    }
}

fn map(input: &[u8], prefix: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    map.insert(format!("{prefix}_ONE"), token(input, 0, "one"));
    map.insert(format!("{prefix}_TWO"), token(input, 8, "two"));
    if input.len() > 16 {
        map.insert(format!("{prefix}_EXTRA"), token(input, 16, "extra"));
    }
    map
}

fn token(input: &[u8], offset: usize, fallback: &str) -> String {
    let bytes = input.iter().skip(offset).take(24).copied();
    let mut out = String::new();
    for byte in bytes {
        let ch = match byte % 38 {
            0..=9 => char::from(b'0' + (byte % 10)),
            10..=35 => char::from(b'a' + ((byte - 10) % 26)),
            36 => '-',
            _ => '_',
        };
        out.push(ch);
    }
    if out.is_empty() || out == "-" || out == "_" {
        fallback.to_string()
    } else {
        out
    }
}

fn number(input: &[u8], offset: usize, max: u16) -> u16 {
    let mut value = 0u16;
    for byte in input.iter().skip(offset).take(2) {
        value = value.wrapping_mul(257).wrapping_add(u16::from(*byte));
    }
    value % (max + 1)
}

fn flag(input: &[u8], offset: usize) -> bool {
    input.get(offset).copied().unwrap_or(0) % 2 == 0
}
