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

#[test]
fn downstream_kubernetes_crd_schema_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml");
    let parsed: Vec<serde_json::Value> =
        yaml::from_documents_str(input).expect("yaml k8s crd stream");
    let reference = serde_yaml::Deserializer::from_str(input)
        .map(serde_json::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml k8s crd stream");

    assert_eq!(parsed, reference);
    assert_eq!(parsed[0]["kind"].as_str(), Some("CustomResourceDefinition"));
    assert_eq!(
        parsed[0]["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"]
            ["properties"]["rules"]["items"]["properties"]["enabled"]["default"]
            .as_bool(),
        Some(true)
    );
    assert_eq!(parsed[1]["kind"].as_str(), Some("Widget"));
    assert_eq!(
        parsed[1]["spec"]["rules"][1]["enabled"].as_bool(),
        Some(false)
    );
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
struct HelmValues {
    #[serde(rename = "replicaCount")]
    replica_count: u32,
    image: HelmImage,
    service: HelmService,
    resources: HelmResources,
    env: Vec<NameValue>,
    config: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmImage {
    repository: String,
    tag: String,
    #[serde(rename = "pullPolicy")]
    pull_policy: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmService {
    #[serde(rename = "type")]
    kind: String,
    port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmResources {
    requests: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct NameValue {
    name: String,
    value: String,
}

#[test]
fn downstream_helm_values_match_serde_yaml() {
    let input = include_str!("fixtures/real-world/helm/values.yaml");
    let values: HelmValues = assert_yaml_matches_serde(input);

    assert_eq!(values.replica_count, 2);
    assert_eq!(values.image.repository.as_str(), "ghcr.io/example/app");
    assert_eq!(values.image.pull_policy.as_str(), "IfNotPresent");
    assert_eq!(values.service.port, 8080);
    assert_eq!(values.resources.requests["cpu"].as_str(), "100m");
    assert_eq!(values.env[1].name.as_str(), "FEATURE_FLAG");
    assert!(values.config.contains("feature.enabled=true"));
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmChartWithDependencies {
    #[serde(rename = "apiVersion")]
    api_version: String,
    name: String,
    #[serde(rename = "appVersion")]
    app_version: String,
    #[serde(rename = "kubeVersion")]
    kube_version: String,
    annotations: BTreeMap<String, String>,
    dependencies: Vec<HelmDependency>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct HelmDependency {
    name: String,
    version: String,
    repository: String,
    condition: Option<String>,
    alias: Option<String>,
    #[serde(rename = "import-values")]
    import_values: Option<Vec<HelmImportValue>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum HelmImportValue {
    Scalar(String),
    ChildParent { child: String, parent: String },
}

#[test]
fn downstream_helm_chart_dependencies_match_serde_yaml() {
    let input = include_str!("fixtures/real-world/helm/Chart.yaml");
    let chart: HelmChartWithDependencies = assert_yaml_matches_serde(input);

    assert_eq!(chart.api_version.as_str(), "v2");
    assert_eq!(chart.name.as_str(), "yaml-demo");
    assert_eq!(chart.app_version.as_str(), "2026.5.24");
    assert_eq!(chart.kube_version.as_str(), ">=1.28.0-0");
    assert_eq!(
        chart.annotations["artifacthub.io/containsSecurityUpdates"].as_str(),
        "false"
    );
    assert_eq!(chart.dependencies[0].name.as_str(), "postgresql");
    assert_eq!(chart.dependencies[0].version.as_str(), "15.5.0");
    assert_eq!(chart.dependencies[0].alias.as_deref(), Some("app-db"));
    match chart.dependencies[1]
        .import_values
        .as_deref()
        .expect("redis import-values")
    {
        [
            HelmImportValue::Scalar(defaults),
            HelmImportValue::ChildParent { child, parent },
        ] => {
            assert_eq!(defaults.as_str(), "defaults");
            assert_eq!(child.as_str(), "exports");
            assert_eq!(parent.as_str(), "redis");
        }
        actual => panic!("unexpected import-values: {actual:?}"),
    }
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

#[test]
fn downstream_openapi_polymorphism_matches_serde_yaml() {
    let input = include_str!("fixtures/real-world/openapi/operations-and-polymorphism.yaml");
    let parsed: serde_json::Value = yaml::from_str(input).expect("yaml openapi");
    let reference: serde_json::Value = serde_yaml::from_str(input).expect("serde_yaml openapi");

    assert_eq!(parsed, reference);
    assert_eq!(
        parsed["paths"]["/orders/{orderId}"]["parameters"][0]["required"].as_bool(),
        Some(true)
    );
    assert_eq!(
        parsed["paths"]["/orders/{orderId}"]["get"]["operationId"].as_str(),
        Some("getOrder")
    );
    assert_eq!(
        parsed["components"]["schemas"]["Order"]["properties"]["status"]["enum"][2].as_str(),
        Some("refunded")
    );
    assert_eq!(
        parsed["components"]["schemas"]["LineItem"]["properties"]["quantity"]["default"].as_i64(),
        Some(1)
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

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedAnsiblePlay {
    name: String,
    hosts: String,
    vars: BTreeMap<String, String>,
    tasks: Vec<TaggedAnsibleTask>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedAnsibleTask {
    name: String,
    #[serde(rename = "ansible.builtin.copy")]
    copy: TaggedAnsibleCopy,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedAnsibleCopy {
    dest: String,
    content: String,
}

#[test]
fn downstream_ansible_tagged_scalars_match_serde_yaml() {
    let input = include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml");
    let plays: Vec<TaggedAnsiblePlay> = assert_yaml_matches_serde(input);

    assert_eq!(plays[0].name.as_str(), "Deploy app with tagged secrets");
    assert!(plays[0].vars["db_password"].contains("$ANSIBLE_VAULT"));
    assert_eq!(
        plays[0].vars["raw_template"].as_str(),
        "{{ literal_must_not_render }}"
    );
    assert_eq!(plays[0].tasks[0].copy.dest.as_str(), "/etc/yaml-demo/.env");
    assert!(
        plays[0].tasks[0]
            .copy
            .content
            .contains("TEMPLATE={{ raw_template }}")
    );
}
