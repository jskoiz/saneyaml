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
    peak_retained_objects: usize,
    checksum: usize,
}

#[derive(Clone, Copy, Default)]
struct Retained {
    bytes: usize,
    objects: usize,
}

impl Retained {
    fn heap_bytes(bytes: usize) -> Self {
        Self {
            bytes,
            objects: usize::from(bytes > 0),
        }
    }

    fn vec_capacity<T>(capacity: usize) -> Self {
        Self {
            bytes: capacity * mem::size_of::<T>(),
            objects: usize::from(capacity > 0),
        }
    }

    fn map_entries<T>(len: usize) -> Self {
        Self {
            bytes: len * mem::size_of::<T>(),
            objects: usize::from(len > 0),
        }
    }

    fn boxed<T>() -> Self {
        Self {
            bytes: mem::size_of::<T>(),
            objects: 1,
        }
    }

    fn peak(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.max(other.bytes),
            objects: self.objects.max(other.objects),
        }
    }
}

impl std::ops::Add for Retained {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            bytes: self.bytes + rhs.bytes,
            objects: self.objects + rhs.objects,
        }
    }
}

impl std::ops::AddAssign for Retained {
    fn add_assign(&mut self, rhs: Self) {
        self.bytes += rhs.bytes;
        self.objects += rhs.objects;
    }
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
            "| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |"
        );
        println!("|---|---:|---:|---:|---:|---:|---:|---:|");
        for result in [
            measure_yaml_parse_documents(corpus, iterations),
            measure_yaml_parse_borrowed_documents(corpus, iterations),
            measure_yaml_value(corpus, iterations),
            measure_serde_yaml_value(corpus, iterations),
            measure_yaml_rust2(corpus, iterations),
            measure_saphyr(corpus, iterations),
        ] {
            black_box(result.checksum);
            println!(
                "| {} | {} | {} | {} | {:.3} | {:.2} | {} | {} |",
                result.label,
                result.iterations,
                result.bytes_per_iteration,
                result.docs_per_iteration,
                result.elapsed.as_secs_f64() * 1000.0,
                ns_per_byte(&result),
                result.peak_retained_bytes,
                result.peak_retained_objects
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
        let owned = yaml::parse_documents(&fixture.input).expect(fixture.path);
        let borrowed = yaml::parse_borrowed_documents(&fixture.input).expect(fixture.path);
        assert_eq!(owned.len(), fixture.docs, "{} document count", fixture.path);
        assert_eq!(
            borrowed.len(),
            fixture.docs,
            "{} borrowed document count",
            fixture.path
        );
        for (index, (owned, borrowed)) in owned.iter().zip(&borrowed).enumerate() {
            let owned_value = yaml::Value::from(owned);
            let borrowed_value = borrowed.clone().into_owned_value();
            assert!(
                borrowed_value.equivalent(&owned_value),
                "{} borrowed document {index} differs from parse_documents",
                fixture.path
            );
        }
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

fn measure_yaml_parse_borrowed_documents(corpus: &Corpus<'_>, iterations: usize) -> BenchResult {
    measure(
        "yaml::parse_borrowed_documents",
        corpus,
        iterations,
        |input, path| yaml::parse_borrowed_documents(input).expect(path).len(),
        |input, path| {
            let docs = yaml::parse_borrowed_documents(input).expect(path);
            retained_yaml_borrowed_node_docs(&docs)
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
    M: FnMut(&str, &str) -> Retained,
{
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for fixture in &corpus.fixtures {
            checksum ^= black_box(run(black_box(&fixture.input), fixture.path));
        }
    }
    let elapsed = start.elapsed();
    let peak_retained = corpus
        .fixtures
        .iter()
        .map(|fixture| retained(&fixture.input, fixture.path))
        .fold(Retained::default(), Retained::peak);

    BenchResult {
        label,
        iterations,
        bytes_per_iteration: corpus.bytes_per_iteration(),
        docs_per_iteration: corpus.docs_per_iteration(),
        elapsed,
        peak_retained_bytes: peak_retained.bytes,
        peak_retained_objects: peak_retained.objects,
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

fn retained_yaml_node_docs(docs: &Vec<yaml::Node>) -> Retained {
    let mut retained = Retained::vec_capacity::<yaml::Node>(docs.capacity());
    for doc in docs {
        retained += retained_yaml_node(doc);
    }
    retained
}

fn retained_yaml_node(node: &yaml::Node) -> Retained {
    let mut retained = node
        .scalar_source()
        .map(|source| Retained::heap_bytes(source.raw().len()))
        .unwrap_or_default();
    retained += match &node.value {
        yaml::NodeValue::Null | yaml::NodeValue::Bool(_) | yaml::NodeValue::Number(_) => {
            Retained::default()
        }
        yaml::NodeValue::String(value) => Retained::heap_bytes(value.capacity()),
        yaml::NodeValue::Sequence(items) => {
            let mut retained = Retained::vec_capacity::<yaml::Node>(items.capacity());
            for item in items {
                retained += retained_yaml_node(item);
            }
            retained
        }
        yaml::NodeValue::Mapping(entries) => {
            let mut retained =
                Retained::vec_capacity::<(yaml::Node, yaml::Node)>(entries.capacity());
            for (key, value) in entries {
                retained += retained_yaml_node(key) + retained_yaml_node(value);
            }
            retained
        }
        yaml::NodeValue::Tagged(tagged) => {
            Retained::boxed::<yaml::TaggedNode>()
                + retained_yaml_tag(&tagged.tag)
                + retained_yaml_node(&tagged.value)
        }
    };
    retained
}

fn retained_yaml_borrowed_node_docs(docs: &Vec<yaml::BorrowedNode<'_>>) -> Retained {
    let mut retained = Retained::vec_capacity::<yaml::BorrowedNode<'_>>(docs.capacity());
    for doc in docs {
        retained += retained_yaml_borrowed_node(doc);
    }
    retained
}

fn retained_yaml_borrowed_node(node: &yaml::BorrowedNode<'_>) -> Retained {
    match &node.value {
        yaml::BorrowedNodeValue::Null
        | yaml::BorrowedNodeValue::Bool(_)
        | yaml::BorrowedNodeValue::Number(_) => Retained::default(),
        yaml::BorrowedNodeValue::String(value) => match value {
            Cow::Borrowed(_) => Retained::default(),
            Cow::Owned(value) => Retained::heap_bytes(value.capacity()),
        },
        yaml::BorrowedNodeValue::Sequence(items) => {
            let mut retained = Retained::vec_capacity::<yaml::BorrowedNode<'_>>(items.capacity());
            for item in items {
                retained += retained_yaml_borrowed_node(item);
            }
            retained
        }
        yaml::BorrowedNodeValue::Mapping(entries) => {
            let mut retained = Retained::vec_capacity::<(
                yaml::BorrowedNode<'_>,
                yaml::BorrowedNode<'_>,
            )>(entries.capacity());
            for (key, value) in entries {
                retained += retained_yaml_borrowed_node(key) + retained_yaml_borrowed_node(value);
            }
            retained
        }
        yaml::BorrowedNodeValue::Tagged(tagged) => {
            Retained::boxed::<yaml::BorrowedTaggedNode<'_>>()
                + retained_yaml_tag(&tagged.tag)
                + retained_yaml_borrowed_node(&tagged.value)
        }
    }
}

fn retained_yaml_value_docs(docs: &Vec<yaml::Value>) -> Retained {
    let mut retained = Retained::vec_capacity::<yaml::Value>(docs.capacity());
    for doc in docs {
        retained += retained_yaml_value(doc);
    }
    retained
}

fn retained_yaml_value(value: &yaml::Value) -> Retained {
    match value {
        yaml::Value::Null | yaml::Value::Bool(_) | yaml::Value::Number(_) => Retained::default(),
        yaml::Value::String(value) => Retained::heap_bytes(value.capacity()),
        yaml::Value::Sequence(items) => {
            let mut retained = Retained::vec_capacity::<yaml::Value>(items.capacity());
            for item in items {
                retained += retained_yaml_value(item);
            }
            retained
        }
        yaml::Value::Mapping(mapping) => {
            let mut retained =
                Retained::vec_capacity::<(yaml::Value, yaml::Value)>(mapping.capacity());
            for (key, value) in mapping {
                retained += retained_yaml_value(key) + retained_yaml_value(value);
            }
            retained
        }
        yaml::Value::Tagged(tagged) => {
            Retained::boxed::<yaml::TaggedValue>()
                + retained_yaml_tag(&tagged.tag)
                + retained_yaml_value(&tagged.value)
        }
    }
}

fn retained_yaml_tag(tag: &yaml::Tag) -> Retained {
    Retained::heap_bytes(tag.handle.capacity()) + Retained::heap_bytes(tag.suffix.capacity())
}

fn retained_serde_yaml_docs(docs: &Vec<serde_yaml::Value>) -> Retained {
    let mut retained = Retained::vec_capacity::<serde_yaml::Value>(docs.capacity());
    for doc in docs {
        retained += retained_serde_yaml_value(doc);
    }
    retained
}

fn retained_serde_yaml_value(value: &serde_yaml::Value) -> Retained {
    match value {
        serde_yaml::Value::Null | serde_yaml::Value::Bool(_) | serde_yaml::Value::Number(_) => {
            Retained::default()
        }
        serde_yaml::Value::String(value) => Retained::heap_bytes(value.capacity()),
        serde_yaml::Value::Sequence(items) => {
            let mut retained = Retained::vec_capacity::<serde_yaml::Value>(items.capacity());
            for item in items {
                retained += retained_serde_yaml_value(item);
            }
            retained
        }
        serde_yaml::Value::Mapping(mapping) => {
            let mut retained =
                Retained::map_entries::<(serde_yaml::Value, serde_yaml::Value)>(mapping.len());
            for (key, value) in mapping {
                retained += retained_serde_yaml_value(key) + retained_serde_yaml_value(value);
            }
            retained
        }
        serde_yaml::Value::Tagged(tagged) => {
            let tag_len = tagged.tag.to_string().len();
            Retained::boxed::<serde_yaml::value::TaggedValue>()
                + Retained::heap_bytes(tag_len)
                + retained_serde_yaml_value(&tagged.value)
        }
    }
}

fn retained_yaml_rust2_docs(docs: &Vec<yaml_rust2::Yaml>) -> Retained {
    let mut retained = Retained::vec_capacity::<yaml_rust2::Yaml>(docs.capacity());
    for doc in docs {
        retained += retained_yaml_rust2(doc);
    }
    retained
}

fn retained_yaml_rust2(value: &yaml_rust2::Yaml) -> Retained {
    match value {
        yaml_rust2::Yaml::Real(value) | yaml_rust2::Yaml::String(value) => {
            Retained::heap_bytes(value.capacity())
        }
        yaml_rust2::Yaml::Array(items) => {
            let mut retained = Retained::vec_capacity::<yaml_rust2::Yaml>(items.capacity());
            for item in items {
                retained += retained_yaml_rust2(item);
            }
            retained
        }
        yaml_rust2::Yaml::Hash(mapping) => {
            let mut retained =
                Retained::map_entries::<(yaml_rust2::Yaml, yaml_rust2::Yaml)>(mapping.len());
            for (key, value) in mapping {
                retained += retained_yaml_rust2(key) + retained_yaml_rust2(value);
            }
            retained
        }
        yaml_rust2::Yaml::Integer(_)
        | yaml_rust2::Yaml::Boolean(_)
        | yaml_rust2::Yaml::Alias(_)
        | yaml_rust2::Yaml::Null
        | yaml_rust2::Yaml::BadValue => Retained::default(),
    }
}

fn retained_saphyr_docs(docs: &Vec<saphyr::Yaml<'_>>) -> Retained {
    let mut retained = Retained::vec_capacity::<saphyr::Yaml<'_>>(docs.capacity());
    for doc in docs {
        retained += retained_saphyr(doc);
    }
    retained
}

fn retained_saphyr(value: &saphyr::Yaml<'_>) -> Retained {
    match value {
        saphyr::Yaml::Representation(text, _, tag) => {
            let text_retained = match text {
                Cow::Borrowed(_) => Retained::default(),
                Cow::Owned(value) => Retained::heap_bytes(value.capacity()),
            };
            let tag_retained = tag
                .as_ref()
                .map(|tag| match tag {
                    Cow::Borrowed(_) => Retained::default(),
                    Cow::Owned(tag) => {
                        Retained::heap_bytes(tag.handle.capacity())
                            + Retained::heap_bytes(tag.suffix.capacity())
                    }
                })
                .unwrap_or_default();
            text_retained + tag_retained
        }
        saphyr::Yaml::Value(scalar) => retained_saphyr_scalar(scalar),
        saphyr::Yaml::Sequence(items) => {
            let mut retained = Retained::vec_capacity::<saphyr::Yaml<'_>>(items.capacity());
            for item in items {
                retained += retained_saphyr(item);
            }
            retained
        }
        saphyr::Yaml::Mapping(mapping) => {
            let mut retained =
                Retained::map_entries::<(saphyr::Yaml<'_>, saphyr::Yaml<'_>)>(mapping.len());
            for (key, value) in mapping {
                retained += retained_saphyr(key) + retained_saphyr(value);
            }
            retained
        }
        saphyr::Yaml::Tagged(tag, value) => {
            let tag_retained = match tag {
                Cow::Borrowed(_) => Retained::default(),
                Cow::Owned(tag) => {
                    Retained::heap_bytes(tag.handle.capacity())
                        + Retained::heap_bytes(tag.suffix.capacity())
                }
            };
            Retained::boxed::<saphyr::Yaml<'_>>() + tag_retained + retained_saphyr(value)
        }
        saphyr::Yaml::Alias(_) | saphyr::Yaml::BadValue => Retained::default(),
    }
}

fn retained_saphyr_scalar(scalar: &saphyr::Scalar<'_>) -> Retained {
    match scalar {
        saphyr::Scalar::String(value) => match value {
            Cow::Borrowed(_) => Retained::default(),
            Cow::Owned(value) => Retained::heap_bytes(value.capacity()),
        },
        saphyr::Scalar::Null
        | saphyr::Scalar::Boolean(_)
        | saphyr::Scalar::Integer(_)
        | saphyr::Scalar::FloatingPoint(_) => Retained::default(),
    }
}
