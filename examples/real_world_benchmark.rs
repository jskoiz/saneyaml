use saphyr::LoadableYamlNode;
use serde::Deserialize;
use std::hint::black_box;
use std::time::{Duration, Instant};

struct Fixture {
    path: &'static str,
    input: &'static str,
    docs: usize,
}

struct BenchResult {
    label: &'static str,
    iterations: usize,
    bytes_per_iteration: usize,
    docs_per_iteration: usize,
    elapsed: Duration,
    checksum: usize,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        path: "github-actions/minimal-ci.yaml",
        input: include_str!("../tests/fixtures/real-world/github-actions/minimal-ci.yaml"),
        docs: 1,
    },
    Fixture {
        path: "github-actions/matrix-ci.yaml",
        input: include_str!("../tests/fixtures/real-world/github-actions/matrix-ci.yaml"),
        docs: 1,
    },
    Fixture {
        path: "github-actions/starter-node-ci.yml",
        input: include_str!("../tests/fixtures/real-world/github-actions/starter-node-ci.yml"),
        docs: 1,
    },
    Fixture {
        path: "github-actions/polymorphic-workflow.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/github-actions/polymorphic-workflow.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "github-actions/reusable-service-workflow.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/github-actions/reusable-service-workflow.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/compose.yaml",
        input: include_str!("../tests/fixtures/real-world/docker-compose/compose.yaml"),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/awesome-nginx-flask-mysql.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/compose-anchors.yaml",
        input: include_str!("../tests/fixtures/real-world/docker-compose/compose-anchors.yaml"),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/compose-platform-resources.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/docker-compose/compose-platform-resources.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/compose-polymorphic.yaml",
        input: include_str!("../tests/fixtures/real-world/docker-compose/compose-polymorphic.yaml"),
        docs: 1,
    },
    Fixture {
        path: "docker-compose/adapted-compose-spec-fragments.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/docker-compose/adapted-compose-spec-fragments.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "kubernetes/deployment.yaml",
        input: include_str!("../tests/fixtures/real-world/kubernetes/deployment.yaml"),
        docs: 1,
    },
    Fixture {
        path: "kubernetes/multi-doc.yaml",
        input: include_str!("../tests/fixtures/real-world/kubernetes/multi-doc.yaml"),
        docs: 2,
    },
    Fixture {
        path: "kubernetes/custom-resource-definition.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/kubernetes/custom-resource-definition.yaml"
        ),
        docs: 2,
    },
    Fixture {
        path: "kubernetes/helm-rendered-stream.yaml",
        input: include_str!("../tests/fixtures/real-world/kubernetes/helm-rendered-stream.yaml"),
        docs: 5,
    },
    Fixture {
        path: "kubernetes/configmap-block-scalars.yaml",
        input: include_str!("../tests/fixtures/real-world/kubernetes/configmap-block-scalars.yaml"),
        docs: 1,
    },
    Fixture {
        path: "kubernetes/upstream-guestbook-frontend-deployment.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/kubernetes/upstream-guestbook-frontend-deployment.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "helm/values.yaml",
        input: include_str!("../tests/fixtures/real-world/helm/values.yaml"),
        docs: 1,
    },
    Fixture {
        path: "helm/Chart.yaml",
        input: include_str!("../tests/fixtures/real-world/helm/Chart.yaml"),
        docs: 1,
    },
    Fixture {
        path: "helm/upstream-hello-world-Chart.yaml",
        input: include_str!("../tests/fixtures/real-world/helm/upstream-hello-world-Chart.yaml"),
        docs: 1,
    },
    Fixture {
        path: "openapi/petstore-fragment.yaml",
        input: include_str!("../tests/fixtures/real-world/openapi/petstore-fragment.yaml"),
        docs: 1,
    },
    Fixture {
        path: "openapi/operations-and-polymorphism.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/openapi/operations-and-polymorphism.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "openapi/upstream-petstore.yaml",
        input: include_str!("../tests/fixtures/real-world/openapi/upstream-petstore.yaml"),
        docs: 1,
    },
    Fixture {
        path: "cloudflare/wrangler.yaml",
        input: include_str!("../tests/fixtures/real-world/cloudflare/wrangler.yaml"),
        docs: 1,
    },
    Fixture {
        path: "cloudflare/adapted-durable-objects-wrangler.yaml",
        input: include_str!(
            "../tests/fixtures/real-world/cloudflare/adapted-durable-objects-wrangler.yaml"
        ),
        docs: 1,
    },
    Fixture {
        path: "cloudformation/sam-api.yaml",
        input: include_str!("../tests/fixtures/real-world/cloudformation/sam-api.yaml"),
        docs: 1,
    },
    Fixture {
        path: "symfony/services.yaml",
        input: include_str!("../tests/fixtures/real-world/symfony/services.yaml"),
        docs: 1,
    },
    Fixture {
        path: "gitlab-ci/basic-pipeline.yml",
        input: include_str!("../tests/fixtures/real-world/gitlab-ci/basic-pipeline.yml"),
        docs: 1,
    },
    Fixture {
        path: "circleci/config.yml",
        input: include_str!("../tests/fixtures/real-world/circleci/config.yml"),
        docs: 1,
    },
    Fixture {
        path: "azure-pipelines/azure-pipelines.yml",
        input: include_str!("../tests/fixtures/real-world/azure-pipelines/azure-pipelines.yml"),
        docs: 1,
    },
    Fixture {
        path: "ansible/playbook.yaml",
        input: include_str!("../tests/fixtures/real-world/ansible/playbook.yaml"),
        docs: 1,
    },
    Fixture {
        path: "ansible/upstream-lamp-simple-site.yml",
        input: include_str!("../tests/fixtures/real-world/ansible/upstream-lamp-simple-site.yml"),
        docs: 1,
    },
    Fixture {
        path: "ansible/vault-and-unsafe-tags.yaml",
        input: include_str!("../tests/fixtures/real-world/ansible/vault-and-unsafe-tags.yaml"),
        docs: 1,
    },
];

fn main() {
    let iterations = std::env::var("YAML_BENCH_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    let bytes_per_iteration = FIXTURES.iter().map(|fixture| fixture.input.len()).sum();
    let docs_per_iteration = FIXTURES.iter().map(|fixture| fixture.docs).sum();

    for fixture in FIXTURES {
        assert_eq!(
            yaml::parse_documents(fixture.input)
                .expect(fixture.path)
                .len(),
            fixture.docs,
            "{} document count",
            fixture.path
        );
    }

    let results = [
        measure(
            "yaml::parse_documents",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        yaml::parse_documents(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "yaml::from_documents_str::<Value>",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        yaml::from_documents_str::<yaml::Value>(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "serde_yaml::Value stream",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        serde_yaml::Deserializer::from_str(fixture.input)
                            .map(serde_yaml::Value::deserialize)
                            .collect::<Result<Vec<_>, _>>()
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "yaml_rust2::YamlLoader",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        yaml_rust2::YamlLoader::load_from_str(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "saphyr::Yaml::load_from_str",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        saphyr::Yaml::load_from_str(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
    ];

    println!(
        "| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for result in results {
        black_box(result.checksum);
        println!(
            "| {} | {} | {} | {} | {:.3} | {:.2} |",
            result.label,
            result.iterations,
            result.bytes_per_iteration,
            result.docs_per_iteration,
            result.elapsed.as_secs_f64() * 1000.0,
            ns_per_byte(&result)
        );
    }
}

fn measure<F>(
    label: &'static str,
    iterations: usize,
    bytes_per_iteration: usize,
    docs_per_iteration: usize,
    mut run: F,
) -> BenchResult
where
    F: FnMut() -> usize,
{
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum ^= black_box(run());
    }
    BenchResult {
        label,
        iterations,
        bytes_per_iteration,
        docs_per_iteration,
        elapsed: start.elapsed(),
        checksum,
    }
}

fn ns_per_byte(result: &BenchResult) -> f64 {
    let bytes = result.iterations * result.bytes_per_iteration;
    result.elapsed.as_nanos() as f64 / bytes as f64
}
