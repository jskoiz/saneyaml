#![allow(dead_code)]

use serde::{Deserialize, de::DeserializeOwned};
use std::collections::BTreeMap;

fn assert_yaml_matches_serde<T>(input: &str) -> T
where
    T: DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let parsed: T = yaml::from_str(input).expect("yaml downstream parse");
    let reference: T = serde_yaml::from_str(input).expect("serde_yaml downstream parse");
    assert_eq!(parsed, reference);
    parsed
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatrixWorkflow {
    name: String,
    env: BTreeMap<String, String>,
    permissions: BTreeMap<String, String>,
    jobs: BTreeMap<String, MatrixJob>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatrixJob {
    name: String,
    #[serde(rename = "runs-on")]
    runs_on: String,
    strategy: MatrixStrategy,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatrixStrategy {
    #[serde(rename = "fail-fast")]
    fail_fast: bool,
    matrix: MatrixAxes,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatrixAxes {
    os: Vec<String>,
    #[serde(rename = "node-version")]
    node_version: Vec<u64>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct WorkflowStep {
    name: Option<String>,
    uses: Option<String>,
    run: Option<String>,
}

#[test]
fn downstream_github_actions_matrix_config_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/github-actions/matrix-ci.yaml");
    let workflow: MatrixWorkflow = assert_yaml_matches_serde(input);

    assert_eq!(workflow.name.as_str(), "Matrix CI");
    assert_eq!(workflow.env["CARGO_TERM_COLOR"].as_str(), "always");
    assert_eq!(workflow.permissions["contents"].as_str(), "read");
    let test = &workflow.jobs["test"];
    assert_eq!(
        test.strategy.matrix.os,
        vec!["ubuntu-latest".to_owned(), "macos-latest".to_owned()]
    );
    assert_eq!(test.strategy.matrix.node_version, vec![20, 22]);
    assert!(
        test.steps
            .iter()
            .any(|step| step.run.as_deref() == Some("npm ci"))
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct ComposeConfig {
    services: BTreeMap<String, ComposeService>,
    secrets: BTreeMap<String, ComposeSecret>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ComposeService {
    image: Option<String>,
    command: Option<String>,
    build: Option<ComposeBuild>,
    ports: Option<Vec<String>>,
    depends_on: Option<ComposeDependsOn>,
    networks: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum ComposeBuild {
    Path(String),
    Config { context: String, target: String },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum ComposeDependsOn {
    List(Vec<String>),
    Conditions(BTreeMap<String, ComposeDependency>),
}

#[derive(Debug, Deserialize, PartialEq)]
struct ComposeDependency {
    condition: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ComposeSecret {
    file: String,
}

#[test]
fn downstream_compose_service_shapes_match_serde_yaml() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");
    let compose: ComposeConfig = assert_yaml_matches_serde(input);

    assert_eq!(
        compose.services["db"].image.as_deref(),
        Some("mariadb:10-focal")
    );
    assert_eq!(
        compose.services["proxy"].depends_on.as_ref(),
        Some(&ComposeDependsOn::List(vec!["backend".to_owned()]))
    );
    assert_eq!(
        compose.secrets["db-password"].file.as_str(),
        "db/password.txt"
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct KubernetesDoc {
    kind: String,
    metadata: KubernetesMetadata,
}

#[derive(Debug, Deserialize, PartialEq)]
struct KubernetesMetadata {
    name: String,
}

#[test]
fn downstream_kubernetes_stream_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/kubernetes/multi-doc.yaml");
    let parsed: Vec<KubernetesDoc> = yaml::from_documents_str(input).expect("yaml k8s stream");
    let reference = serde_yaml::Deserializer::from_str(input)
        .map(KubernetesDoc::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml k8s stream");

    assert_eq!(parsed, reference);
    assert_eq!(parsed[0].kind.as_str(), "Service");
    assert_eq!(parsed[1].metadata.name.as_str(), "yaml-demo");
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmChart {
    #[serde(rename = "apiVersion")]
    api_version: String,
    name: String,
    description: String,
    #[serde(rename = "type")]
    chart_type: String,
    version: String,
    #[serde(rename = "appVersion")]
    app_version: String,
}

#[test]
fn downstream_helm_chart_metadata_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml");
    let chart: HelmChart = assert_yaml_matches_serde(input);

    assert_eq!(chart.name.as_str(), "hello-world");
    assert_eq!(chart.app_version.as_str(), "1.16.0");
}

#[derive(Debug, Deserialize, PartialEq)]
struct OpenApiSpec {
    openapi: String,
    info: OpenApiInfo,
    paths: BTreeMap<String, OpenApiPath>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OpenApiInfo {
    title: String,
    version: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OpenApiPath {
    get: Option<OpenApiOperation>,
    post: Option<OpenApiOperation>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OpenApiOperation {
    #[serde(rename = "operationId")]
    operation_id: String,
    tags: Vec<String>,
}

#[test]
fn downstream_openapi_operation_map_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/openapi/upstream-petstore.yaml");
    let spec: OpenApiSpec = assert_yaml_matches_serde(input);

    assert_eq!(spec.info.title.as_str(), "Swagger Petstore");
    assert_eq!(
        spec.paths["/pets"]
            .get
            .as_ref()
            .expect("get operation")
            .operation_id
            .as_str(),
        "listPets"
    );
    assert_eq!(
        spec.paths["/pets"]
            .post
            .as_ref()
            .expect("post operation")
            .tags,
        vec!["pets".to_owned()]
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct WranglerConfig {
    name: String,
    main: String,
    durable_objects: DurableObjects,
    migrations: Vec<WranglerMigration>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct DurableObjects {
    bindings: Vec<DurableObjectBinding>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct DurableObjectBinding {
    name: String,
    class_name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct WranglerMigration {
    tag: String,
    #[serde(default)]
    new_classes: Vec<String>,
    #[serde(default)]
    new_sqlite_classes: Vec<String>,
}

#[test]
fn downstream_wrangler_bindings_match_serde_yaml() {
    let input =
        include_str!("fixtures/real-world/cloudflare/adapted-durable-objects-wrangler.yaml");
    let wrangler: WranglerConfig = assert_yaml_matches_serde(input);

    assert_eq!(wrangler.name.as_str(), "durable-objects");
    assert_eq!(
        wrangler.durable_objects.bindings[1].class_name.as_str(),
        "SQLiteDurableObject"
    );
    assert_eq!(
        wrangler.migrations[0].new_sqlite_classes,
        vec!["SQLiteDurableObject".to_owned()]
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct AnsiblePlay {
    name: String,
    hosts: String,
    remote_user: String,
    roles: Vec<String>,
}

#[test]
fn downstream_ansible_playbook_sequence_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/ansible/upstream-lamp-simple-site.yml");
    let parsed: Vec<AnsiblePlay> = yaml::from_str(input).expect("yaml ansible playbook");
    let reference: Vec<AnsiblePlay> =
        serde_yaml::from_str(input).expect("serde_yaml ansible playbook");

    assert_eq!(parsed, reference);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[2].roles, vec!["db".to_owned()]);
}
