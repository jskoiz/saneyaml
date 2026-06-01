use saphyr::LoadableYamlNode;
use serde::Deserialize;
use std::borrow::Cow;
use std::hint::black_box;
use std::mem;
use std::time::{Duration, Instant};

struct Fixture<'a> {
    path: &'static str,
    input: Cow<'a, str>,
    docs: usize,
}

struct Corpus<'a> {
    label: &'static str,
    fixtures: Vec<Fixture<'a>>,
}

impl Corpus<'_> {
    fn bytes_per_iteration(&self) -> usize {
        self.fixtures
            .iter()
            .map(|fixture| fixture.input.len())
            .sum()
    }

    fn docs_per_iteration(&self) -> usize {
        self.fixtures.iter().map(|fixture| fixture.docs).sum()
    }
}

struct BenchResult {
    label: &'static str,
    iterations: usize,
    bytes_per_iteration: usize,
    docs_per_iteration: usize,
    elapsed: Duration,
    peak_retained_bytes: usize,
    checksum: usize,
}

const DOWNSTREAM_FIXTURES: &[Fixture<'static>] = &[
    Fixture {
        path: "pingora/pingora-core-pingora_conf.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/pingora/pingora-core-pingora_conf.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "pingora/pingora-proxy-pingora_conf.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/pingora/pingora-proxy-pingora_conf.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "pingora/pingora-proxy-example-conf.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/pingora/pingora-proxy-example-conf.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "rust-i18n/app-en.yml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/rust-i18n/app-en.yml"
        )),
        docs: 1,
    },
    Fixture {
        path: "rust-i18n/app-fr.yml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/rust-i18n/app-fr.yml"
        )),
        docs: 1,
    },
    Fixture {
        path: "rust-i18n/user.en.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/rust-i18n/user.en.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "rust-i18n/v2.yml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/rust-i18n/v2.yml"
        )),
        docs: 1,
    },
    Fixture {
        path: "cfn-guard/cfn-lambda.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/cfn-guard/cfn-lambda.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "cfn-guard/test-command-test.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/cfn-guard/test-command-test.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "cfn-guard/s3-bucket-logging-enabled-tests.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/cfn-guard/s3-bucket-logging-enabled-tests.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "navi/config-example.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/navi/config-example.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "navi/tests-config.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/navi/tests-config.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/AuthenticationClass.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/AuthenticationClass.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/DummyCluster.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/DummyCluster.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/Listener.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/Listener.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/ListenerClass.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/ListenerClass.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/PodListeners.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/PodListeners.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/S3Bucket.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/S3Bucket.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/S3Connection.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/S3Connection.yaml"
        )),
        docs: 1,
    },
    Fixture {
        path: "stackable-operator/Scaler.yaml",
        input: Cow::Borrowed(include_str!(
            "../tests/fixtures/downstream/stackable-operator/Scaler.yaml"
        )),
        docs: 1,
    },
];

fn main() {
    let iterations = std::env::var("YAML_LARGE_BENCH_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);

    let (stream_1mib, stream_docs) = generated_multi_doc_stream(1024 * 1024);
    let wide_256kib = generated_wide_mapping(256 * 1024);
    let wide_1mib = generated_wide_mapping(1024 * 1024);

    let corpora = vec![
        Corpus {
            label: "external_downstream_all",
            fixtures: downstream_fixtures(),
        },
        Corpus {
            label: "stackable_dummy_cluster",
            fixtures: vec![Fixture {
                path: "stackable-operator/DummyCluster.yaml",
                input: Cow::Borrowed(include_str!(
                    "../tests/fixtures/downstream/stackable-operator/DummyCluster.yaml"
                )),
                docs: 1,
            }],
        },
        Corpus {
            label: "generated_multi_doc_stream_1mib",
            fixtures: vec![Fixture {
                path: "generated/multi-doc-stream-1mib.yaml",
                input: Cow::Owned(stream_1mib),
                docs: stream_docs,
            }],
        },
        Corpus {
            label: "generated_wide_mapping_256kib",
            fixtures: vec![Fixture {
                path: "generated/wide-mapping-256kib.yaml",
                input: Cow::Owned(wide_256kib),
                docs: 1,
            }],
        },
        Corpus {
            label: "generated_wide_mapping_1mib",
            fixtures: vec![Fixture {
                path: "generated/wide-mapping-1mib.yaml",
                input: Cow::Owned(wide_1mib),
                docs: 1,
            }],
        },
    ];

    for corpus in &corpora {
        validate_corpus(corpus);
        println!("\n## {}", corpus.label);
        println!(
            "| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |"
        );
        println!("|---|---:|---:|---:|---:|---:|---:|");
        for result in [
            measure_yaml_parse_documents(corpus, iterations),
            measure_yaml_value(corpus, iterations),
            measure_serde_yaml_value(corpus, iterations),
            measure_yaml_rust2(corpus, iterations),
            measure_saphyr(corpus, iterations),
        ] {
            black_box(result.checksum);
            println!(
                "| {} | {} | {} | {} | {:.3} | {:.2} | {} |",
                result.label,
                result.iterations,
                result.bytes_per_iteration,
                result.docs_per_iteration,
                result.elapsed.as_secs_f64() * 1000.0,
                ns_per_byte(&result),
                result.peak_retained_bytes
            );
        }
    }
}

fn downstream_fixtures() -> Vec<Fixture<'static>> {
    DOWNSTREAM_FIXTURES
        .iter()
        .map(|fixture| Fixture {
            path: fixture.path,
            input: Cow::Borrowed(fixture.input.as_ref()),
            docs: fixture.docs,
        })
        .collect()
}

fn validate_corpus(corpus: &Corpus<'_>) {
    for fixture in &corpus.fixtures {
        assert_eq!(
            yaml::parse_documents(&fixture.input)
                .expect(fixture.path)
                .len(),
            fixture.docs,
            "{} document count",
            fixture.path
        );
    }
}

fn measure_yaml_parse_documents(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "yaml::parse_documents",
        corpus,
        iterations,
        |input, path| yaml::parse_documents(input).expect(path).len(),
        |input, path| {
            let docs = yaml::parse_documents(input).expect(path);
            retained_yaml_node_docs(&docs)
        },
    )
}

fn measure_yaml_value(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "yaml::from_documents_str::<Value>",
        corpus,
        iterations,
        |input, path| {
            yaml::from_documents_str::<yaml::Value>(input)
                .expect(path)
                .len()
        },
        |input, path| {
            let docs = yaml::from_documents_str::<yaml::Value>(input).expect(path);
            retained_yaml_value_docs(&docs)
        },
    )
}

fn measure_serde_yaml_value(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "serde_yaml::Value stream",
        corpus,
        iterations,
        |input, path| {
            serde_yaml::Deserializer::from_str(input)
                .map(serde_yaml::Value::deserialize)
                .collect::<Result<Vec<_>, _>>()
                .expect(path)
                .len()
        },
        |input, path| {
            let docs = serde_yaml::Deserializer::from_str(input)
                .map(serde_yaml::Value::deserialize)
                .collect::<Result<Vec<_>, _>>()
                .expect(path);
            retained_serde_yaml_docs(&docs)
        },
    )
}

fn measure_yaml_rust2(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "yaml_rust2::YamlLoader",
        corpus,
        iterations,
        |input, path| {
            yaml_rust2::YamlLoader::load_from_str(input)
                .expect(path)
                .len()
        },
        |input, path| {
            let docs = yaml_rust2::YamlLoader::load_from_str(input).expect(path);
            retained_yaml_rust2_docs(&docs)
        },
    )
}

fn measure_saphyr(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "saphyr::Yaml::load_from_str",
        corpus,
        iterations,
        |input, path| saphyr::Yaml::load_from_str(input).expect(path).len(),
        |input, path| {
            let docs = saphyr::Yaml::load_from_str(input).expect(path);
            retained_saphyr_docs(&docs)
        },
    )
}

fn measure<R, M>(
    label: &'static str,
    corpus: &Corpus<'_>,
    iterations: usize,
    mut run: R,
    mut retained: M,
) -> BenchResult
where
    R: FnMut(&str, &str) -> usize,
    M: FnMut(&str, &str) -> usize,
{
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for fixture in &corpus.fixtures {
            checksum ^= black_box(run(black_box(&fixture.input), fixture.path));
        }
    }
    let elapsed = start.elapsed();
    let peak_retained_bytes = corpus
        .fixtures
        .iter()
        .map(|fixture| retained(&fixture.input, fixture.path))
        .max()
        .unwrap_or(0);

    BenchResult {
        label,
        iterations,
        bytes_per_iteration: corpus.bytes_per_iteration(),
        docs_per_iteration: corpus.docs_per_iteration(),
        elapsed,
        peak_retained_bytes,
        checksum,
    }
}

fn ns_per_byte(result: &BenchResult) -> f64 {
    let bytes = result.iterations * result.bytes_per_iteration;
    result.elapsed.as_nanos() as f64 / bytes as f64
}

fn generated_multi_doc_stream(target_bytes: usize) -> (String, usize) {
    let mut input = String::with_capacity(target_bytes + 256);
    let mut docs = 0usize;
    while input.len() < target_bytes {
        input.push_str("---\nservice:\n  name: app-");
        input.push_str(&docs.to_string());
        input.push_str("\n  image: ghcr.io/example/app:");
        input.push_str(&(docs % 97).to_string());
        input.push_str("\n  ports:\n    - ");
        input.push_str(&(8000 + docs % 1000).to_string());
        input.push_str("\n  env:\n    RUST_LOG: info\n    FEATURE_FLAG: true\n");
        docs += 1;
    }
    (input, docs)
}

fn generated_wide_mapping(target_bytes: usize) -> String {
    let mut input = String::with_capacity(target_bytes + 256);
    input.push_str("services:\n");
    let mut idx = 0usize;
    while input.len() < target_bytes {
        input.push_str("  service-");
        input.push_str(&idx.to_string());
        input.push_str(":\n    image: ghcr.io/example/service:");
        input.push_str(&(idx % 113).to_string());
        input.push_str("\n    replicas: ");
        input.push_str(&(1 + idx % 9).to_string());
        input.push_str("\n    enabled: true\n");
        idx += 1;
    }
    input
}

fn retained_yaml_node_docs(docs: &Vec<yaml::Node>) -> usize {
    docs.capacity() * mem::size_of::<yaml::Node>()
        + docs.iter().map(retained_yaml_node).sum::<usize>()
}

fn retained_yaml_node(node: &yaml::Node) -> usize {
    node.scalar_source()
        .map(|source| source.raw().len())
        .unwrap_or(0)
        + match &node.value {
            yaml::NodeValue::Null | yaml::NodeValue::Bool(_) | yaml::NodeValue::Number(_) => 0,
            yaml::NodeValue::String(value) => value.capacity(),
            yaml::NodeValue::Sequence(items) => {
                items.capacity() * mem::size_of::<yaml::Node>()
                    + items.iter().map(retained_yaml_node).sum::<usize>()
            }
            yaml::NodeValue::Mapping(entries) => {
                entries.capacity() * mem::size_of::<(yaml::Node, yaml::Node)>()
                    + entries
                        .iter()
                        .map(|(key, value)| retained_yaml_node(key) + retained_yaml_node(value))
                        .sum::<usize>()
            }
            yaml::NodeValue::Tagged(tagged) => {
                mem::size_of::<yaml::TaggedNode>()
                    + retained_yaml_tag(&tagged.tag)
                    + retained_yaml_node(&tagged.value)
            }
        }
}

fn retained_yaml_value_docs(docs: &Vec<yaml::Value>) -> usize {
    docs.capacity() * mem::size_of::<yaml::Value>()
        + docs.iter().map(retained_yaml_value).sum::<usize>()
}

fn retained_yaml_value(value: &yaml::Value) -> usize {
    match value {
        yaml::Value::Null | yaml::Value::Bool(_) | yaml::Value::Number(_) => 0,
        yaml::Value::String(value) => value.capacity(),
        yaml::Value::Sequence(items) => {
            items.capacity() * mem::size_of::<yaml::Value>()
                + items.iter().map(retained_yaml_value).sum::<usize>()
        }
        yaml::Value::Mapping(mapping) => {
            mapping.capacity() * mem::size_of::<(yaml::Value, yaml::Value)>()
                + mapping
                    .iter()
                    .map(|(key, value)| retained_yaml_value(key) + retained_yaml_value(value))
                    .sum::<usize>()
        }
        yaml::Value::Tagged(tagged) => {
            mem::size_of::<yaml::TaggedValue>()
                + retained_yaml_tag(&tagged.tag)
                + retained_yaml_value(&tagged.value)
        }
    }
}

fn retained_yaml_tag(tag: &yaml::Tag) -> usize {
    tag.handle.capacity() + tag.suffix.capacity()
}

fn retained_serde_yaml_docs(docs: &Vec<serde_yaml::Value>) -> usize {
    docs.capacity() * mem::size_of::<serde_yaml::Value>()
        + docs.iter().map(retained_serde_yaml_value).sum::<usize>()
}

fn retained_serde_yaml_value(value: &serde_yaml::Value) -> usize {
    match value {
        serde_yaml::Value::Null | serde_yaml::Value::Bool(_) | serde_yaml::Value::Number(_) => 0,
        serde_yaml::Value::String(value) => value.capacity(),
        serde_yaml::Value::Sequence(items) => {
            items.capacity() * mem::size_of::<serde_yaml::Value>()
                + items.iter().map(retained_serde_yaml_value).sum::<usize>()
        }
        serde_yaml::Value::Mapping(mapping) => {
            mapping.len() * mem::size_of::<(serde_yaml::Value, serde_yaml::Value)>()
                + mapping
                    .iter()
                    .map(|(key, value)| {
                        retained_serde_yaml_value(key) + retained_serde_yaml_value(value)
                    })
                    .sum::<usize>()
        }
        serde_yaml::Value::Tagged(tagged) => {
            mem::size_of::<serde_yaml::value::TaggedValue>()
                + tagged.tag.to_string().len()
                + retained_serde_yaml_value(&tagged.value)
        }
    }
}

fn retained_yaml_rust2_docs(docs: &Vec<yaml_rust2::Yaml>) -> usize {
    docs.capacity() * mem::size_of::<yaml_rust2::Yaml>()
        + docs.iter().map(retained_yaml_rust2).sum::<usize>()
}

fn retained_yaml_rust2(value: &yaml_rust2::Yaml) -> usize {
    match value {
        yaml_rust2::Yaml::Real(value) | yaml_rust2::Yaml::String(value) => value.capacity(),
        yaml_rust2::Yaml::Array(items) => {
            items.capacity() * mem::size_of::<yaml_rust2::Yaml>()
                + items.iter().map(retained_yaml_rust2).sum::<usize>()
        }
        yaml_rust2::Yaml::Hash(mapping) => {
            mapping.len() * mem::size_of::<(yaml_rust2::Yaml, yaml_rust2::Yaml)>()
                + mapping
                    .iter()
                    .map(|(key, value)| retained_yaml_rust2(key) + retained_yaml_rust2(value))
                    .sum::<usize>()
        }
        yaml_rust2::Yaml::Integer(_)
        | yaml_rust2::Yaml::Boolean(_)
        | yaml_rust2::Yaml::Alias(_)
        | yaml_rust2::Yaml::Null
        | yaml_rust2::Yaml::BadValue => 0,
    }
}

fn retained_saphyr_docs(docs: &Vec<saphyr::Yaml<'_>>) -> usize {
    docs.capacity() * mem::size_of::<saphyr::Yaml<'_>>()
        + docs.iter().map(retained_saphyr).sum::<usize>()
}

fn retained_saphyr(value: &saphyr::Yaml<'_>) -> usize {
    match value {
        saphyr::Yaml::Representation(text, _, tag) => {
            let text_bytes = match text {
                Cow::Borrowed(_) => 0,
                Cow::Owned(value) => value.capacity(),
            };
            let tag_bytes = tag
                .as_ref()
                .map(|tag| match tag {
                    Cow::Borrowed(_) => 0,
                    Cow::Owned(tag) => tag.handle.capacity() + tag.suffix.capacity(),
                })
                .unwrap_or(0);
            text_bytes + tag_bytes
        }
        saphyr::Yaml::Value(scalar) => retained_saphyr_scalar(scalar),
        saphyr::Yaml::Sequence(items) => {
            items.capacity() * mem::size_of::<saphyr::Yaml<'_>>()
                + items.iter().map(retained_saphyr).sum::<usize>()
        }
        saphyr::Yaml::Mapping(mapping) => {
            mapping.len() * mem::size_of::<(saphyr::Yaml<'_>, saphyr::Yaml<'_>)>()
                + mapping
                    .iter()
                    .map(|(key, value)| retained_saphyr(key) + retained_saphyr(value))
                    .sum::<usize>()
        }
        saphyr::Yaml::Tagged(tag, value) => {
            let tag_bytes = match tag {
                Cow::Borrowed(_) => 0,
                Cow::Owned(tag) => tag.handle.capacity() + tag.suffix.capacity(),
            };
            mem::size_of::<saphyr::Yaml<'_>>() + tag_bytes + retained_saphyr(value)
        }
        saphyr::Yaml::Alias(_) | saphyr::Yaml::BadValue => 0,
    }
}

fn retained_saphyr_scalar(scalar: &saphyr::Scalar<'_>) -> usize {
    match scalar {
        saphyr::Scalar::String(value) => match value {
            Cow::Borrowed(_) => 0,
            Cow::Owned(value) => value.capacity(),
        },
        saphyr::Scalar::Null
        | saphyr::Scalar::Boolean(_)
        | saphyr::Scalar::Integer(_)
        | saphyr::Scalar::FloatingPoint(_) => 0,
    }
}
