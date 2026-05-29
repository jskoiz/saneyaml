#![allow(dead_code)]

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
struct Workflow {
    name: String,
    on: WorkflowTriggers,
    permissions: BTreeMap<String, String>,
    jobs: BTreeMap<String, MatrixJob>,
}

#[derive(Debug, Deserialize)]
struct WorkflowTriggers {
    push: PushTrigger,
    workflow_dispatch: WorkflowDispatch,
}

#[derive(Debug, Deserialize)]
struct PushTrigger {
    branches: Vec<String>,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WorkflowDispatch {
    inputs: BTreeMap<String, WorkflowInput>,
}

#[derive(Debug, Deserialize)]
struct WorkflowInput {
    description: String,
    required: bool,
    default: String,
}

#[derive(Debug, Deserialize)]
struct MatrixJob {
    name: String,
    #[serde(rename = "runs-on")]
    runs_on: String,
    strategy: Strategy,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize)]
struct Strategy {
    #[serde(rename = "fail-fast")]
    fail_fast: bool,
    matrix: Matrix,
}

#[derive(Debug, Deserialize)]
struct Matrix {
    os: Vec<String>,
    #[serde(rename = "node-version")]
    node_version: Vec<u16>,
    include: Vec<MatrixInclude>,
}

#[derive(Debug, Deserialize)]
struct MatrixInclude {
    os: String,
    #[serde(rename = "node-version")]
    node_version: String,
    coverage: bool,
}

#[derive(Debug, Deserialize)]
struct WorkflowStep {
    uses: Option<String>,
    name: Option<String>,
    run: Option<String>,
    #[serde(rename = "if")]
    r#if: Option<String>,
    with: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Chart {
    #[serde(rename = "apiVersion")]
    api_version: String,
    name: String,
    description: String,
    #[serde(rename = "type")]
    r#type: String,
    version: String,
    #[serde(rename = "appVersion")]
    app_version: String,
}

#[derive(Debug, Deserialize)]
struct Wrangler {
    name: String,
    main: String,
    compatibility_date: String,
    compatibility_flags: Vec<String>,
    vars: BTreeMap<String, String>,
    routes: Vec<Route>,
    d1_databases: Vec<D1Database>,
}

#[derive(Debug, Deserialize)]
struct Route {
    pattern: String,
    zone_name: String,
}

#[derive(Debug, Deserialize)]
struct D1Database {
    binding: String,
    database_name: String,
    database_id: String,
}

#[derive(Debug, Deserialize)]
struct Play {
    name: String,
    hosts: String,
    remote_user: String,
    roles: Vec<String>,
}

fn main() {
    github_actions_matrix_uses_package_alias();
    docker_compose_merge_anchor_expands_through_package_alias();
    kubernetes_stream_uses_package_alias_deserializer();
    helm_chart_reads_through_package_alias();
    openapi_value_reads_through_package_alias();
    wrangler_reads_through_package_alias();
    ansible_playbook_reads_through_package_alias();
}

fn github_actions_matrix_uses_package_alias() {
    let input = include_str!("../fixtures/real-world/github-actions/matrix-ci.yaml");
    let workflow: Workflow = serde_yaml::from_str(input).expect("matrix workflow parses");
    assert_eq!(workflow.name, "Matrix CI");
    assert_eq!(workflow.on.push.branches[1], "release/**");
    assert_eq!(
        workflow
            .on
            .workflow_dispatch
            .inputs
            .get("dry-run")
            .expect("dry-run input")
            .default,
        "false"
    );
    assert_eq!(workflow.permissions["id-token"], "write");
    let job = &workflow.jobs["test"];
    assert_eq!(job.runs_on, "${{ matrix.os }}");
    assert!(!job.strategy.fail_fast);
    assert_eq!(job.strategy.matrix.os[1], "macos-latest");
    assert_eq!(job.strategy.matrix.node_version, [20, 22]);
    assert!(job.strategy.matrix.include[0].coverage);
    assert_eq!(job.steps[1].with.as_ref().unwrap()["cache"], "npm");
}

fn docker_compose_merge_anchor_expands_through_package_alias() {
    let input = include_str!("../fixtures/real-world/docker-compose/compose-anchors.yaml");
    let value: serde_yaml::Value = serde_yaml::from_str(input).expect("compose parses");
    let web = &value["services"]["web"];
    let worker = &value["services"]["worker"];
    assert!(web["<<"].is_null());
    assert!(worker["<<"].is_null());
    assert_eq!(web["restart"].as_str(), Some("unless-stopped"));
    assert_eq!(web["logging"]["driver"].as_str(), Some("json-file"));
    assert_eq!(web["environment"]["RUST_LOG"].as_str(), Some("info"));
    assert_eq!(web["image"].as_str(), Some("nginx:latest"));
    assert_eq!(worker["command"][2].as_str(), Some("default"));
    assert_eq!(worker["restart"].as_str(), Some("unless-stopped"));
}

fn kubernetes_stream_uses_package_alias_deserializer() {
    let input = include_str!("../fixtures/real-world/kubernetes/multi-doc.yaml");
    let docs = serde_yaml::Deserializer::from_str(input)
        .map(Resource::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("kubernetes stream parses");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].api_version, "v1");
    assert_eq!(docs[0].kind, "Service");
    assert_eq!(docs[1].kind, "Deployment");
    assert_eq!(docs[1].metadata.name, "yaml-demo");
}

fn helm_chart_reads_through_package_alias() {
    let input = include_str!("../fixtures/real-world/helm/upstream-hello-world-Chart.yaml");
    let chart: Chart = serde_yaml::from_str(input).expect("chart parses");
    assert_eq!(chart.api_version, "v2");
    assert_eq!(chart.name, "hello-world");
    assert_eq!(chart.r#type, "application");
    assert_eq!(chart.version, "0.1.0");
    assert_eq!(chart.app_version, "1.16.0");
}

fn openapi_value_reads_through_package_alias() {
    let input = include_str!("../fixtures/real-world/openapi/upstream-petstore.yaml");
    let value: serde_yaml::Value = serde_yaml::from_str(input).expect("openapi parses");
    assert_eq!(value["openapi"].as_str(), Some("3.0.0"));
    assert_eq!(
        value["paths"]["/pets"]["get"]["operationId"].as_str(),
        Some("listPets")
    );
    assert_eq!(
        value["paths"]["/pets/{petId}"]["get"]["parameters"][0]["name"].as_str(),
        Some("petId")
    );
    assert_eq!(
        value["components"]["schemas"]["Pet"]["required"][1].as_str(),
        Some("name")
    );
}

fn wrangler_reads_through_package_alias() {
    let input = include_str!("../fixtures/real-world/cloudflare/wrangler.yaml");
    let wrangler: Wrangler = serde_yaml::from_str(input).expect("wrangler parses");
    assert_eq!(wrangler.name, "yaml-demo");
    assert_eq!(wrangler.compatibility_date, "2026-05-23");
    assert_eq!(wrangler.compatibility_flags, ["nodejs_compat"]);
    assert_eq!(wrangler.vars["ENVIRONMENT"], "production");
    assert_eq!(wrangler.routes[0].zone_name, "example.com");
    assert_eq!(wrangler.d1_databases[0].binding, "DB");
}

fn ansible_playbook_reads_through_package_alias() {
    let input = include_str!("../fixtures/real-world/ansible/upstream-lamp-simple-site.yml");
    let plays: Vec<Play> = serde_yaml::from_str(input).expect("ansible playbook parses");
    assert_eq!(plays.len(), 3);
    assert_eq!(plays[0].hosts, "all");
    assert_eq!(plays[1].roles, ["web"]);
    assert_eq!(plays[2].name, "deploy MySQL and configure the databases");
}
