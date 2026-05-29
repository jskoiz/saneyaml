use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use yaml::{CollectionStyle, LosslessNodeKind, ScalarStyle, parse_lossless};

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");
const SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    fixture: Vec<FixtureRecord>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    path: String,
    expected_docs: usize,
    gates: Vec<String>,
}

#[test]
fn real_world_lossless_replay_gate_is_manifest_owned() {
    let manifest = fixture_manifest();
    let lossless_paths = lossless_replay_paths(&manifest);
    assert_eq!(
        lossless_paths,
        BTreeSet::from([
            "ansible/vault-and-unsafe-tags.yaml",
            "cloudflare/wrangler.yaml",
            "docker-compose/awesome-nginx-flask-mysql.yaml",
            "github-actions/starter-node-ci.yml",
            "helm/upstream-hello-world-Chart.yaml",
            "kubernetes/configmap-block-scalars.yaml",
            "kubernetes/helm-rendered-stream.yaml",
            "openapi/operations-and-polymorphism.yaml",
        ])
    );

    for fixture in manifest
        .fixture
        .iter()
        .filter(|fixture| fixture.gates.iter().any(|gate| gate == "lossless-replay"))
    {
        let input = fs::read_to_string(Path::new(FIXTURE_ROOT).join(&fixture.path))
            .unwrap_or_else(|error| panic!("read real-world fixture {}: {error}", fixture.path));
        let stream = parse_lossless(&input)
            .unwrap_or_else(|error| panic!("lossless parse {}: {error}", fixture.path));

        assert_eq!(
            stream.as_source(),
            input,
            "source replay for {}",
            fixture.path
        );
        assert_eq!(
            stream.to_string(),
            input,
            "Display replay for {}",
            fixture.path
        );
        assert_eq!(
            stream.clone().into_source(),
            input,
            "owned source replay for {}",
            fixture.path
        );
        assert_eq!(
            stream.documents().len(),
            fixture.expected_docs,
            "document count for {}",
            fixture.path
        );
    }
}

#[test]
fn lossless_replay_preserves_github_actions_comments_and_expressions() {
    let input = include_str!("fixtures/real-world/github-actions/starter-node-ci.yml");
    let stream = parse_lossless(input).expect("lossless parse GitHub Actions starter workflow");

    let comments = stream
        .comments()
        .map(|comment| comment.text())
        .collect::<Vec<_>>();
    assert!(comments.iter().any(|comment| {
        comment.contains("clean installation of node dependencies")
            && comment.contains("cache/restore them")
    }));
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("automating-builds-and-tests"))
    );
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("supported Node.js release schedule"))
    );

    let plain_values = scalar_values(&stream, ScalarStyle::Plain);
    assert!(plain_values.contains(&"${{ matrix.node-version }}"));
    assert!(plain_values.contains(&"$default-branch"));

    let flow_sequence_sources = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence {
                style: CollectionStyle::Flow,
                ..
            } => stream.source_fragment(node.span()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(flow_sequence_sources.contains(&"[ $default-branch ]"));
    assert!(flow_sequence_sources.contains(&"[18.x, 20.x, 22.x]"));
}

#[test]
fn lossless_replay_preserves_ansible_tags_and_block_scalars() {
    let input = include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml");
    let stream = parse_lossless(input).expect("lossless parse ansible tags");

    let tags = stream
        .nodes()
        .iter()
        .filter_map(|node| {
            let tag = node.tag()?;
            Some((
                tag.tag.to_string(),
                stream.source_fragment(tag.span).expect("tag source span"),
            ))
        })
        .collect::<BTreeSet<_>>();
    assert!(tags.contains(&("!vault".to_owned(), "!vault")));
    assert!(tags.contains(&("!unsafe".to_owned(), "!unsafe")));

    let literal_values = literal_scalar_values(&stream);
    assert!(
        literal_values
            .iter()
            .any(|value| value.contains("$ANSIBLE_VAULT;1.1;AES256"))
    );
    assert!(
        literal_values
            .iter()
            .any(|value| value.contains("PASSWORD={{ db_password }}"))
    );

    let unsafe_value = stream.nodes().iter().find_map(|node| match node.kind() {
        LosslessNodeKind::Scalar {
            value,
            style: ScalarStyle::DoubleQuoted,
        } if value == "{{ literal_must_not_render }}" => Some(value.as_str()),
        _ => None,
    });
    assert_eq!(unsafe_value, Some("{{ literal_must_not_render }}"));
}

#[test]
fn lossless_replay_preserves_compose_comments_and_flow_healthchecks() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");
    let stream = parse_lossless(input).expect("lossless parse Docker Compose snapshot");

    let comments = stream
        .comments()
        .map(|comment| comment.text())
        .collect::<Vec<_>>();
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("mariadb image"))
    );
    assert!(comments.iter().any(|comment| comment == &"#image: mysql:8"));
    assert!(stream.as_source().contains("depends_on: \n"));

    let flow_sequence_sources = collection_sources(&stream, CollectionStyle::Flow);
    assert!(
        flow_sequence_sources
            .iter()
            .any(|source| source.starts_with("['CMD-SHELL'") && source.contains("mysqladmin ping"))
    );
}

#[test]
fn lossless_replay_preserves_helm_chart_comments_and_quoted_app_version() {
    let input = include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml");
    let stream = parse_lossless(input).expect("lossless parse Helm Chart snapshot");

    let comments = stream
        .comments()
        .map(|comment| comment.text())
        .collect::<Vec<_>>();
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("chart version"))
    );
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("recommended to use it with quotes"))
    );

    let double_quoted_sources = scalar_sources(&stream, ScalarStyle::DoubleQuoted);
    assert!(double_quoted_sources.contains(&"\"1.16.0\""));
}

#[test]
fn lossless_replay_preserves_kubernetes_stream_boundaries_and_comments() {
    let input = include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml");
    let stream = parse_lossless(input).expect("lossless parse helm-rendered stream");

    assert_eq!(stream.documents().len(), 5);
    assert_eq!(
        stream
            .documents()
            .iter()
            .filter(|document| document.explicit_start())
            .count(),
        5
    );
    assert!(
        stream
            .documents()
            .last()
            .expect("last document")
            .explicit_end()
    );
    let comments = stream
        .comments()
        .map(|comment| comment.text())
        .collect::<Vec<_>>();
    assert!(comments.contains(&"# Source: chart/templates/disabled-cronjob.yaml"));
    assert!(comments.contains(&"# Rendered empty when cron.enabled=false"));

    let literal_values = literal_scalar_values(&stream);
    assert!(
        literal_values
            .iter()
            .any(|value| value.contains("featureFlags:\n  canary: true"))
    );
}

#[test]
fn lossless_replay_keeps_configmap_block_scalar_data() {
    let input = include_str!("fixtures/real-world/kubernetes/configmap-block-scalars.yaml");
    let stream = parse_lossless(input).expect("lossless parse configmap block scalars");

    let literal_sources = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Scalar {
                style: ScalarStyle::Literal,
                ..
            } => stream.source_fragment(node.span()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        literal_sources
            .iter()
            .any(|source| source.starts_with("|-") && source.contains("# this comment is data"))
    );
    assert!(
        literal_sources
            .iter()
            .any(|source| source.starts_with('|') && source.contains("hello\n\n    world"))
    );

    let literal_values = literal_scalar_values(&stream);
    assert!(
        literal_values
            .iter()
            .any(|value| value.contains("# blank line above is data"))
    );
    assert!(
        literal_values
            .iter()
            .any(|value| value.contains("hello\n\nworld"))
    );
}

#[test]
fn lossless_replay_preserves_openapi_block_scalars_flow_collections_and_refs() {
    let input = include_str!("fixtures/real-world/openapi/operations-and-polymorphism.yaml");
    let stream = parse_lossless(input).expect("lossless parse OpenAPI operations fixture");

    let literal_sources = scalar_sources(&stream, ScalarStyle::Literal);
    assert!(
        literal_sources.iter().any(|source| source.starts_with("|-")
            && source.contains("Requires an authenticated caller."))
    );

    let flow_sequence_sources = collection_sources(&stream, CollectionStyle::Flow);
    assert!(flow_sequence_sources.contains(&"[orders]"));
    assert!(flow_sequence_sources.contains(&"[id, status, line_items]"));
    assert!(flow_sequence_sources.contains(&"[pending, paid, refunded]"));

    let double_quoted_sources = scalar_sources(&stream, ScalarStyle::DoubleQuoted);
    assert!(double_quoted_sources.contains(&"\"200\""));
    assert!(double_quoted_sources.contains(&"\"404\""));
    assert!(double_quoted_sources.contains(&"\"1.0.0\""));

    assert!(double_quoted_sources.contains(&"\"#/components/schemas/Order\""));
    assert!(double_quoted_sources.contains(&"\"#/components/schemas/Error\""));

    let plain_values = scalar_values(&stream, ScalarStyle::Plain);
    assert!(plain_values.contains(&"application/problem+json"));
}

#[test]
fn lossless_replay_preserves_wrangler_comments_quoted_dates_and_flow_flags() {
    let input = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let stream = parse_lossless(input).expect("lossless parse Wrangler fixture");

    let comments = stream
        .comments()
        .map(|comment| comment.text())
        .collect::<Vec<_>>();
    assert!(
        comments
            .iter()
            .any(|comment| comment.contains("runtime settings"))
    );

    let double_quoted_sources = scalar_sources(&stream, ScalarStyle::DoubleQuoted);
    assert!(double_quoted_sources.contains(&"\"2026-05-23\""));

    let flow_sequence_sources = collection_sources(&stream, CollectionStyle::Flow);
    assert!(flow_sequence_sources.contains(&"[nodejs_compat]"));
}

fn fixture_manifest() -> FixtureManifest {
    toml::from_str(SOURCE).expect("real-world fixture source manifest parses")
}

fn lossless_replay_paths(manifest: &FixtureManifest) -> BTreeSet<&str> {
    manifest
        .fixture
        .iter()
        .filter(|fixture| fixture.gates.iter().any(|gate| gate == "lossless-replay"))
        .map(|fixture| fixture.path.as_str())
        .collect()
}

fn literal_scalar_values(stream: &yaml::LosslessStream) -> Vec<&str> {
    scalar_values(stream, ScalarStyle::Literal)
}

fn scalar_sources(stream: &yaml::LosslessStream, expected_style: ScalarStyle) -> Vec<&str> {
    stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Scalar { style, .. } if *style == expected_style => {
                stream.source_fragment(node.span())
            }
            _ => None,
        })
        .collect()
}

fn scalar_values(stream: &yaml::LosslessStream, expected_style: ScalarStyle) -> Vec<&str> {
    stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Scalar { value, style } if *style == expected_style => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect()
}

fn collection_sources(stream: &yaml::LosslessStream, expected_style: CollectionStyle) -> Vec<&str> {
    stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Mapping { style, .. } | LosslessNodeKind::Sequence { style, .. }
                if *style == expected_style =>
            {
                stream.source_fragment(node.span())
            }
            _ => None,
        })
        .collect()
}
