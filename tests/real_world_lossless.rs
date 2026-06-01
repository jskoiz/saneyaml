use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use yaml::{
    CollectionStyle, LosslessEffectiveMappingSource, LosslessNodeKind, LosslessStream, NodeId,
    PathSegment, ScalarStyle, parse_lossless,
};

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

#[test]
fn lossless_effective_mapping_view_expands_compose_merge_without_rewriting_source() {
    let input =
        include_str!("fixtures/real-world/docker-compose/adapted-compose-spec-fragments.yaml");
    let stream = parse_lossless(input).expect("lossless parse Compose spec fragment");
    let root = stream.documents()[0].root().expect("root mapping");
    let services = mapping_value_by_scalar_key(&stream, root, "services");
    let frontend = mapping_value_by_scalar_key(&stream, services, "frontend");
    let environment = mapping_value_by_scalar_key(&stream, frontend, "environment");

    let entries = stream
        .effective_mapping_entries(environment)
        .expect("Compose environment effective entries");

    assert_eq!(stream.as_source(), input);
    assert!(
        stream
            .nodes()
            .iter()
            .any(|node| stream.source_fragment(node.span()) == Some("<<")),
        "raw merge key remains available for source-preserving tools"
    );
    assert_effective_scalar_entry(&stream, &entries, "YET_ANOTHER", "VARIABLE", None, false);
    assert_effective_scalar_entry(
        &stream,
        &entries,
        "FOO",
        "BAR",
        Some("default-environment"),
        false,
    );
    assert_effective_scalar_entry(&stream, &entries, "KEY", "VALUE", Some("keys"), false);
}

#[test]
fn lossless_in_place_edit_bumps_helm_versions_with_minimal_diff() {
    let input = include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml");
    let stream = parse_lossless(input).expect("lossless parse Helm chart");
    let root = stream.documents()[0].root().expect("root mapping");

    let mut edit = stream.edit();
    edit.replace_mapping_value_source(root, "version", "0.2.0")
        .expect("replace chart version");
    edit.replace_mapping_value_source(root, "appVersion", "\"1.17.0\"")
        .expect("replace app version");
    let edited = edit.finish().expect("edited Helm chart re-parses");

    // Untouched spans (comments, blank lines, every other key) stay byte-stable;
    // only the two intended value lines change.
    assert_minimal_line_diff(
        input,
        &edited,
        &[
            ("version: 0.1.0", "version: 0.2.0"),
            ("appVersion: \"1.16.0\"", "appVersion: \"1.17.0\""),
        ],
    );
}

#[test]
fn lossless_in_place_edit_updates_compose_db_service_with_minimal_diff() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");
    let stream = parse_lossless(input).expect("lossless parse Compose stack");
    let root = stream.documents()[0].root().expect("root mapping");
    let services = mapping_value_by_scalar_key(&stream, root, "services");
    let db = mapping_value_by_scalar_key(&stream, services, "db");

    let mut edit = stream.edit();
    edit.replace_mapping_value_source(db, "image", "mariadb:11-focal")
        .expect("replace db image");
    edit.replace_mapping_value_source(db, "restart", "unless-stopped")
        .expect("replace db restart policy");
    let edited = edit.finish().expect("edited Compose stack re-parses");

    // The db service nests under services; the sibling `restart: always` lines on
    // the backend/proxy services and the inline `#image: mysql:8` comment must be
    // left untouched.
    assert_minimal_line_diff(
        input,
        &edited,
        &[
            ("    image: mariadb:10-focal", "    image: mariadb:11-focal"),
            ("    restart: always", "    restart: unless-stopped"),
        ],
    );
}

#[test]
fn lossless_in_place_edit_extends_wrangler_flow_and_block_sequences() {
    let input = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let stream = parse_lossless(input).expect("lossless parse wrangler config");
    let root = stream.documents()[0].root().expect("root mapping");
    let flags = mapping_value_by_scalar_key(&stream, root, "compatibility_flags");
    let routes = mapping_value_by_scalar_key(&stream, root, "routes");

    let mut edit = stream.edit();
    edit.insert_flow_sequence_item_source(flags, 1, "nodejs_als")
        .expect("append compatibility flag");
    edit.insert_block_sequence_item_source(
        routes,
        1,
        "pattern: api.example.com/*\nzone_name: example.com",
    )
    .expect("append route");
    let edited = edit.finish().expect("edited wrangler config re-parses");

    // The runtime-settings comment and every untouched key stay byte-stable;
    // only the flow flag line changes in place and the new block route appears.
    assert_line_changes(
        input,
        &edited,
        &["compatibility_flags: [nodejs_compat]"],
        &[
            "compatibility_flags: [nodejs_compat, nodejs_als]",
            "  - pattern: api.example.com/*",
            "    zone_name: example.com",
        ],
    );
}

#[test]
fn lossless_in_place_delete_keeps_compose_service_comments() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");
    let stream = parse_lossless(input).expect("lossless parse Compose stack");
    let root = stream.documents()[0].root().expect("root mapping");
    let services = mapping_value_by_scalar_key(&stream, root, "services");
    let db = mapping_value_by_scalar_key(&stream, services, "db");

    let edited = stream
        .delete_block_mapping_entry_source(db, "expose")
        .expect("delete db expose entry");

    // Only the three `expose` lines are removed; the db service's leading
    // comments (which are attached to other entries) survive untouched.
    assert_line_changes(
        input,
        &edited,
        &["    expose:", "      - 3306", "      - 33060"],
        &[],
    );
    assert!(
        edited.contains("# We use a mariadb image which supports both amd64 & arm64 architecture"),
        "image comment preserved after deleting a sibling entry"
    );
    assert!(
        edited.contains("#image: mysql:8"),
        "commented-out alternative image preserved after deletion"
    );
}

#[test]
fn lossless_path_addressed_edits_update_compose_without_manual_navigation() {
    let input = include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml");
    let stream = parse_lossless(input).expect("lossless parse Compose stack");

    // Replace a deeply nested value addressed by path, with no hand-built NodeId
    // navigation through services -> db -> image.
    let after_image = stream
        .replace_value_at_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("db"),
                PathSegment::from("image"),
            ],
            "mariadb:11-focal",
        )
        .expect("replace db image by path");
    assert_minimal_line_diff(
        input,
        &after_image,
        &[("    image: mariadb:10-focal", "    image: mariadb:11-focal")],
    );

    // Resolve a container by path, then reuse the existing sequence helper to
    // append an item — the path layer composes with the structural editors.
    let backend_ports = stream
        .resolve_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("backend"),
                PathSegment::from("ports"),
            ],
        )
        .expect("resolve services.backend.ports");
    let after_port = stream
        .insert_block_sequence_item_source(backend_ports, 1, "9000:9000")
        .expect("append backend port");
    assert_line_changes(input, &after_port, &[], &["      - 9000:9000"]);
}

#[test]
fn lossless_path_addressed_edits_update_kubernetes_deployment() {
    let input = include_str!("fixtures/real-world/kubernetes/deployment.yaml");
    let stream = parse_lossless(input).expect("lossless parse Kubernetes deployment");

    // A deeply nested container image, addressed by path with no manual walk
    // through spec -> template -> spec -> containers[0] -> image.
    let bumped = stream
        .replace_value_at_path(
            0,
            &[
                "spec".into(),
                "template".into(),
                "spec".into(),
                "containers".into(),
                0usize.into(),
                "image".into(),
            ],
            "nginx:1.27",
        )
        .expect("bump container image by path");
    assert_minimal_line_diff(
        input,
        &bumped,
        &[("          image: nginx:1.25", "          image: nginx:1.27")],
    );

    // Delete the nested resources block; the block style is detected from the
    // resolved container mapping.
    let pruned = stream
        .delete_at_path(
            0,
            &[
                "spec".into(),
                "template".into(),
                "spec".into(),
                "containers".into(),
                0usize.into(),
                "resources".into(),
            ],
        )
        .expect("delete container resources by path");
    assert_line_changes(
        input,
        &pruned,
        &[
            "          resources:",
            "            requests:",
            "              cpu: 100m",
            "              memory: 128Mi",
        ],
        &[],
    );

    // Insert a label into the metadata.labels block mapping.
    let labeled = stream
        .insert_entry_at_path(0, &["metadata".into(), "labels".into()], "tier: frontend")
        .expect("insert metadata label by path");
    assert_line_changes(input, &labeled, &[], &["    tier: frontend"]);
}

#[test]
fn lossless_path_addressed_edit_extends_openapi_flow_required() {
    let input = include_str!("fixtures/real-world/openapi/petstore-fragment.yaml");
    let stream = parse_lossless(input).expect("lossless parse OpenAPI fragment");

    // `required: [id, name]` is a flow sequence; insert_item_at_path detects the
    // flow style and rewrites the line in place.
    let extended = stream
        .insert_item_at_path(
            0,
            &[
                "components".into(),
                "schemas".into(),
                "Pet".into(),
                "required".into(),
            ],
            2,
            "tag",
        )
        .expect("append required field by path");
    assert_minimal_line_diff(
        input,
        &extended,
        &[(
            "      required: [id, name]",
            "      required: [id, name, tag]",
        )],
    );

    // A nested scalar replace through the same path layer.
    let retitled = stream
        .replace_value_at_path(0, &["info".into(), "version".into()], "\"1.1.0\"")
        .expect("bump info.version by path");
    assert_minimal_line_diff(
        input,
        &retitled,
        &[("  version: \"1.0.0\"", "  version: \"1.1.0\"")],
    );
}

/// Asserts the line-level diff between `original` and `edited` consists of exactly
/// the given removed and added lines (each in document order). Because it is built
/// on a longest-common-subsequence diff, every line not listed here — comments,
/// blank lines, sibling entries — is proven byte-stable.
fn assert_line_changes(
    original: &str,
    edited: &str,
    expected_removed: &[&str],
    expected_added: &[&str],
) {
    let before: Vec<&str> = original.lines().collect();
    let after: Vec<&str> = edited.lines().collect();
    let (n, m) = (before.len(), after.len());

    // Longest-common-subsequence length table over whole lines.
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if before[i] == after[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    let mut removed = Vec::new();
    let mut added = Vec::new();
    while i < n && j < m {
        if before[i] == after[j] {
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            removed.push(before[i]);
            i += 1;
        } else {
            added.push(after[j]);
            j += 1;
        }
    }
    removed.extend_from_slice(&before[i..]);
    added.extend_from_slice(&after[j..]);

    assert_eq!(removed, expected_removed, "unexpected removed lines");
    assert_eq!(added, expected_added, "unexpected added lines");
}

/// Asserts the edit changed only the intended lines: line count is preserved and
/// the ordered set of `(before, after)` line replacements matches `expected`,
/// proving every untouched line is byte-stable.
fn assert_minimal_line_diff(original: &str, edited: &str, expected: &[(&str, &str)]) {
    let original_lines: Vec<&str> = original.lines().collect();
    let edited_lines: Vec<&str> = edited.lines().collect();
    assert_eq!(
        original_lines.len(),
        edited_lines.len(),
        "in-place edit must not add or remove lines"
    );
    let changes: Vec<(&str, &str)> = original_lines
        .iter()
        .zip(edited_lines.iter())
        .filter(|(before, after)| before != after)
        .map(|(before, after)| (*before, *after))
        .collect();
    let expected: Vec<(&str, &str)> = expected.to_vec();
    assert_eq!(changes, expected, "unexpected lines changed during edit");
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

fn mapping_value_by_scalar_key(stream: &LosslessStream, mapping: NodeId, key: &str) -> NodeId {
    let mapping = stream.node(mapping).expect("mapping node");
    let LosslessNodeKind::Mapping { entries, .. } = mapping.kind() else {
        panic!("expected mapping node");
    };
    entries
        .iter()
        .find_map(|(entry_key, value)| {
            (scalar_value(stream, *entry_key) == Some(key)).then_some(*value)
        })
        .unwrap_or_else(|| panic!("mapping key {key:?} not found"))
}

fn assert_effective_scalar_entry(
    stream: &LosslessStream,
    entries: &[yaml::LosslessEffectiveMappingEntry],
    key: &str,
    value: &str,
    merge_anchor: Option<&str>,
    overridden: bool,
) {
    let entry = entries
        .iter()
        .find(|entry| {
            scalar_value(stream, entry.key()) == Some(key)
                && scalar_value(stream, entry.value()) == Some(value)
        })
        .unwrap_or_else(|| panic!("effective entry {key:?}: {value:?} not found"));
    assert_eq!(entry.is_overridden(), overridden);
    match (entry.source(), merge_anchor) {
        (source, None) => assert!(source.is_explicit(), "expected explicit entry"),
        (
            LosslessEffectiveMappingSource::Merge {
                alias,
                target_anchor,
                ..
            },
            Some(expected),
        ) => {
            let alias = alias.and_then(|id| stream.alias(id)).expect("merge alias");
            let target = target_anchor
                .and_then(|id| stream.anchor(id))
                .expect("target anchor");
            assert_eq!(alias.name(), expected);
            assert_eq!(target.name(), expected);
        }
        (source, Some(expected)) => panic!("expected merge entry from {expected}, got {source:?}"),
    }
}

fn scalar_value(stream: &LosslessStream, node: NodeId) -> Option<&str> {
    match stream.node(node)?.kind() {
        LosslessNodeKind::Scalar { value, .. } => Some(value),
        _ => None,
    }
}
