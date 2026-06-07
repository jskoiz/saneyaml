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
    let serde_saphyr_options = serde_saphyr_benchmark_options();

    for fixture in FIXTURES {
        assert_eq!(
            saneyaml::parse_documents(fixture.input)
                .expect(fixture.path)
                .len(),
            fixture.docs,
            "{} document count",
            fixture.path
        );
        let saneyaml_serde_yaml_docs =
            saneyaml::from_documents_str::<serde_yaml::Value>(fixture.input).expect(fixture.path);
        let saneyaml_event_serde_yaml_docs =
            saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>(
                fixture.input,
            )
            .expect(fixture.path);
        let serde_saphyr_serde_yaml_docs =
            serde_saphyr_serde_yaml_documents(fixture.input, fixture.path, &serde_saphyr_options);
        assert_eq!(
            saneyaml_serde_yaml_docs.len(),
            fixture.docs,
            "{} saneyaml serde_yaml::Value document count",
            fixture.path
        );
        assert_eq!(
            saneyaml_event_serde_yaml_docs.len(),
            fixture.docs,
            "{} saneyaml event-backed serde_yaml::Value document count",
            fixture.path
        );
        assert_eq!(
            saneyaml_event_serde_yaml_docs, saneyaml_serde_yaml_docs,
            "{} event-backed serde_yaml::Value document shape",
            fixture.path
        );
        assert_eq!(
            serde_saphyr_serde_yaml_docs.len(),
            serde_saphyr_comparable_documents(&saneyaml_serde_yaml_docs).len(),
            "{} serde-saphyr serde_yaml::Value document count",
            fixture.path
        );
        assert_eq!(
            serde_saphyr_serde_yaml_docs,
            serde_saphyr_comparable_documents(&saneyaml_serde_yaml_docs),
            "{} generic serde_yaml::Value document shape",
            fixture.path
        );
    }

    let results = [
        measure(
            "saneyaml::parse_documents",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        saneyaml::parse_documents(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "saneyaml::from_documents_str::<Value>",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        saneyaml::from_documents_str::<saneyaml::Value>(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "saneyaml::from_documents_str::<serde_yaml::Value>",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        saneyaml::from_documents_str::<serde_yaml::Value>(fixture.input)
                            .expect(fixture.path)
                            .len()
                    })
                    .sum()
            },
        ),
        measure(
            "saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>(
                            fixture.input,
                        )
                        .expect(fixture.path)
                        .len()
                    })
                    .sum()
            },
        ),
        measure(
            "serde_saphyr::from_multiple_with_options::<serde_yaml::Value>",
            iterations,
            bytes_per_iteration,
            docs_per_iteration,
            || {
                FIXTURES
                    .iter()
                    .map(|fixture| {
                        serde_saphyr_serde_yaml_documents(
                            fixture.input,
                            fixture.path,
                            &serde_saphyr_options,
                        )
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

fn serde_saphyr_serde_yaml_documents(
    input: &str,
    path: &str,
    options: &serde_saphyr::Options,
) -> Vec<serde_yaml::Value> {
    serde_saphyr::from_multiple_with_options::<serde_yaml::Value>(input, options.clone())
        .unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn serde_saphyr_comparable_documents(docs: &[serde_yaml::Value]) -> Vec<serde_yaml::Value> {
    // Match serde-saphyr's serde_yaml::Value contract: skip null docs and
    // treat tags as transparent.
    docs.iter()
        .filter(|doc| !doc.is_null())
        .map(strip_serde_yaml_tags)
        .collect()
}

fn strip_serde_yaml_tags(value: &serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::Sequence(items) => {
            serde_yaml::Value::Sequence(items.iter().map(strip_serde_yaml_tags).collect())
        }
        serde_yaml::Value::Mapping(mapping) => {
            let mut stripped = serde_yaml::Mapping::new();
            for (key, value) in mapping {
                stripped.insert(strip_serde_yaml_tags(key), strip_serde_yaml_tags(value));
            }
            serde_yaml::Value::Mapping(stripped)
        }
        serde_yaml::Value::Tagged(tagged) => strip_serde_yaml_tags(&tagged.value),
        value => value.clone(),
    }
}

fn serde_saphyr_benchmark_options() -> serde_saphyr::Options {
    let many = usize::MAX;
    serde_saphyr::options! {
        strict_booleans: true,
        budget: serde_saphyr::budget! {
            max_reader_input_bytes: None,
            max_events: many,
            max_aliases: many,
            max_anchors: many,
            max_depth: many,
            max_inclusion_depth: u32::MAX,
            max_documents: many,
            max_nodes: many,
            max_total_scalar_bytes: many,
            max_total_comment_bytes: many,
            max_merge_keys: many,
            enforce_alias_anchor_ratio: false,
            alias_anchor_min_aliases: many,
            alias_anchor_ratio_multiplier: many,
        },
    }
}
