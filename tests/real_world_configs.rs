#![allow(non_snake_case)]

use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Cursor;

fn assert_borrowed_from(source: &str, borrowed: &str) {
    let source_start = source.as_ptr() as usize;
    let source_end = source_start + source.len();
    let borrowed_start = borrowed.as_ptr() as usize;
    let borrowed_end = borrowed_start + borrowed.len();
    assert!(
        borrowed_start >= source_start && borrowed_end <= source_end,
        "`{borrowed}` should borrow from input range {source_start:#x}..{source_end:#x}, got {borrowed_start:#x}..{borrowed_end:#x}"
    );
    let offset = borrowed_start - source_start;
    assert_eq!(
        &source.as_bytes()[offset..offset + borrowed.len()],
        borrowed.as_bytes()
    );
}

#[derive(Debug, Deserialize)]
struct Workflow {
    name: String,
    on: BTreeMap<String, Option<EventFilter>>,
    env: BTreeMap<String, String>,
    jobs: BTreeMap<String, Job>,
}

#[derive(Debug, Deserialize)]
struct EventFilter {
    branches: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Job {
    #[serde(rename = "runs-on")]
    runs_on: String,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    uses: Option<String>,
    name: Option<String>,
    run: Option<String>,
    with: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct MatrixWorkflow {
    name: String,
    on: MatrixTriggers,
    permissions: BTreeMap<String, String>,
    jobs: BTreeMap<String, MatrixJob>,
}

#[derive(Debug, Deserialize)]
struct MatrixTriggers {
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
    steps: Vec<MatrixStep>,
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
struct MatrixStep {
    uses: Option<String>,
    name: Option<String>,
    run: Option<String>,
    #[serde(rename = "if")]
    r#if: Option<String>,
    with: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct StarterNodeWorkflow {
    name: String,
    on: BTreeMap<String, EventFilter>,
    jobs: BTreeMap<String, StarterNodeJob>,
}

#[derive(Debug, Deserialize)]
struct StarterNodeJob {
    #[serde(rename = "runs-on")]
    runs_on: String,
    strategy: StarterNodeStrategy,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct StarterNodeStrategy {
    matrix: StarterNodeMatrix,
}

#[derive(Debug, Deserialize)]
struct StarterNodeMatrix {
    #[serde(rename = "node-version")]
    node_version: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PolymorphicWorkflow {
    name: String,
    on: WorkflowOn,
    permissions: WorkflowPermissions,
    jobs: BTreeMap<String, PolymorphicWorkflowJob>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkflowOn {
    Event(String),
    Events(Vec<String>),
    Configured(BTreeMap<String, yaml::Value>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkflowPermissions {
    Preset(String),
    Map(BTreeMap<String, String>),
}

#[derive(Debug, Deserialize)]
struct PolymorphicWorkflowJob {
    #[serde(rename = "runs-on")]
    runs_on: RunsOn,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RunsOn {
    String(String),
    Labels(Vec<String>),
    Target(RunnerTarget),
}

#[derive(Debug, Deserialize)]
struct RunnerTarget {
    group: Option<String>,
    labels: Option<RunnerLabels>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RunnerLabels {
    String(String),
    List(Vec<String>),
}

#[test]
fn rw_parse_github_actions__minimal_ci() {
    let input = include_str!("fixtures/real-world/github-actions/minimal-ci.yaml");
    let workflow: Workflow = yaml::from_str(input).expect("deserialize workflow");
    assert_eq!(workflow.name, "CI");
    assert_eq!(workflow.on["push"].as_ref().unwrap().branches, ["main"]);
    assert!(workflow.on["pull_request"].is_none());
    assert_eq!(workflow.env["CARGO_TERM_COLOR"], "always");
    let job = &workflow.jobs["test"];
    assert_eq!(job.runs_on, "ubuntu-latest");
    assert_eq!(job.steps[0].uses.as_deref(), Some("actions/checkout@v4"));
    assert_eq!(job.steps[1].name.as_deref(), Some("Test"));
    assert_eq!(job.steps[1].run.as_deref(), Some("cargo test --all"));
    assert_eq!(job.steps[1].with.as_ref().unwrap()["profile"], "ci");
}

#[test]
fn rw_parse_github_actions__matrix_ci() {
    let input = include_str!("fixtures/real-world/github-actions/matrix-ci.yaml");
    let value: yaml::Value = yaml::from_str(input).expect("deserialize matrix workflow value");
    assert_eq!(
        value["on"]["push"]["branches"][1].as_str(),
        Some("release/**")
    );
    assert_eq!(
        value["on"]["workflow_dispatch"]["inputs"]["dry-run"]["required"].as_bool(),
        Some(false)
    );
    assert_eq!(value["permissions"]["id-token"].as_str(), Some("write"));
    assert_eq!(
        value["jobs"]["test"]["runs-on"].as_str(),
        Some("${{ matrix.os }}")
    );
    assert_eq!(
        value["jobs"]["test"]["strategy"]["fail-fast"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["jobs"]["test"]["strategy"]["matrix"]["node-version"][1].as_u64(),
        Some(22)
    );
    assert_eq!(
        value["jobs"]["test"]["strategy"]["matrix"]["include"][0]["coverage"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["jobs"]["test"]["steps"][3]["if"].as_str(),
        Some("${{ matrix.coverage == true }}")
    );

    let workflow: MatrixWorkflow = yaml::from_str(input).expect("deserialize matrix workflow");
    assert_eq!(workflow.name, "Matrix CI");
    assert_eq!(workflow.on.push.branches[1], "release/**");
    assert_eq!(workflow.on.push.paths[0], "src/**");
    assert!(
        !workflow
            .on
            .workflow_dispatch
            .inputs
            .get("dry-run")
            .expect("dry-run input")
            .required
    );
    assert_eq!(
        workflow
            .on
            .workflow_dispatch
            .inputs
            .get("dry-run")
            .expect("dry-run input")
            .description,
        "Skip publish steps"
    );
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
    assert_eq!(workflow.permissions["contents"], "read");
    let job = &workflow.jobs["test"];
    assert_eq!(
        job.name,
        "Test ${{ matrix.os }} / Node ${{ matrix.node-version }}"
    );
    assert_eq!(job.runs_on, "${{ matrix.os }}");
    assert!(!job.strategy.fail_fast);
    assert_eq!(job.strategy.matrix.os[1], "macos-latest");
    assert_eq!(job.strategy.matrix.node_version, [20, 22]);
    assert_eq!(job.strategy.matrix.include[0].os, "ubuntu-latest");
    assert_eq!(job.strategy.matrix.include[0].node_version, "22");
    assert!(job.strategy.matrix.include[0].coverage);
    assert_eq!(job.steps[0].uses.as_deref(), Some("actions/checkout@v4"));
    assert_eq!(job.steps[1].name.as_deref(), Some("Setup Node"));
    assert_eq!(job.steps[1].with.as_ref().unwrap()["cache"], "npm");
    assert_eq!(
        job.steps[3].r#if.as_deref(),
        Some("${{ matrix.coverage == true }}")
    );
    assert_eq!(job.steps[3].run.as_deref(), Some("npm test -- --coverage"));
}

#[test]
fn rw_parse_github_actions__starter_node_ci_upstream_snapshot() {
    let input = include_str!("fixtures/real-world/github-actions/starter-node-ci.yml");

    let value: yaml::Value = yaml::from_str(input).expect("deserialize starter workflow value");
    assert_eq!(value["name"].as_str(), Some("Node.js CI"));
    assert_eq!(
        value["on"]["push"]["branches"][0].as_str(),
        Some("$default-branch")
    );
    assert_eq!(
        value["jobs"]["build"]["strategy"]["matrix"]["node-version"][2].as_str(),
        Some("22.x")
    );
    assert_eq!(
        value["jobs"]["build"]["steps"][1]["with"]["cache"].as_str(),
        Some("npm")
    );

    let workflow: StarterNodeWorkflow =
        yaml::from_str(input).expect("deserialize upstream starter workflow");
    assert_eq!(workflow.name, "Node.js CI");
    assert_eq!(workflow.on["push"].branches, ["$default-branch"]);
    assert_eq!(workflow.on["pull_request"].branches, ["$default-branch"]);
    let job = &workflow.jobs["build"];
    assert_eq!(job.runs_on, "ubuntu-latest");
    assert_eq!(job.strategy.matrix.node_version, ["18.x", "20.x", "22.x"]);
    assert_eq!(job.steps[0].uses.as_deref(), Some("actions/checkout@v4"));
    assert_eq!(
        job.steps[1].name.as_deref(),
        Some("Use Node.js ${{ matrix.node-version }}")
    );
    assert_eq!(job.steps[1].uses.as_deref(), Some("actions/setup-node@v4"));
    assert_eq!(
        job.steps[1].with.as_ref().unwrap()["node-version"],
        "${{ matrix.node-version }}"
    );
    assert_eq!(job.steps[1].with.as_ref().unwrap()["cache"], "npm");
    assert_eq!(job.steps[2].run.as_deref(), Some("npm ci"));
    assert_eq!(
        job.steps[3].run.as_deref(),
        Some("npm run build --if-present")
    );
    assert_eq!(job.steps[4].run.as_deref(), Some("npm test"));
}

#[test]
fn rw_parse_github_actions__polymorphic_workflow() {
    let input = include_str!("fixtures/real-world/github-actions/polymorphic-workflow.yaml");

    let value: yaml::Value = yaml::from_str(input).expect("deserialize polymorphic workflow value");
    assert_eq!(value["on"][0].as_str(), Some("push"));
    assert_eq!(value["on"][1].as_str(), Some("pull_request"));
    assert_eq!(value["permissions"].as_str(), Some("read-all"));
    assert_eq!(
        value["jobs"]["hosted"]["runs-on"].as_str(),
        Some("ubuntu-latest")
    );
    assert_eq!(
        value["jobs"]["self_hosted"]["runs-on"][2].as_str(),
        Some("x64")
    );
    assert_eq!(
        value["jobs"]["grouped"]["runs-on"]["group"].as_str(),
        Some("ubuntu-runners")
    );
    assert_eq!(
        value["jobs"]["grouped"]["runs-on"]["labels"].as_str(),
        Some("ubuntu-24.04-16core")
    );

    let workflow: PolymorphicWorkflow =
        yaml::from_str(input).expect("deserialize polymorphic workflow");
    assert_eq!(workflow.name, "Polymorphic Workflow");
    assert!(matches!(
        workflow.on,
        WorkflowOn::Events(ref events)
            if events.iter().map(String::as_str).collect::<Vec<_>>()
                == ["push", "pull_request"]
    ));
    assert!(matches!(
        workflow.permissions,
        WorkflowPermissions::Preset(ref preset) if preset == "read-all"
    ));
    assert!(matches!(
        workflow.jobs["hosted"].runs_on,
        RunsOn::String(ref label) if label == "ubuntu-latest"
    ));
    assert!(matches!(
        workflow.jobs["self_hosted"].runs_on,
        RunsOn::Labels(ref labels)
            if labels.iter().map(String::as_str).collect::<Vec<_>>()
                == ["self-hosted", "linux", "x64"]
    ));
    assert!(matches!(
        workflow.jobs["grouped"].runs_on,
        RunsOn::Target(RunnerTarget {
            group: Some(ref group),
            labels: Some(RunnerLabels::String(ref label)),
        }) if group == "ubuntu-runners" && label == "ubuntu-24.04-16core"
    ));
    assert_eq!(
        workflow.jobs["hosted"].steps[0].uses.as_deref(),
        Some("actions/checkout@v4")
    );
    assert_eq!(
        workflow.jobs["grouped"].steps[0].run.as_deref(),
        Some("echo grouped runner")
    );

    let single_event: WorkflowOn =
        yaml::from_str("workflow_dispatch\n").expect("single event workflow trigger");
    assert!(matches!(single_event, WorkflowOn::Event(ref event) if event == "workflow_dispatch"));

    let configured_event: WorkflowOn =
        yaml::from_str("push:\n  branches: [main]\n").expect("configured workflow trigger");
    assert!(matches!(
        configured_event,
        WorkflowOn::Configured(ref events) if events["push"]["branches"][0].as_str() == Some("main")
    ));

    let permission_map: WorkflowPermissions =
        yaml::from_str("contents: read\n").expect("mapping workflow permissions");
    assert!(matches!(
        permission_map,
        WorkflowPermissions::Map(ref permissions) if permissions["contents"] == "read"
    ));

    let runner_labels: RunnerLabels =
        yaml::from_str("[self-hosted, linux]\n").expect("runner label list");
    assert!(matches!(
        runner_labels,
        RunnerLabels::List(ref labels)
            if labels.iter().map(String::as_str).collect::<Vec<_>>()
                == ["self-hosted", "linux"]
    ));
}

#[derive(Debug, Deserialize)]
struct Compose {
    version: String,
    services: BTreeMap<String, Service>,
}

#[derive(Debug, Deserialize)]
struct Service {
    image: String,
    platform: Option<String>,
    ports: Option<Vec<String>>,
    environment: Option<BTreeMap<String, String>>,
    deploy: Option<Deploy>,
    command: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Deploy {
    replicas: Option<u32>,
    resources: Option<ComposeResources>,
}

#[derive(Debug, Deserialize)]
struct ComposeResources {
    limits: BTreeMap<String, String>,
    reservations: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AwesomeCompose {
    services: BTreeMap<String, AwesomeComposeService>,
    volumes: BTreeMap<String, Option<yaml::Value>>,
    secrets: BTreeMap<String, SecretFile>,
    networks: BTreeMap<String, Option<yaml::Value>>,
}

#[derive(Debug, Deserialize)]
struct AwesomeComposeService {
    image: Option<String>,
    build: Option<BuildConfig>,
    command: Option<Command>,
    restart: Option<String>,
    healthcheck: Option<Healthcheck>,
    secrets: Option<Vec<String>>,
    volumes: Option<Vec<String>>,
    networks: Option<Vec<String>>,
    environment: Option<Environment>,
    expose: Option<Vec<u32>>,
    ports: Option<Vec<String>>,
    depends_on: Option<ComposeDependsOn>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BuildConfig {
    Path(String),
    Context(BuildContext),
}

#[derive(Debug, Deserialize)]
struct BuildContext {
    context: String,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ComposeDependsOn {
    List(Vec<String>),
    Conditions(BTreeMap<String, DependsOn>),
}

#[derive(Debug, Deserialize)]
struct SecretFile {
    file: String,
}

#[test]
fn rw_parse_docker_compose__service_config() {
    let input = include_str!("fixtures/real-world/docker-compose/compose.yaml");
    let compose: Compose = yaml::from_str(input).expect("deserialize compose");
    assert_eq!(compose.version, "3.9");
    assert_eq!(compose.services["web"].image, "nginx:latest");
    assert_eq!(
        compose.services["web"].ports.as_ref().unwrap(),
        &["8080:80", "127.0.0.1:8081:80"]
    );
    assert_eq!(
        compose.services["web"].environment.as_ref().unwrap()["FEATURE_FLAG"],
        "true"
    );
    assert_eq!(
        compose.services["web"].deploy.as_ref().unwrap().replicas,
        Some(2)
    );
    assert_eq!(
        compose.services["worker"].command.as_ref().unwrap(),
        &["worker", "--queue", "default"]
    );
}

#[test]
fn rw_parse_docker_compose__awesome_nginx_flask_mysql_upstream_snapshot() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");

    let value: yaml::Value = yaml::from_str(input).expect("deserialize upstream compose value");
    assert_eq!(
        value["services"]["db"]["image"].as_str(),
        Some("mariadb:10-focal")
    );
    assert_eq!(
        value["services"]["db"]["healthcheck"]["test"][0].as_str(),
        Some("CMD-SHELL")
    );
    assert_eq!(
        value["services"]["db"]["healthcheck"]["test"][1].as_str(),
        Some(
            r#"mysqladmin ping -h 127.0.0.1 --password="$$(cat /run/secrets/db-password)" --silent"#
        )
    );
    assert_eq!(
        value["services"]["backend"]["depends_on"]["db"]["condition"].as_str(),
        Some("service_healthy")
    );
    assert_eq!(
        value["services"]["proxy"]["depends_on"][0].as_str(),
        Some("backend")
    );
    assert_eq!(
        value["secrets"]["db-password"]["file"].as_str(),
        Some("db/password.txt")
    );

    let compose: AwesomeCompose =
        yaml::from_str(input).expect("deserialize upstream compose typed config");
    assert_eq!(compose.services.len(), 3);

    let db = &compose.services["db"];
    assert_eq!(db.image.as_deref(), Some("mariadb:10-focal"));
    assert_eq!(
        db.command.as_ref().map(command_text),
        Some("--default-authentication-plugin=mysql_native_password")
    );
    assert_eq!(db.restart.as_deref(), Some("always"));
    assert_eq!(db.secrets.as_ref().unwrap(), &["db-password"]);
    assert_eq!(db.volumes.as_ref().unwrap(), &["db-data:/var/lib/mysql"]);
    assert_eq!(db.networks.as_ref().unwrap(), &["backnet"]);
    assert_eq!(db.expose.as_ref().unwrap(), &[3306, 33060]);
    assert!(matches!(
        db.environment,
        Some(Environment::List(ref items))
            if items == &[
                "MYSQL_DATABASE=example",
                "MYSQL_ROOT_PASSWORD_FILE=/run/secrets/db-password"
            ]
    ));
    assert!(matches!(
        db.healthcheck.as_ref().unwrap().test,
        HealthcheckTest::List(ref items)
            if items[0] == "CMD-SHELL"
                && items[1].contains("mysqladmin ping")
                && items[1].contains("db-password")
    ));

    let backend = &compose.services["backend"];
    assert!(matches!(
        backend.build,
        Some(BuildConfig::Context(ref build))
            if build.context == "backend" && build.target.as_deref() == Some("builder")
    ));
    assert_eq!(backend.ports.as_ref().unwrap(), &["8000:8000"]);
    assert!(matches!(
        backend.depends_on,
        Some(ComposeDependsOn::Conditions(ref depends_on))
            if depends_on["db"].condition == "service_healthy"
    ));

    let proxy = &compose.services["proxy"];
    assert!(matches!(
        proxy.build,
        Some(BuildConfig::Path(ref path)) if path == "proxy"
    ));
    assert_eq!(proxy.ports.as_ref().unwrap(), &["80:80"]);
    assert!(matches!(
        proxy.depends_on,
        Some(ComposeDependsOn::List(ref depends_on)) if depends_on == &["backend"]
    ));

    assert!(compose.volumes["db-data"].is_none());
    assert_eq!(compose.secrets["db-password"].file, "db/password.txt");
    assert!(compose.networks["backnet"].is_none());
    assert!(compose.networks["frontnet"].is_none());
}

#[test]
fn rw_parse_docker_compose__extension_anchors_and_literal_merge_keys() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml");
    let compose: Compose = yaml::from_str(input).expect("deserialize compose with anchors");
    assert_eq!(compose.version, "3.9");
    assert_eq!(compose.services["web"].image, "nginx:latest");
    assert_eq!(
        compose.services["web"].ports.as_ref().unwrap(),
        &["8080:80"]
    );
    assert_eq!(compose.services["worker"].image, "example/worker:latest");
    assert_eq!(
        compose.services["worker"].command.as_ref().unwrap(),
        &["worker", "--queue", "default"]
    );

    let value: yaml::Value = yaml::from_str(input).expect("dynamic compose value");
    assert_eq!(
        value["x-service-defaults"]["restart"].as_str(),
        Some("unless-stopped")
    );
    assert_eq!(
        value["services"]["web"]["<<"]["logging"]["driver"].as_str(),
        Some("json-file")
    );
    assert_eq!(value["services"]["web"]["restart"].as_str(), None);
}

#[test]
fn rw_parse_docker_compose__apply_merge_expands_extension_defaults() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml");
    let mut value: yaml::Value = yaml::from_str(input).expect("dynamic compose value");

    value.apply_merge().expect("apply compose merge keys");

    assert!(value["services"]["web"]["<<"].is_null());
    assert_eq!(
        value["services"]["web"]["restart"].as_str(),
        Some("unless-stopped")
    );
    assert_eq!(
        value["services"]["web"]["logging"]["driver"].as_str(),
        Some("json-file")
    );
    assert_eq!(
        value["services"]["web"]["environment"]["RUST_LOG"].as_str(),
        Some("info")
    );
    assert_eq!(
        value["services"]["web"]["image"].as_str(),
        Some("nginx:latest")
    );
    assert_eq!(
        value["services"]["web"]["ports"][0].as_str(),
        Some("8080:80")
    );
    assert_eq!(
        value["services"]["worker"]["command"][0].as_str(),
        Some("worker")
    );
}

#[test]
fn rw_parse_docker_compose__platform_and_deploy_resources() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-platform-resources.yaml");

    let value: yaml::Value = yaml::from_str(input).expect("deserialize compose resources value");
    assert_eq!(
        value["services"]["api"]["platform"].as_str(),
        Some("linux/amd64")
    );
    assert_eq!(
        value["services"]["api"]["deploy"]["resources"]["limits"]["cpus"].as_str(),
        Some("0.50")
    );
    assert_eq!(
        value["services"]["api"]["deploy"]["resources"]["limits"]["memory"].as_str(),
        Some("512M")
    );
    assert_eq!(
        value["services"]["api"]["deploy"]["resources"]["reservations"]["memory"].as_str(),
        Some("256M")
    );

    let compose: Compose = yaml::from_str(input).expect("deserialize compose");
    let api = &compose.services["api"];
    assert_eq!(api.image, "ghcr.io/example/api:1.2.3");
    assert_eq!(api.platform.as_deref(), Some("linux/amd64"));
    assert_eq!(api.deploy.as_ref().unwrap().replicas, Some(1));
    let resources = api.deploy.as_ref().unwrap().resources.as_ref().unwrap();
    assert_eq!(resources.limits["cpus"], "0.50");
    assert_eq!(resources.limits["memory"], "512M");
    assert_eq!(resources.reservations["cpus"], "0.25");
    assert_eq!(resources.reservations["memory"], "256M");
}

#[derive(Debug, Deserialize)]
struct PolymorphicCompose {
    version: String,
    #[serde(rename = "x-healthcheck")]
    x_healthcheck: Healthcheck,
    services: BTreeMap<String, PolymorphicService>,
}

#[derive(Debug, Deserialize)]
struct PolymorphicService {
    image: String,
    profiles: Option<Vec<String>>,
    env_file: Option<Vec<String>>,
    environment: Option<Environment>,
    healthcheck: Healthcheck,
    depends_on: Option<BTreeMap<String, DependsOn>>,
    volumes: Option<Vec<VolumeMount>>,
    command: Option<Command>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Environment {
    Map(BTreeMap<String, String>),
    List(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct Healthcheck {
    test: HealthcheckTest,
    interval: Option<String>,
    timeout: Option<String>,
    retries: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HealthcheckTest {
    String(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct DependsOn {
    condition: String,
}

#[derive(Debug, Deserialize)]
struct VolumeMount {
    #[serde(rename = "type")]
    kind: String,
    source: String,
    target: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Command {
    String(String),
    List(Vec<String>),
}

fn command_text(command: &Command) -> &str {
    match command {
        Command::String(command) => command,
        Command::List(_) => panic!("expected string command"),
    }
}

#[test]
fn rw_parse_docker_compose__polymorphic_service_fields() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-polymorphic.yaml");
    let compose: PolymorphicCompose =
        yaml::from_str(input).expect("deserialize polymorphic compose");
    assert_eq!(compose.version, "3.9");
    assert!(matches!(
        compose.x_healthcheck.test,
        HealthcheckTest::List(ref items) if items[0] == "CMD-SHELL"
    ));

    let web = &compose.services["web"];
    assert_eq!(web.image, "ghcr.io/example/web:1.2.3");
    assert_eq!(web.profiles.as_ref().unwrap(), &["web"]);
    assert_eq!(web.env_file.as_ref().unwrap(), &[".env", ".env.local"]);
    assert!(matches!(
        web.environment,
        Some(Environment::List(ref items))
            if items == &["RUST_LOG=info", "FEATURE_FLAG=true", "EMPTY_VALUE="]
    ));
    assert_eq!(web.healthcheck.retries, Some(5));
    assert_eq!(web.healthcheck.interval.as_deref(), Some("10s"));
    assert_eq!(web.healthcheck.timeout.as_deref(), Some("3s"));
    assert_eq!(
        web.depends_on.as_ref().unwrap()["db"].condition,
        "service_healthy"
    );
    let volume = &web.volumes.as_ref().unwrap()[0];
    assert_eq!(volume.kind, "bind");
    assert_eq!(volume.source, "./config");
    assert_eq!(volume.target, "/etc/yaml-demo");

    let worker = &compose.services["worker"];
    assert!(matches!(
        worker.command,
        Some(Command::String(ref command)) if command == "bundle exec sidekiq"
    ));
    assert!(matches!(
        worker.healthcheck.test,
        HealthcheckTest::String(ref test) if test == "pgrep -f sidekiq"
    ));

    let db = &compose.services["db"];
    assert!(matches!(
        db.command,
        Some(Command::List(ref command))
            if command == &["postgres", "-c", "shared_buffers=256MB"]
    ));
    assert!(matches!(
        db.environment,
        Some(Environment::Map(ref values))
            if values["POSTGRES_DB"] == "app" && values["POSTGRES_PASSWORD"] == "example"
    ));
}

#[derive(Debug, Deserialize)]
struct Deployment {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: DeploymentSpec,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    name: Option<String>,
    namespace: Option<String>,
    labels: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct DeploymentSpec {
    replicas: u32,
    template: PodTemplate,
}

#[derive(Debug, Deserialize)]
struct PodTemplate {
    metadata: Metadata,
    spec: PodSpec,
}

#[derive(Debug, Deserialize)]
struct PodSpec {
    containers: Vec<Container>,
}

#[derive(Debug, Deserialize)]
struct Container {
    name: String,
    image: String,
    ports: Vec<ContainerPort>,
    resources: Resources,
}

#[derive(Debug, Deserialize)]
struct ContainerPort {
    #[serde(rename = "containerPort")]
    container_port: u16,
}

#[derive(Debug, Deserialize)]
struct Resources {
    requests: BTreeMap<String, String>,
}

#[test]
fn rw_parse_kubernetes__deployment_manifest() {
    let input = include_str!("fixtures/real-world/kubernetes/deployment.yaml");
    let deployment: Deployment = yaml::from_str(input).expect("deserialize deployment");
    assert_eq!(deployment.api_version, "apps/v1");
    assert_eq!(deployment.kind, "Deployment");
    assert_eq!(deployment.metadata.name.as_deref(), Some("yaml-demo"));
    assert_eq!(deployment.spec.replicas, 2);
    assert_eq!(
        deployment.spec.template.metadata.labels.unwrap()["app"],
        "yaml-demo"
    );
    let container = &deployment.spec.template.spec.containers[0];
    assert_eq!(container.name, "app");
    assert_eq!(container.image, "nginx:1.25");
    assert_eq!(container.ports[0].container_port, 80);
    assert_eq!(container.resources.requests["cpu"], "100m");
    assert_eq!(container.resources.requests["memory"], "128Mi");
}

#[derive(Debug, Deserialize)]
struct ManifestHeader {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
}

#[derive(Debug, Deserialize)]
struct CustomResourceDefinition {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: CrdSpec,
}

#[derive(Debug, Deserialize)]
struct CrdSpec {
    group: String,
    scope: String,
    names: CrdNames,
    versions: Vec<CrdVersion>,
}

#[derive(Debug, Deserialize)]
struct CrdNames {
    plural: String,
    singular: String,
    kind: String,
    #[serde(rename = "shortNames")]
    short_names: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CrdVersion {
    name: String,
    served: bool,
    storage: bool,
    schema: CrdSchema,
    #[serde(rename = "additionalPrinterColumns")]
    additional_printer_columns: Option<Vec<AdditionalPrinterColumn>>,
}

#[derive(Debug, Deserialize)]
struct CrdSchema {
    #[serde(rename = "openAPIV3Schema")]
    open_api_v3_schema: yaml::Value,
}

#[derive(Debug, Deserialize)]
struct AdditionalPrinterColumn {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "jsonPath")]
    json_path: String,
}

#[derive(Debug, Deserialize)]
struct Widget {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: WidgetSpec,
}

#[derive(Debug, Deserialize)]
struct WidgetSpec {
    image: String,
    replicas: u32,
    mode: String,
    config: BTreeMap<String, String>,
    rules: Vec<WidgetRule>,
}

#[derive(Debug, Deserialize)]
struct WidgetRule {
    name: String,
    enabled: bool,
}

#[test]
fn rw_parse_kubernetes__multi_doc_manifest_stream() {
    let input = include_str!("fixtures/real-world/kubernetes/multi-doc.yaml");
    let docs: Vec<ManifestHeader> =
        yaml::from_documents_str(input).expect("deserialize manifest stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].api_version, "v1");
    assert_eq!(docs[0].kind, "Service");
    assert_eq!(docs[0].metadata.name.as_deref(), Some("yaml-demo"));
    assert_eq!(docs[1].kind, "Deployment");
}

#[test]
fn rw_parse_kubernetes__crd_openapi_schema_and_custom_resource_stream() {
    let input = include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml");
    let docs: Vec<yaml::Value> =
        yaml::from_documents_str(input).expect("deserialize CRD/custom resource stream");

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["kind"].as_str(), Some("CustomResourceDefinition"));
    assert_eq!(docs[1]["kind"].as_str(), Some("Widget"));

    let crd: CustomResourceDefinition = yaml::from_value(docs[0].clone()).expect("typed CRD");
    assert_eq!(crd.api_version, "apiextensions.k8s.io/v1");
    assert_eq!(crd.kind, "CustomResourceDefinition");
    assert_eq!(crd.metadata.name.as_deref(), Some("widgets.example.com"));
    assert_eq!(crd.spec.group, "example.com");
    assert_eq!(crd.spec.scope, "Namespaced");
    assert_eq!(crd.spec.names.plural, "widgets");
    assert_eq!(crd.spec.names.singular, "widget");
    assert_eq!(crd.spec.names.kind, "Widget");
    assert_eq!(crd.spec.names.short_names.as_ref().unwrap()[0], "wdg");

    let version = &crd.spec.versions[0];
    assert_eq!(version.name, "v1alpha1");
    assert!(version.served);
    assert!(version.storage);
    let column = &version.additional_printer_columns.as_ref().unwrap()[0];
    assert_eq!(column.name, "Replicas");
    assert_eq!(column.kind, "integer");
    assert_eq!(column.json_path, ".spec.replicas");

    let schema = &version.schema.open_api_v3_schema;
    let spec_schema = &schema["properties"]["spec"];
    assert_eq!(schema["required"][0].as_str(), Some("spec"));
    assert_eq!(
        spec_schema["properties"]["image"]["type"].as_str(),
        Some("string")
    );
    assert_eq!(
        spec_schema["properties"]["replicas"]["minimum"].as_u64(),
        Some(1)
    );
    assert_eq!(
        spec_schema["properties"]["config"]["x-kubernetes-preserve-unknown-fields"].as_bool(),
        Some(true)
    );
    assert_eq!(
        spec_schema["properties"]["rules"]["x-kubernetes-list-map-keys"][0].as_str(),
        Some("name")
    );

    let widget: Widget = yaml::from_value(docs[1].clone()).expect("typed custom resource");
    assert_eq!(widget.api_version, "example.com/v1alpha1");
    assert_eq!(widget.kind, "Widget");
    assert_eq!(widget.metadata.name.as_deref(), Some("demo-widget"));
    assert_eq!(widget.metadata.namespace.as_deref(), Some("default"));
    assert_eq!(widget.spec.image, "ghcr.io/example/widget:1.0");
    assert_eq!(widget.spec.replicas, 2);
    assert_eq!(widget.spec.mode, "prod");
    assert_eq!(widget.spec.config["LOG_LEVEL"], "info");
    assert_eq!(widget.spec.rules[1].name, "search");
    assert!(!widget.spec.rules[1].enabled);
}

#[test]
fn rw_parse_kubernetes__multi_doc_values_preserve_resource_specs() {
    let input = include_str!("fixtures/real-world/kubernetes/multi-doc.yaml");
    let docs = yaml::Deserializer::from_str(input)
        .map(yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("deserialize manifest values");

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["kind"].as_str(), Some("Service"));
    assert_eq!(
        docs[0]["spec"]["selector"]["app"].as_str(),
        Some("yaml-demo")
    );
    assert_eq!(docs[0]["spec"]["ports"][0]["port"].as_u64(), Some(80));
    assert_eq!(
        docs[0]["spec"]["ports"][0]["targetPort"].as_str(),
        Some("http")
    );
    assert_eq!(docs[1]["kind"].as_str(), Some("Deployment"));
    assert_eq!(
        docs[1]["spec"]["template"]["spec"]["containers"][0]["ports"][0]["name"].as_str(),
        Some("http")
    );
    assert_eq!(
        docs[1]["spec"]["template"]["spec"]["containers"][0]["ports"][0]["containerPort"].as_u64(),
        Some(80)
    );
}

#[test]
fn rw_parse_kubernetes__helm_rendered_stream_preserves_empty_docs_and_ambiguous_strings() {
    let input = include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml");
    let docs =
        yaml::from_documents_reader::<yaml::Value, _>(Cursor::new(input)).expect("stream values");
    let iter_docs = yaml::Deserializer::from_reader(Cursor::new(input))
        .map(yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("iterator stream values");

    assert_eq!(iter_docs, docs);
    assert_eq!(docs.len(), 5);
    assert!(docs[1].is_null());
    assert_eq!(docs[0]["kind"].as_str(), Some("Namespace"));
    assert_eq!(docs[2]["kind"].as_str(), Some("ConfigMap"));
    assert_eq!(docs[3]["kind"].as_str(), Some("Secret"));
    assert_eq!(docs[4]["kind"].as_str(), Some("Deployment"));
    assert!(
        docs[2]["data"]["application.yaml"]
            .as_str()
            .expect("application.yaml block scalar")
            .contains("canary: true")
    );
    assert_eq!(docs[3]["stringData"]["password"].as_str(), Some("yes"));
    assert_eq!(docs[3]["stringData"]["token"].as_str(), Some("on"));
    assert_eq!(
        docs[4]["spec"]["template"]["spec"]["containers"][0]["env"][1]["value"].as_str(),
        Some("on")
    );
    assert_eq!(
        docs[4]["spec"]["template"]["spec"]["containers"][0]["resources"]["requests"]["cpu"]
            .as_str(),
        Some("100m")
    );
}

#[derive(Debug, Deserialize)]
struct ConfigMap {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
    data: BTreeMap<String, String>,
}

#[test]
fn rw_parse_kubernetes__configmap_block_scalars_preserve_data_comments() {
    let input = include_str!("fixtures/real-world/kubernetes/configmap-block-scalars.yaml");
    let config: ConfigMap = yaml::from_str(input).expect("deserialize configmap");
    assert_eq!(config.api_version, "v1");
    assert_eq!(config.kind, "ConfigMap");
    assert_eq!(config.metadata.name.as_deref(), Some("yaml-demo-config"));
    assert_eq!(
        config.data["app.yaml"],
        "# this comment is data\nserver:\n  port: 8080\n\n  # blank line above is data"
    );
    assert_eq!(config.data["message.txt"], "hello\n\nworld\n");
}

#[derive(Debug, Deserialize)]
struct HelmValues {
    #[serde(rename = "replicaCount")]
    replica_count: u32,
    image: HelmImage,
    service: HelmService,
    resources: Resources,
    env: Vec<NameValue>,
    config: String,
}

#[derive(Debug, Deserialize)]
struct HelmImage {
    repository: String,
    tag: String,
    #[serde(rename = "pullPolicy")]
    pull_policy: String,
}

#[derive(Debug, Deserialize)]
struct HelmService {
    #[serde(rename = "type")]
    kind: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct NameValue {
    name: String,
    value: String,
}

#[test]
fn rw_parse_helm__values_yaml() {
    let input = include_str!("fixtures/real-world/helm/values.yaml");
    let values: HelmValues = yaml::from_slice(input.as_bytes()).expect("deserialize helm values");
    assert_eq!(values.replica_count, 2);
    assert_eq!(values.image.repository, "ghcr.io/example/app");
    assert_eq!(values.image.tag, "1.2.3");
    assert_eq!(values.image.pull_policy, "IfNotPresent");
    assert_eq!(values.service.kind, "ClusterIP");
    assert_eq!(values.service.port, 8080);
    assert_eq!(values.resources.requests["cpu"], "100m");
    assert_eq!(values.env[0].name, "RUST_LOG");
    assert_eq!(values.env[1].value, "true");
    assert!(values.config.contains("feature.enabled=true"));
}

#[derive(Debug, Deserialize)]
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
    #[serde(rename = "kubeVersion")]
    kube_version: String,
    home: String,
    sources: Vec<String>,
    keywords: Vec<String>,
    annotations: BTreeMap<String, String>,
    maintainers: Vec<HelmMaintainer>,
    dependencies: Vec<HelmDependency>,
}

#[derive(Debug, Deserialize)]
struct HelmMaintainer {
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct HelmDependency {
    name: String,
    version: String,
    repository: String,
    condition: Option<String>,
    tags: Option<Vec<String>>,
    alias: Option<String>,
    #[serde(rename = "import-values")]
    import_values: Option<Vec<HelmImportValue>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HelmImportValue {
    Scalar(String),
    ChildParent { child: String, parent: String },
}

#[test]
fn rw_parse_helm__chart_yaml_metadata_and_dependencies() {
    let input = include_str!("fixtures/real-world/helm/Chart.yaml");
    let chart: HelmChart = yaml::from_str(input).expect("deserialize helm chart");
    assert_eq!(chart.api_version, "v2");
    assert_eq!(chart.name, "yaml-demo");
    assert_eq!(chart.description, "Demo chart for parser coverage");
    assert_eq!(chart.chart_type, "application");
    assert_eq!(chart.version, "1.2.3");
    assert_eq!(chart.app_version, "2026.5.24");
    assert_eq!(chart.kube_version, ">=1.28.0-0");
    assert_eq!(chart.home, "https://example.com/yaml-demo");
    assert_eq!(chart.sources, ["https://github.com/example/yaml-demo"]);
    assert_eq!(chart.keywords, ["yaml", "parser", "config"]);
    assert_eq!(
        chart.annotations["artifacthub.io/category"],
        "integration-delivery"
    );
    assert_eq!(
        chart.annotations["artifacthub.io/containsSecurityUpdates"],
        "false"
    );
    assert_eq!(chart.maintainers[0].name, "Platform Team");
    assert_eq!(chart.maintainers[0].email, "platform@example.com");

    let postgresql = &chart.dependencies[0];
    assert_eq!(postgresql.name, "postgresql");
    assert_eq!(postgresql.version, "15.5.0");
    assert_eq!(
        postgresql.repository,
        "oci://registry-1.docker.io/bitnamicharts"
    );
    assert_eq!(postgresql.condition.as_deref(), Some("postgresql.enabled"));
    let postgresql_tags = postgresql.tags.as_deref().expect("postgresql tags");
    assert_eq!(postgresql_tags.len(), 1);
    assert_eq!(postgresql_tags[0], "database");
    assert_eq!(postgresql.alias.as_deref(), Some("app-db"));
    assert!(postgresql.import_values.is_none());

    let redis = &chart.dependencies[1];
    assert_eq!(redis.name, "redis");
    assert_eq!(redis.version, "~20.1.0");
    assert_eq!(redis.repository, "https://charts.bitnami.com/bitnami");
    match redis.import_values.as_deref() {
        Some(
            [
                HelmImportValue::Scalar(defaults),
                HelmImportValue::ChildParent { child, parent },
            ],
        ) => {
            assert_eq!(defaults, "defaults");
            assert_eq!(child, "exports");
            assert_eq!(parent, "redis");
        }
        actual => panic!("unexpected import-values: {actual:?}"),
    }

    let value: yaml::Value = yaml::from_str(input).expect("deserialize helm chart value");
    assert_eq!(value["version"].as_str(), Some("1.2.3"));
    assert_eq!(
        value["dependencies"][0]["repository"].as_str(),
        Some("oci://registry-1.docker.io/bitnamicharts")
    );
    assert_eq!(
        value["dependencies"][1]["version"].as_str(),
        Some("~20.1.0")
    );
    assert_eq!(
        value["annotations"]["artifacthub.io/containsSecurityUpdates"].as_str(),
        Some("false")
    );
}

#[derive(Debug, Deserialize)]
struct OpenApi {
    openapi: String,
    info: OpenApiInfo,
    paths: BTreeMap<String, PathItem>,
    components: Components,
}

#[derive(Debug, Deserialize)]
struct OpenApiInfo {
    title: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct PathItem {
    get: Operation,
}

#[derive(Debug, Deserialize)]
struct Operation {
    #[serde(rename = "operationId")]
    operation_id: String,
    responses: BTreeMap<String, Response>,
}

#[derive(Debug, Deserialize)]
struct Response {
    description: String,
}

#[derive(Debug, Deserialize)]
struct Components {
    schemas: BTreeMap<String, Schema>,
}

#[derive(Debug, Deserialize)]
struct Schema {
    #[serde(rename = "type")]
    kind: String,
    required: Option<Vec<String>>,
    properties: Option<BTreeMap<String, Schema>>,
}

#[test]
fn rw_parse_openapi__fragment() {
    let input = include_str!("fixtures/real-world/openapi/petstore-fragment.yaml");
    let spec: OpenApi = yaml::from_reader(Cursor::new(input)).expect("deserialize openapi");
    assert_eq!(spec.openapi, "3.1.0");
    assert_eq!(spec.info.title, "Pet API");
    assert_eq!(spec.info.version, "1.0.0");
    assert_eq!(spec.paths["/pets"].get.operation_id, "listPets");
    assert_eq!(spec.paths["/pets"].get.responses["200"].description, "ok");
    assert_eq!(spec.components.schemas["Pet"].kind, "object");
    assert_eq!(
        spec.components.schemas["Pet"].required.as_ref().unwrap(),
        &["id", "name"]
    );
    assert_eq!(
        spec.components.schemas["Pet"].properties.as_ref().unwrap()["id"].kind,
        "integer"
    );
}

#[test]
fn rw_parse_openapi__value_preserves_dynamic_schema_keys() {
    let input = include_str!("fixtures/real-world/openapi/petstore-fragment.yaml");
    let value: yaml::Value = yaml::from_str(input).expect("deserialize openapi value");

    assert_eq!(value["openapi"].as_str(), Some("3.1.0"));
    assert_eq!(
        value["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["type"]
            .as_str(),
        Some("array")
    );
    assert_eq!(
        value["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["items"]["$ref"]
            .as_str(),
        Some("#/components/schemas/Pet")
    );
    assert_eq!(
        value["components"]["schemas"]["Pet"]["required"][0].as_str(),
        Some("id")
    );
    assert_eq!(
        value["components"]["schemas"]["Pet"]["required"][1].as_str(),
        Some("name")
    );
}

#[test]
fn rw_parse_openapi__path_parameters_examples_and_extensions() {
    let input = include_str!("fixtures/real-world/openapi/operations-and-polymorphism.yaml");

    let docs = yaml::parse_documents(input).expect("parse rich openapi fixture");
    assert_eq!(docs.len(), 1);

    let value: yaml::Value = yaml::from_str(input).expect("deserialize rich openapi value");
    assert_eq!(
        value["paths"]["/orders/{orderId}"]["parameters"][0]["in"].as_str(),
        Some("path")
    );
    assert_eq!(
        value["paths"]["/orders/{orderId}"]["parameters"][0]["required"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["paths"]["/orders/{orderId}"]["get"]["operationId"].as_str(),
        Some("getOrder")
    );
    assert_eq!(
        value["paths"]["/orders/{orderId}"]["get"]["description"].as_str(),
        Some("Returns an order with nested line items.\n\nRequires an authenticated caller.")
    );
    assert_eq!(
        value["paths"]["/orders/{orderId}"]["get"]["responses"]["404"]["content"]
            ["application/problem+json"]["schema"]["$ref"]
            .as_str(),
        Some("#/components/schemas/Error")
    );
    assert_eq!(
        value["components"]["schemas"]["Order"]["properties"]["status"]["enum"][2].as_str(),
        Some("refunded")
    );
    assert_eq!(
        value["components"]["schemas"]["LineItem"]["properties"]["quantity"]["default"].as_u64(),
        Some(1)
    );
    assert_eq!(value["x-tagGroups"][0]["tags"][0].as_str(), Some("orders"));

    let total = value["paths"]["/orders/{orderId}"]["get"]["responses"]["200"]["content"]
        ["application/json"]["examples"]["paid"]["value"]["total"]
        .as_f64()
        .expect("paid example total");
    assert!((total - 19.99).abs() < f64::EPSILON);
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

#[derive(Debug, Deserialize, PartialEq)]
struct BorrowedWrangler<'a> {
    name: &'a str,
    main: &'a str,
    compatibility_date: &'a str,
    compatibility_flags: Vec<&'a str>,
    vars: BTreeMap<&'a str, &'a str>,
    routes: Vec<BorrowedRoute<'a>>,
    d1_databases: Vec<BorrowedD1Database<'a>>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct BorrowedRoute<'a> {
    pattern: &'a str,
    zone_name: &'a str,
}

#[derive(Debug, Deserialize, PartialEq)]
struct BorrowedD1Database<'a> {
    binding: &'a str,
    database_name: &'a str,
    database_id: &'a str,
}

#[test]
fn rw_parse_cloudflare__wrangler_yaml() {
    let input = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let wrangler: Wrangler = yaml::from_str(input).expect("deserialize wrangler yaml");
    assert_eq!(wrangler.name, "yaml-demo");
    assert_eq!(wrangler.main, "src/index.ts");
    assert_eq!(wrangler.compatibility_date, "2026-05-23");
    assert_eq!(wrangler.compatibility_flags, ["nodejs_compat"]);
    assert_eq!(wrangler.vars["ENVIRONMENT"], "production");
    assert_eq!(wrangler.routes[0].pattern, "example.com/*");
    assert_eq!(wrangler.routes[0].zone_name, "example.com");
    assert_eq!(wrangler.d1_databases[0].binding, "DB");
    assert_eq!(wrangler.d1_databases[0].database_name, "yaml-demo");
    assert_eq!(
        wrangler.d1_databases[0].database_id,
        "00000000-0000-0000-0000-000000000000"
    );
}

#[test]
fn rw_parse_cloudflare__wrangler_direct_deserializer_borrows_from_input() {
    let input = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let wrangler: BorrowedWrangler<'_> =
        BorrowedWrangler::deserialize(yaml::Deserializer::from_str(input))
            .expect("deserialize borrowed wrangler yaml");
    let reference: BorrowedWrangler<'_> =
        BorrowedWrangler::deserialize(serde_yaml::Deserializer::from_str(input))
            .expect("serde_yaml borrowed wrangler yaml");

    assert_eq!(wrangler, reference);
    assert_borrowed_from(input, wrangler.name);
    assert_borrowed_from(input, wrangler.main);
    assert_borrowed_from(input, wrangler.compatibility_date);
    assert_borrowed_from(input, wrangler.compatibility_flags[0]);
    assert_borrowed_from(input, wrangler.vars["ENVIRONMENT"]);
    assert_borrowed_from(input, wrangler.routes[0].pattern);
    assert_borrowed_from(input, wrangler.d1_databases[0].database_id);
}

#[derive(Debug, Deserialize)]
struct Play {
    name: String,
    hosts: String,
    #[serde(rename = "become")]
    become_enabled: bool,
    vars: AnsibleVars,
    tasks: Vec<Task>,
}

#[derive(Debug, Deserialize)]
struct AnsibleVars {
    app_name: String,
    app_port: u16,
}

#[derive(Debug, Deserialize)]
struct Task {
    name: String,
    #[serde(rename = "ansible.builtin.copy")]
    copy: Option<CopyTask>,
    #[serde(rename = "ansible.builtin.service")]
    service: Option<ServiceTask>,
}

#[derive(Debug, Deserialize)]
struct CopyTask {
    dest: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ServiceTask {
    name: String,
    state: String,
}

#[test]
fn rw_parse_ansible__playbook() {
    let input = include_str!("fixtures/real-world/ansible/playbook.yaml");
    let plays: Vec<Play> = yaml::from_str(input).expect("deserialize ansible playbook");
    assert_eq!(plays[0].name, "Deploy app");
    assert_eq!(plays[0].hosts, "all");
    assert!(plays[0].become_enabled);
    assert_eq!(plays[0].vars.app_name, "yaml-demo");
    assert_eq!(plays[0].vars.app_port, 8080);
    assert_eq!(plays[0].tasks[0].name, "Render config");
    assert_eq!(
        plays[0].tasks[0].copy.as_ref().unwrap().dest,
        "/etc/yaml-demo/config.ini"
    );
    assert!(
        plays[0].tasks[0]
            .copy
            .as_ref()
            .unwrap()
            .content
            .contains("port={{ app_port }}")
    );
    assert_eq!(
        plays[0].tasks[1].service.as_ref().unwrap().state,
        "restarted"
    );
    assert_eq!(
        plays[0].tasks[1].service.as_ref().unwrap().name,
        "yaml-demo"
    );
}

#[derive(Debug, Deserialize)]
struct TaggedPlay {
    name: String,
    hosts: String,
    vars: BTreeMap<String, String>,
    tasks: Vec<TaggedTask>,
}

#[derive(Debug, Deserialize)]
struct TaggedTask {
    name: String,
    #[serde(rename = "ansible.builtin.copy")]
    copy: CopyTask,
}

#[test]
fn rw_parse_ansible__vault_and_unsafe_tags_preserve_value_tags_and_typed_reads() {
    let input = include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml");

    let value: yaml::Value = yaml::from_str(input).expect("deserialize tagged ansible value");
    let vault = value[0]["vars"]["db_password"]
        .as_tagged()
        .expect("vault tag");
    assert_eq!(vault.tag, yaml::Tag::new("vault"));
    assert!(
        vault
            .value
            .as_str()
            .expect("vault scalar")
            .contains("$ANSIBLE_VAULT;1.1;AES256")
    );
    let unsafe_template = value[0]["vars"]["raw_template"]
        .as_tagged()
        .expect("unsafe tag");
    assert_eq!(unsafe_template.tag, yaml::Tag::new("unsafe"));
    assert_eq!(
        unsafe_template.value.as_str(),
        Some("{{ literal_must_not_render }}")
    );

    let plays: Vec<TaggedPlay> = yaml::from_str(input).expect("deserialize tagged ansible play");
    assert_eq!(plays[0].name, "Deploy app with tagged secrets");
    assert_eq!(plays[0].hosts, "all");
    assert!(plays[0].vars["db_password"].contains("$ANSIBLE_VAULT"));
    assert_eq!(
        plays[0].vars["raw_template"],
        "{{ literal_must_not_render }}"
    );
    assert_eq!(plays[0].tasks[0].name, "Write env");
    assert_eq!(plays[0].tasks[0].copy.dest, "/etc/yaml-demo/.env");
    assert!(
        plays[0].tasks[0]
            .copy
            .content
            .contains("TEMPLATE={{ raw_template }}")
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct EnumConfig {
    mode: Mode,
    action: Action,
}

#[derive(Debug, Deserialize, PartialEq)]
enum Mode {
    Fast,
    Slow,
}

#[derive(Debug, Deserialize, PartialEq)]
enum Action {
    Shell { run: String },
}

#[test]
fn serde_reads_string_and_single_key_map_enums() {
    let config: EnumConfig = yaml::from_str("mode: Fast\naction:\n  Shell:\n    run: echo ok\n")
        .expect("deserialize enum config");
    assert_eq!(
        config,
        EnumConfig {
            mode: Mode::Fast,
            action: Action::Shell {
                run: "echo ok".to_string()
            }
        }
    );
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BadReplicas {
    replicas: u32,
}

#[test]
fn serde_type_errors_include_scalar_span() {
    let error = yaml::from_str::<BadReplicas>("replicas: many\n").expect_err("type error");
    assert!(error.to_string().contains("expected unsigned integer"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 11);
    let location = error.location().expect("location");
    assert_eq!(location.index(), 10);
    assert_eq!(location.line(), 1);
    assert_eq!(location.column(), 11);
}
