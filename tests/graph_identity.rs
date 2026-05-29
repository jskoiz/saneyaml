use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use yaml::{AnchorId, LosslessNodeKind, NodeId, Value, parse_lossless};
use yaml_rust2::parser::MarkedEventReceiver;

const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const REAL_WORLD_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");
const YAML_SUITE_MANIFEST: &str = include_str!("fixtures/yaml-test-suite/manifest.toml");
const YAML_SUITE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/yaml-test-suite/data"
);

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
    parity: SuiteParity,
}

#[derive(Debug, Deserialize)]
struct SuiteParity {
    lossless_graph: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
    name: String,
}

impl SuiteCase {
    fn fixture_dir(&self) -> String {
        self.id.replace('/', "-")
    }
}

#[derive(Debug, Deserialize)]
struct RealWorldManifest {
    fixture: Vec<RealWorldFixture>,
}

#[derive(Debug, Deserialize)]
struct RealWorldFixture {
    path: String,
    gates: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GraphOp {
    DocumentStart,
    DocumentEnd,
    Anchor { id: String, kind: GraphNodeKind },
    Alias { target: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphNodeKind {
    Scalar,
    Sequence,
    Mapping,
    Alias,
}

#[derive(Default)]
struct GraphNormalizer {
    next: usize,
    ids: BTreeMap<usize, String>,
}

impl GraphNormalizer {
    fn reset(&mut self) {
        self.next = 0;
        self.ids.clear();
    }

    fn define(&mut self, id: usize) -> Option<String> {
        if id == 0 {
            return None;
        }
        let normalized = self.next_id();
        self.ids.insert(id, normalized.clone());
        Some(normalized)
    }

    fn define_lossless(&mut self, id: AnchorId) -> String {
        let normalized = self.next_id();
        self.ids.insert(id.index(), normalized.clone());
        normalized
    }

    fn alias(&self, id: usize) -> String {
        self.ids
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("unknown:{id}"))
    }

    fn alias_lossless(&self, id: AnchorId) -> String {
        self.alias(id.index())
    }

    fn next_id(&mut self) -> String {
        self.next += 1;
        format!("a{}", self.next)
    }
}

struct YamlRust2Sink {
    events: Vec<yaml_rust2::parser::Event>,
}

impl MarkedEventReceiver for YamlRust2Sink {
    fn on_event(&mut self, event: yaml_rust2::parser::Event, _marker: yaml_rust2::scanner::Marker) {
        self.events.push(event);
    }
}

#[test]
fn lossless_graph_links_alias_to_anchor_identity() {
    let stream = parse_lossless("root: &root\n  child: 1\nref: *root\n").expect("lossless parse");
    let alias = &stream.aliases()[0];
    let target = stream.anchor(alias.target()).expect("target anchor");

    assert_eq!(alias.name(), "root");
    assert_eq!(target.name(), "root");
    assert_eq!(
        stream.node(target.node()).expect("anchored node").anchor(),
        Some(target.id())
    );
    assert!(matches!(
        stream.node(alias.node()).expect("alias node").kind(),
        LosslessNodeKind::Alias { target: alias_target, .. } if *alias_target == target.id()
    ));
}

#[test]
fn lossless_graph_tracks_anchor_redefinition_generations() {
    let stream = parse_lossless("a: &x one\nb: *x\nc: &x two\nd: *x\n").expect("lossless parse");
    let aliases = stream.aliases();

    assert_eq!(aliases.len(), 2);
    assert_ne!(aliases[0].target(), aliases[1].target());

    let first_anchor = stream.anchor(aliases[0].target()).expect("first anchor");
    let second_anchor = stream.anchor(aliases[1].target()).expect("second anchor");
    assert_eq!(first_anchor.name(), "x");
    assert_eq!(second_anchor.name(), "x");
    assert_eq!(
        stream.source_fragment(stream.node(first_anchor.node()).unwrap().span()),
        Some("one")
    );
    assert_eq!(
        stream.source_fragment(stream.node(second_anchor.node()).unwrap().span()),
        Some("two")
    );
}

#[test]
fn lossless_graph_accepts_recursive_alias_as_reference() {
    let stream = parse_lossless("root: &root [*root]\n").expect("lossless parse");
    let alias = &stream.aliases()[0];
    let anchor = stream.anchor(alias.target()).expect("recursive target");

    assert_eq!(anchor.name(), "root");
    assert!(matches!(
        stream.node(anchor.node()).expect("anchor node").kind(),
        LosslessNodeKind::Sequence { children, .. }
            if children.contains(&alias.node())
    ));
}

#[test]
fn lossless_graph_rejects_unknown_alias() {
    let error = parse_lossless("ref: *missing\n").expect_err("unknown alias");

    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.location().map(|location| location.line()), Some(1));
}

#[test]
fn lossless_graph_keeps_merge_key_and_alias_raw() {
    let input = "base: &base {a: 1}\nmerged:\n  <<: *base\n  b: 2\n";
    let stream = parse_lossless(input).expect("lossless parse");

    assert_eq!(stream.aliases().len(), 1);
    assert!(
        stream
            .nodes()
            .iter()
            .any(|node| stream.source_fragment(node.span()) == Some("<<"))
    );
    assert!(matches!(
        stream.node(stream.aliases()[0].node()).expect("merge alias").kind(),
        LosslessNodeKind::Alias { name, .. } if name == "base"
    ));
}

#[test]
fn lossless_graph_preserves_explicit_core_tag_alias_identity() {
    let input = "\
# explicit core tags with alias identity
values: &values
  forced_string: !!str null
  legacy_bool: !!bool ON
  explicit_null: !!null ~
copy: *values
";
    let stream = parse_lossless(input).expect("lossless parse");

    assert_eq!(stream.to_string(), input);
    let alias = stream
        .aliases()
        .iter()
        .find(|alias| alias.name() == "values")
        .expect("values alias");
    let target = stream.anchor(alias.target()).expect("values target");

    assert_eq!(target.name(), "values");
    assert_eq!(
        stream
            .node(target.node())
            .expect("anchored mapping")
            .anchor(),
        Some(target.id())
    );
    assert!(matches!(
        stream.node(alias.node()).expect("alias node").kind(),
        LosslessNodeKind::Alias { target: alias_target, .. } if *alias_target == target.id()
    ));

    let tag_tokens = stream
        .nodes()
        .iter()
        .filter_map(|node| node.tag())
        .map(|tag| (tag.tag.to_string(), stream.source_fragment(tag.span)))
        .collect::<Vec<_>>();

    for expected in ["!!str", "!!bool", "!!null"] {
        assert!(
            tag_tokens
                .iter()
                .any(|(tag, source)| tag == expected && *source == Some(expected)),
            "missing lossless explicit tag {expected}: {tag_tokens:?}"
        );
    }
}

#[test]
fn semantic_aliases_are_cloned_while_lossless_stream_keeps_identity() {
    let input = "\
base: &base
  items: [1]
first: *base
second: *base
";
    let mut value: Value = yaml::from_str(input).expect("semantic value");

    value["first"]["items"][0] = Value::from(2u64);
    assert_eq!(value["base"]["items"][0].as_u64(), Some(1));
    assert_eq!(value["first"]["items"][0].as_u64(), Some(2));
    assert_eq!(value["second"]["items"][0].as_u64(), Some(1));

    let stream = parse_lossless(input).expect("lossless parse");
    assert_eq!(stream.aliases().len(), 2);
    let first = stream
        .aliases()
        .iter()
        .find(|alias| alias.name() == "base")
        .expect("first base alias");
    let target = stream.anchor(first.target()).expect("base target");
    assert_eq!(target.name(), "base");
    assert!(
        stream
            .aliases()
            .iter()
            .all(|alias| alias.target() == target.id()),
        "all base aliases point at the same lossless graph target"
    );
}

#[test]
fn recursive_alias_identity_is_lossless_only_not_semantic_value_identity() {
    let input = "root: &root [*root]\n";

    let error = yaml::from_str::<Value>(input).expect_err("semantic recursive alias rejected");
    assert!(error.to_string().contains("recursive alias"));

    let stream = parse_lossless(input).expect("lossless parse");
    let alias = stream.aliases().first().expect("recursive alias");
    let anchor = stream.anchor(alias.target()).expect("recursive target");

    assert_eq!(alias.name(), "root");
    assert_eq!(anchor.name(), "root");
    assert!(matches!(
        stream.node(anchor.node()).expect("anchor node").kind(),
        LosslessNodeKind::Sequence { children, .. }
            if children.contains(&alias.node())
    ));
}

#[test]
fn lossless_graph_anchor_targets_match_reference_parser_events() {
    let cases = [
        (
            "anchor_redefinition",
            "a: &x one\nb: *x\nc: &x two\nd: *x\n",
        ),
        ("recursive_alias", "root: &root [*root]\n"),
        (
            "document_anchor_reset",
            "---\na: &x one\nb: *x\n---\na: &x two\nb: *x\n",
        ),
        (
            "merge_alias",
            "base: &base {a: 1}\nmerged:\n  <<: *base\n  b: 2\n",
        ),
        (
            "yaml11_lossless_merge_graph",
            include_str!("fixtures/yaml11-conformance/lossless-merge-graph.yaml"),
        ),
        (
            "yaml11_lossless_recursive_graph",
            include_str!("fixtures/yaml11-conformance/lossless-recursive-graph.yaml"),
        ),
        (
            "yaml_test_suite_6m2f",
            include_str!("fixtures/yaml-test-suite/data/6M2F/in.yaml"),
        ),
        (
            "docker_compose_anchors",
            include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml"),
        ),
    ];

    for (name, input) in cases {
        let ours = normalize_lossless_graph(input).expect("lossless graph");
        assert_eq!(
            ours,
            normalize_yaml_rust2_graph(input).expect("yaml-rust2 graph"),
            "yaml-rust2 graph identity parity for {name}"
        );
        assert_eq!(
            ours,
            normalize_saphyr_graph(input).expect("saphyr graph"),
            "saphyr graph identity parity for {name}"
        );
    }
}

#[test]
fn edited_lossless_graph_still_matches_reference_parser_events() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml");
    let stream = parse_lossless(input).expect("lossless parse");
    let image_start = input.find("nginx:latest").expect("web image value");
    let image_end = image_start + "nginx:latest".len();
    let labels_insert = input.find("    ports:\n").expect("ports entry");
    let command_start = input.find("    command:").expect("worker command");
    let command_end = command_start + input[command_start..].find('\n').expect("command line") + 1;

    let mut edit = stream.edit();
    edit.replace_source_span(
        stream
            .source_span(image_start, image_end)
            .expect("image source span"),
        "nginx:1.27",
    )
    .expect("replace image source");
    edit.insert_source(labels_insert, "    labels:\n      com.example.role: web\n")
        .expect("insert labels");
    edit.delete_source_span(
        stream
            .source_span(command_start, command_end)
            .expect("command source span"),
    )
    .expect("delete command");
    let output = edit.finish().expect("validated edited YAML");

    let edited = parse_lossless(&output).expect("edited output reparses");
    assert_eq!(edited.as_source(), output);
    assert_eq!(edited.aliases().len(), 2);
    assert!(
        edited
            .nodes()
            .iter()
            .any(|node| edited.source_fragment(node.span()) == Some("<<"))
    );

    let ours = normalize_lossless_graph(&output).expect("edited lossless graph");
    assert_eq!(
        ours,
        normalize_yaml_rust2_graph(&output).expect("edited yaml-rust2 graph"),
        "yaml-rust2 graph identity parity after source edits"
    );
    assert_eq!(
        ours,
        normalize_saphyr_graph(&output).expect("edited saphyr graph"),
        "saphyr graph identity parity after source edits"
    );
}

#[test]
fn real_world_lossless_graph_manifest_cases_match_reference_parser_events() {
    let manifest: RealWorldManifest =
        toml::from_str(REAL_WORLD_SOURCE).expect("real-world manifest parses");
    let cases = manifest
        .fixture
        .iter()
        .filter(|fixture| fixture.gates.iter().any(|gate| gate == "lossless-graph"))
        .collect::<Vec<_>>();
    let paths = cases
        .iter()
        .map(|fixture| fixture.path.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        paths,
        BTreeSet::from([
            "docker-compose/adapted-compose-spec-fragments.yaml",
            "docker-compose/compose-anchors.yaml",
            "docker-compose/compose-polymorphic.yaml",
        ])
    );

    for fixture in cases {
        let input = fs::read_to_string(Path::new(REAL_WORLD_ROOT).join(&fixture.path))
            .unwrap_or_else(|error| panic!("read real-world fixture {}: {error}", fixture.path));
        let stream = parse_lossless(&input)
            .unwrap_or_else(|error| panic!("{} lossless parse: {error}", fixture.path));
        assert_eq!(
            stream.as_source(),
            input,
            "{} source retained",
            fixture.path
        );
        assert_eq!(stream.to_string(), input, "{} display replay", fixture.path);
        assert!(
            !stream.aliases().is_empty(),
            "{} must exercise aliases",
            fixture.path
        );

        let ours = normalize_lossless_graph(&input)
            .unwrap_or_else(|error| panic!("{} lossless graph: {error}", fixture.path));
        assert_eq!(
            ours,
            normalize_yaml_rust2_graph(&input)
                .unwrap_or_else(|error| panic!("{} yaml-rust2 graph: {error}", fixture.path)),
            "yaml-rust2 graph identity parity for {}",
            fixture.path
        );
        assert_eq!(
            ours,
            normalize_saphyr_graph(&input)
                .unwrap_or_else(|error| panic!("{} saphyr graph: {error}", fixture.path)),
            "saphyr graph identity parity for {}",
            fixture.path
        );
    }
}

#[test]
fn yaml_suite_anchor_alias_cases_match_reference_parser_events() {
    let manifest: SuiteManifest =
        toml::from_str(YAML_SUITE_MANIFEST).expect("YAML-suite manifest parses");
    let cases_by_id = manifest
        .case
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let cases = manifest
        .parity
        .lossless_graph
        .iter()
        .map(|id| {
            assert!(seen.insert(id.as_str()), "duplicate lossless graph id {id}");
            cases_by_id
                .get(id.as_str())
                .unwrap_or_else(|| panic!("lossless graph id {id} must exist in YAML-suite cases"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cases.len(),
        14,
        "manifest-owned lossless graph parity case count drifted"
    );

    for case in cases {
        let path = Path::new(YAML_SUITE_ROOT)
            .join(case.fixture_dir())
            .join("in.yaml");
        let input = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "read YAML-suite graph fixture {} ({}): {error}",
                case.id, case.name
            )
        });
        let ours = normalize_lossless_graph(&input)
            .unwrap_or_else(|error| panic!("{} lossless graph: {error}", case.id));
        assert_eq!(
            ours,
            normalize_yaml_rust2_graph(&input)
                .unwrap_or_else(|error| panic!("{} yaml-rust2 graph: {error}", case.id)),
            "yaml-rust2 graph identity parity for {} ({})",
            case.id,
            case.name
        );
        assert_eq!(
            ours,
            normalize_saphyr_graph(&input)
                .unwrap_or_else(|error| panic!("{} saphyr graph: {error}", case.id)),
            "saphyr graph identity parity for {} ({})",
            case.id,
            case.name
        );
    }
}

fn normalize_lossless_graph(input: &str) -> yaml::Result<Vec<GraphOp>> {
    let stream = parse_lossless(input)?;
    let mut normalizer = GraphNormalizer::default();
    let mut graph = Vec::new();

    for document in stream.documents() {
        normalizer.reset();
        graph.push(GraphOp::DocumentStart);
        if let Some(root) = document.root() {
            push_lossless_graph_node(&stream, root, &mut normalizer, &mut graph);
        }
        graph.push(GraphOp::DocumentEnd);
    }

    Ok(graph)
}

fn push_lossless_graph_node(
    stream: &yaml::LosslessStream,
    node_id: NodeId,
    normalizer: &mut GraphNormalizer,
    graph: &mut Vec<GraphOp>,
) {
    let node = stream.node(node_id).expect("lossless node id");

    if let Some(anchor) = node.anchor() {
        graph.push(GraphOp::Anchor {
            id: normalizer.define_lossless(anchor),
            kind: lossless_graph_kind(node.kind()),
        });
    }

    match node.kind() {
        LosslessNodeKind::Scalar { .. } => {}
        LosslessNodeKind::Sequence { children, .. } => {
            for child in children {
                push_lossless_graph_node(stream, *child, normalizer, graph);
            }
        }
        LosslessNodeKind::Mapping { entries, .. } => {
            for (key, value) in entries {
                push_lossless_graph_node(stream, *key, normalizer, graph);
                push_lossless_graph_node(stream, *value, normalizer, graph);
            }
        }
        LosslessNodeKind::Alias { target, .. } => graph.push(GraphOp::Alias {
            target: normalizer.alias_lossless(*target),
        }),
    }
}

fn lossless_graph_kind(kind: &LosslessNodeKind) -> GraphNodeKind {
    match kind {
        LosslessNodeKind::Scalar { .. } => GraphNodeKind::Scalar,
        LosslessNodeKind::Sequence { .. } => GraphNodeKind::Sequence,
        LosslessNodeKind::Mapping { .. } => GraphNodeKind::Mapping,
        LosslessNodeKind::Alias { .. } => GraphNodeKind::Alias,
    }
}

fn normalize_yaml_rust2_graph(input: &str) -> Result<Vec<GraphOp>, yaml_rust2::scanner::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;

    let mut normalizer = GraphNormalizer::default();
    let mut graph = Vec::new();
    for event in sink.events {
        match event {
            yaml_rust2::parser::Event::Nothing
            | yaml_rust2::parser::Event::StreamStart
            | yaml_rust2::parser::Event::StreamEnd
            | yaml_rust2::parser::Event::SequenceEnd
            | yaml_rust2::parser::Event::MappingEnd => {}
            yaml_rust2::parser::Event::DocumentStart => {
                normalizer.reset();
                graph.push(GraphOp::DocumentStart);
            }
            yaml_rust2::parser::Event::DocumentEnd => graph.push(GraphOp::DocumentEnd),
            yaml_rust2::parser::Event::Alias(anchor) => graph.push(GraphOp::Alias {
                target: normalizer.alias(anchor),
            }),
            yaml_rust2::parser::Event::Scalar(_, _, anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Scalar,
                    });
                }
            }
            yaml_rust2::parser::Event::SequenceStart(anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Sequence,
                    });
                }
            }
            yaml_rust2::parser::Event::MappingStart(anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Mapping,
                    });
                }
            }
        }
    }

    Ok(graph)
}

fn normalize_saphyr_graph(input: &str) -> Result<Vec<GraphOp>, saphyr_parser::ScanError> {
    let mut normalizer = GraphNormalizer::default();
    let mut graph = Vec::new();

    for result in saphyr_parser::Parser::new_from_str(input) {
        match result?.0 {
            saphyr_parser::Event::Nothing
            | saphyr_parser::Event::StreamStart
            | saphyr_parser::Event::StreamEnd
            | saphyr_parser::Event::SequenceEnd
            | saphyr_parser::Event::MappingEnd => {}
            saphyr_parser::Event::DocumentStart(_) => {
                normalizer.reset();
                graph.push(GraphOp::DocumentStart);
            }
            saphyr_parser::Event::DocumentEnd => graph.push(GraphOp::DocumentEnd),
            saphyr_parser::Event::Alias(anchor) => graph.push(GraphOp::Alias {
                target: normalizer.alias(anchor),
            }),
            saphyr_parser::Event::Scalar(_, _, anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Scalar,
                    });
                }
            }
            saphyr_parser::Event::SequenceStart(anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Sequence,
                    });
                }
            }
            saphyr_parser::Event::MappingStart(anchor, _) => {
                if let Some(id) = normalizer.define(anchor) {
                    graph.push(GraphOp::Anchor {
                        id,
                        kind: GraphNodeKind::Mapping,
                    });
                }
            }
        }
    }

    Ok(graph)
}
