use yaml::{
    CollectionStyle, LosslessNodeKind, LosslessTriviaKind, PathSegment, ScalarStyle,
    parse_lossless, parse_lossless_bytes,
};

#[test]
fn lossless_stream_keeps_source_comments_directives_and_markers() {
    let input = "\
%YAML 1.2
%TAG !e! tag:example.com,2026:
---
# header
root: &root
  # child
  name: \"api\" # inline
  list: [1, 'two', !e!Thing three]
ref: *root
...
";
    let stream = parse_lossless(input).expect("lossless parse");

    assert_eq!(stream.as_source(), input);
    assert_eq!(stream.to_string(), input);
    assert_eq!(stream.clone().into_source(), input);

    let document = &stream.documents()[0];
    assert!(document.explicit_start());
    assert!(document.explicit_end());
    assert_eq!(
        document
            .directives()
            .yaml_version
            .as_ref()
            .expect("YAML directive")
            .minor,
        2
    );
    assert_eq!(document.directives().tag_directives[0].handle, "!e!");

    let comments = stream.comments().collect::<Vec<_>>();
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].text(), "# header");
    assert_eq!(comments[1].text(), "# child");
    assert_eq!(comments[2].text(), "# inline");
    assert_eq!(comments[2].kind(), LosslessTriviaKind::Comment);

    let quoted = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Scalar {
                    value,
                    style: ScalarStyle::DoubleQuoted,
                } if value == "api"
            )
        })
        .expect("double-quoted scalar");
    assert_eq!(stream.source_fragment(quoted.span()), Some("\"api\""));
}

#[test]
fn lossless_bytes_reject_invalid_utf8_with_span() {
    let error = parse_lossless_bytes(b"ok: \xFF\n").expect_err("invalid UTF-8");

    assert!(error.to_string().contains("input is not valid UTF-8"));
    assert_eq!(error.location().map(|location| location.index()), Some(4));
}

#[test]
fn lossless_stream_exposes_flow_and_block_collection_styles() {
    let stream = parse_lossless("block:\n  - one\nflow: [a, b]\n").expect("lossless parse");
    let sequence_styles = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence { style, .. } => Some(*style),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sequence_styles,
        [CollectionStyle::Block, CollectionStyle::Flow]
    );
}

#[test]
fn lossless_stream_exposes_empty_and_whitespace_blank_lines() {
    let input = "a: 1\n\n  \n# note\nb: 2\n";
    let stream = parse_lossless(input).expect("lossless parse");
    let blanks = stream
        .trivia()
        .iter()
        .filter(|trivia| trivia.kind() == LosslessTriviaKind::BlankLine)
        .collect::<Vec<_>>();

    assert_eq!(blanks.len(), 2);
    assert_eq!(blanks[0].text(), "");
    assert_eq!(stream.source_fragment(blanks[0].span()), Some(""));
    assert_eq!(blanks[1].text(), "  ");
    assert_eq!(stream.source_fragment(blanks[1].span()), Some("  "));
    assert_eq!(stream.to_string(), input);
}

#[test]
fn lossless_edit_replaces_scalar_source_and_preserves_untouched_formatting() {
    let input = "\
# service config
root: &root
  name: \"api\" # keep comment
  ports: [80, 443]
copy: *root
";
    let stream = parse_lossless(input).expect("lossless parse");
    let name = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Scalar {
                    value,
                    style: ScalarStyle::DoubleQuoted,
                } if value == "api"
            )
        })
        .expect("name scalar");

    let mut edit = stream.edit();
    edit.replace_scalar_source(name.id(), "worker")
        .expect("replace scalar");
    let output = edit.finish().expect("validated edited YAML");

    assert_eq!(
        output,
        "\
# service config
root: &root
  name: worker # keep comment
  ports: [80, 443]
copy: *root
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 2);
    assert_eq!(edited.aliases().len(), 1);
    assert_eq!(
        edited
            .anchor(edited.aliases()[0].target())
            .expect("alias target")
            .name(),
        "root"
    );
}

#[test]
fn lossless_edit_replaces_flow_mapping_source_and_preserves_surrounding_bytes() {
    let input = "\
# service config
root: &root {name: api, ports: [80, 443]} # keep
copy: *root
";
    let stream = parse_lossless(input).expect("lossless parse");
    let mapping = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Mapping {
                    style: CollectionStyle::Flow,
                    ..
                }
            )
        })
        .expect("flow mapping");

    let output = stream
        .replace_node_source(mapping.id(), "{name: worker, ports: [8080]}")
        .expect("validated mapping replacement");

    assert_eq!(
        output,
        "\
# service config
root: &root {name: worker, ports: [8080]} # keep
copy: *root
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 2);
    assert_eq!(edited.aliases().len(), 1);
}

#[test]
fn lossless_edit_replaces_flow_sequence_source_and_preserves_tail_formatting() {
    let input = "\
root:
  list: [one, two, three] # keep
  other: yes
";
    let stream = parse_lossless(input).expect("lossless parse");
    let sequence = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Sequence {
                    style: CollectionStyle::Flow,
                    ..
                }
            )
        })
        .expect("flow sequence");

    let output = stream
        .replace_node_source(sequence.id(), "[alpha, beta]")
        .expect("validated sequence replacement");

    assert_eq!(
        output,
        "\
root:
  list: [alpha, beta] # keep
  other: yes
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 1);
}

#[test]
fn lossless_edit_rewrites_compose_source_spans_and_keeps_merge_graph() {
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

    assert_eq!(
        output,
        "\
version: \"3.9\"

x-service-defaults: &service-defaults
  restart: unless-stopped
  logging:
    driver: json-file
  environment:
    RUST_LOG: info

services:
  web:
    <<: *service-defaults
    image: nginx:1.27
    labels:
      com.example.role: web
    ports:
      - \"8080:80\"
  worker:
    <<: *service-defaults
    image: example/worker:latest
"
    );
    let edited = parse_lossless(&output).expect("edited compose source reparses");
    assert_eq!(edited.aliases().len(), 2);
    assert!(
        edited
            .nodes()
            .iter()
            .any(|node| edited.source_fragment(node.span()) == Some("<<"))
    );
}

#[test]
fn lossless_edit_structurally_updates_block_mapping_entries() {
    let input = "\
# service config
service: &svc
  image: nginx:latest # keep image comment
  ports:
    - \"8080:80\"
copy: *svc
";
    let stream = parse_lossless(input).expect("lossless parse");
    let service = block_mapping_with_key(&stream, "image");

    let mut edit = stream.edit();
    edit.replace_mapping_value_source(service.id(), "image", "nginx:1.27")
        .expect("replace image value");
    edit.insert_block_mapping_entry_source(
        service.id(),
        "\
labels:
  com.example.role: web",
    )
    .expect("insert labels entry");
    edit.delete_block_mapping_entry_source(service.id(), "ports")
        .expect("delete ports entry");
    let output = edit.finish().expect("validated structural edit");

    assert_eq!(
        output,
        "\
# service config
service: &svc
  image: nginx:1.27 # keep image comment
  labels:
    com.example.role: web
copy: *svc
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 2);
    assert_eq!(edited.aliases().len(), 1);
    assert_eq!(
        edited
            .anchor(edited.aliases()[0].target())
            .expect("alias target")
            .name(),
        "svc"
    );
}

#[test]
fn lossless_edit_rejects_invalid_structural_mapping_edits() {
    let stream = parse_lossless("service:\n  image: nginx\n").expect("lossless parse");
    let service = block_mapping_with_key(&stream, "image");

    let missing = stream
        .edit()
        .replace_mapping_value_source(service.id(), "ports", "[]")
        .expect_err("missing key");
    assert!(missing.to_string().contains("was not found"));

    let invalid_entry = stream
        .edit()
        .insert_block_mapping_entry_source(service.id(), "one: 1\ntwo: 2")
        .expect_err("not one mapping entry");
    assert!(
        invalid_entry
            .to_string()
            .contains("exactly one mapping entry")
    );

    let flow = parse_lossless("service: {image: nginx}\n").expect("lossless parse");
    let flow_mapping = flow
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Mapping {
                    style: CollectionStyle::Flow,
                    ..
                }
            )
        })
        .expect("flow mapping");
    let error = flow
        .edit()
        .delete_block_mapping_entry_source(flow_mapping.id(), "image")
        .expect_err("flow deletion rejected");
    assert!(error.to_string().contains("requires a block mapping"));

    let block_error = stream
        .edit()
        .insert_flow_mapping_entry_source(service.id(), "ports: []")
        .expect_err("block insertion through flow helper rejected");
    assert!(block_error.to_string().contains("requires a flow mapping"));
}

#[test]
fn lossless_edit_inserts_block_mapping_entry_after_final_line_without_trailing_newline() {
    let stream = parse_lossless("service:\n  image: nginx").expect("lossless parse");
    let service = block_mapping_with_key(&stream, "image");

    let output = stream
        .insert_block_mapping_entry_source(service.id(), "replicas: 2")
        .expect("insert final entry");

    assert_eq!(output, "service:\n  image: nginx\n  replicas: 2\n");
}

#[test]
fn lossless_edit_structurally_updates_flow_mapping_entries() {
    let input = "\
service: {image: nginx, ports: [80], labels: {role: web}} # keep
after: true
";
    let stream = parse_lossless(input).expect("lossless parse");
    let service = flow_mapping_with_key(&stream, "image");

    let mut edit = stream.edit();
    edit.insert_flow_mapping_entry_source(service.id(), "replicas: 2")
        .expect("insert flow entry");
    edit.delete_flow_mapping_entry_source(service.id(), "ports")
        .expect("delete flow entry");
    let output = edit.finish().expect("validated flow mapping edit");

    assert_eq!(
        output,
        "\
service: {image: nginx, labels: {role: web}, replicas: 2} # keep
after: true
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 1);
}

#[test]
fn lossless_edit_handles_empty_and_single_entry_flow_mappings() {
    let empty = parse_lossless("service: {}\n").expect("lossless parse");
    let mapping = flow_mapping_by_len(&empty, 0);
    let output = empty
        .insert_flow_mapping_entry_source(mapping.id(), "replicas: 2")
        .expect("insert into empty flow mapping");
    assert_eq!(output, "service: {replicas: 2}\n");

    let single = parse_lossless("service: {replicas: 2}\n").expect("lossless parse");
    let mapping = flow_mapping_by_len(&single, 1);
    let output = single
        .delete_flow_mapping_entry_source(mapping.id(), "replicas")
        .expect("delete single flow mapping entry");
    assert_eq!(output, "service: {}\n");
}

#[test]
fn lossless_edit_structurally_updates_block_sequence_items() {
    let input = "\
steps:
  # keep step comment
  - name: build
    run: cargo build # keep run comment
  - name: test
    run: cargo test
after: true
";
    let stream = parse_lossless(input).expect("lossless parse");
    let steps = block_sequence_with_item(&stream, "build");

    let mut edit = stream.edit();
    edit.replace_sequence_item_source(
        steps.id(),
        0,
        "\
name: lint
run: cargo clippy",
    )
    .expect("replace first step");
    edit.insert_block_sequence_item_source(
        steps.id(),
        1,
        "\
name: fmt
run: cargo fmt",
    )
    .expect("insert middle step");
    edit.delete_block_sequence_item_source(steps.id(), 1)
        .expect("delete original test step");
    let output = edit.finish().expect("validated sequence edit");

    assert_eq!(
        output,
        "\
steps:
  # keep step comment
  - name: lint
    run: cargo clippy
  - name: fmt
    run: cargo fmt
after: true
"
    );
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 1);
}

#[test]
fn lossless_edit_rejects_invalid_structural_sequence_edits() {
    let stream = parse_lossless("steps:\n  - build\n").expect("lossless parse");
    let steps = block_sequence_with_item(&stream, "build");

    let missing = stream
        .edit()
        .replace_sequence_item_source(steps.id(), 1, "test")
        .expect_err("missing item");
    assert!(missing.to_string().contains("out of bounds"));

    let invalid_item = stream
        .edit()
        .insert_block_sequence_item_source(steps.id(), 1, "one\n---\ntwo")
        .expect_err("multi-document item rejected");
    assert!(invalid_item.to_string().contains("one YAML document"));

    let flow = parse_lossless("steps: [build]\n").expect("lossless parse");
    let flow_sequence = flow
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Sequence {
                    style: CollectionStyle::Flow,
                    ..
                }
            )
        })
        .expect("flow sequence");
    let error = flow
        .edit()
        .delete_block_sequence_item_source(flow_sequence.id(), 0)
        .expect_err("flow deletion rejected");
    assert!(error.to_string().contains("requires a block sequence"));

    let block_error = stream
        .edit()
        .insert_flow_sequence_item_source(steps.id(), 1, "test")
        .expect_err("block insertion through flow helper rejected");
    assert!(block_error.to_string().contains("requires a flow sequence"));
}

#[test]
fn lossless_edit_appends_block_sequence_item_after_final_line_without_trailing_newline() {
    let stream = parse_lossless("steps:\n  - build").expect("lossless parse");
    let steps = block_sequence_with_item(&stream, "build");

    let output = stream
        .insert_block_sequence_item_source(steps.id(), 1, "test")
        .expect("append final item");

    assert_eq!(output, "steps:\n  - build\n  - test\n");
}

#[test]
fn lossless_edit_structurally_updates_flow_sequence_items() {
    let input = "steps: [build, test, deploy] # keep\n";
    let stream = parse_lossless(input).expect("lossless parse");
    let steps = flow_sequence_with_item(&stream, "build");

    let mut edit = stream.edit();
    edit.insert_flow_sequence_item_source(steps.id(), 1, "fmt")
        .expect("insert flow item");
    edit.delete_flow_sequence_item_source(steps.id(), 2)
        .expect("delete flow item");
    let output = edit.finish().expect("validated flow sequence edit");

    assert_eq!(output, "steps: [build, fmt, test] # keep\n");
    let edited = parse_lossless(&output).expect("edited source reparses");
    assert_eq!(edited.comments().count(), 1);
}

#[test]
fn lossless_edit_handles_empty_and_single_item_flow_sequences() {
    let empty = parse_lossless("steps: []\n").expect("lossless parse");
    let sequence = flow_sequence_by_len(&empty, 0);
    let output = empty
        .insert_flow_sequence_item_source(sequence.id(), 0, "build")
        .expect("insert into empty flow sequence");
    assert_eq!(output, "steps: [build]\n");

    let single = parse_lossless("steps: [build]\n").expect("lossless parse");
    let sequence = flow_sequence_by_len(&single, 1);
    let output = single
        .delete_flow_sequence_item_source(sequence.id(), 0)
        .expect("delete single flow sequence item");
    assert_eq!(output, "steps: []\n");
}

#[test]
fn lossless_edit_rejects_non_scalar_scalar_replacement() {
    let stream = parse_lossless("name: api\n").expect("lossless parse");
    let scalar = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Scalar { value, .. } if value == "api"
            )
        })
        .expect("api scalar");
    let error = stream
        .edit()
        .replace_scalar_source(scalar.id(), "[worker]")
        .expect_err("sequence is not a scalar replacement");

    assert!(
        error
            .to_string()
            .contains("replacement must parse as one scalar node")
    );
}

#[test]
fn lossless_edit_rejects_overlapping_replacements() {
    let stream = parse_lossless("name: api\n").expect("lossless parse");
    let scalar = stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Scalar { value, .. } if value == "api"
            )
        })
        .expect("api scalar");
    let mut edit = stream.edit();
    edit.replace_scalar_source(scalar.id(), "worker")
        .expect("first replacement");
    edit.replace_scalar_source(scalar.id(), "web")
        .expect("second replacement");

    let error = edit.finish().expect_err("overlap is rejected");
    assert!(error.to_string().contains("lossless replacements overlap"));
}

fn flow_mapping_with_key<'a>(
    stream: &'a yaml::LosslessStream,
    key: &str,
) -> &'a yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| match node.kind() {
            LosslessNodeKind::Mapping {
                style: CollectionStyle::Flow,
                entries,
            } => entries.iter().any(|(key_id, _)| {
                stream
                    .node(*key_id)
                    .is_some_and(|key_node| matches!(key_node.kind(), LosslessNodeKind::Scalar { value, .. } if value == key))
            }),
            _ => false,
        })
        .expect("flow mapping with key")
}

fn flow_mapping_by_len(stream: &yaml::LosslessStream, len: usize) -> &yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Mapping {
                    style: CollectionStyle::Flow,
                    entries,
                } if entries.len() == len
            )
        })
        .expect("flow mapping with entry count")
}

fn block_mapping_with_key<'a>(
    stream: &'a yaml::LosslessStream,
    key: &str,
) -> &'a yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| match node.kind() {
            LosslessNodeKind::Mapping {
                style: CollectionStyle::Block,
                entries,
            } => entries.iter().any(|(key_id, _)| {
                stream
                    .node(*key_id)
                    .is_some_and(|key_node| matches!(key_node.kind(), LosslessNodeKind::Scalar { value, .. } if value == key))
            }),
            _ => false,
        })
        .expect("block mapping with key")
}

fn block_sequence_with_item<'a>(
    stream: &'a yaml::LosslessStream,
    value: &str,
) -> &'a yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| match node.kind() {
            LosslessNodeKind::Sequence {
                style: CollectionStyle::Block,
                children,
            } => children.iter().any(|child| {
                stream.node(*child).is_some_and(|child_node| {
                    sequence_node_contains_scalar_value(stream, child_node, value)
                })
            }),
            _ => false,
        })
        .expect("block sequence with item")
}

fn flow_sequence_with_item<'a>(
    stream: &'a yaml::LosslessStream,
    value: &str,
) -> &'a yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| match node.kind() {
            LosslessNodeKind::Sequence {
                style: CollectionStyle::Flow,
                children,
            } => children.iter().any(|child| {
                stream.node(*child).is_some_and(|child_node| {
                    sequence_node_contains_scalar_value(stream, child_node, value)
                })
            }),
            _ => false,
        })
        .expect("flow sequence with item")
}

fn flow_sequence_by_len(stream: &yaml::LosslessStream, len: usize) -> &yaml::LosslessNode {
    stream
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.kind(),
                LosslessNodeKind::Sequence {
                    style: CollectionStyle::Flow,
                    children,
                } if children.len() == len
            )
        })
        .expect("flow sequence with item count")
}

fn sequence_node_contains_scalar_value(
    stream: &yaml::LosslessStream,
    node: &yaml::LosslessNode,
    value: &str,
) -> bool {
    match node.kind() {
        LosslessNodeKind::Scalar { value: scalar, .. } => scalar == value,
        LosslessNodeKind::Sequence { children, .. } => children.iter().any(|child| {
            stream
                .node(*child)
                .is_some_and(|node| sequence_node_contains_scalar_value(stream, node, value))
        }),
        LosslessNodeKind::Mapping { entries, .. } => entries.iter().any(|(key, value_id)| {
            [*key, *value_id].into_iter().any(|node_id| {
                stream
                    .node(node_id)
                    .is_some_and(|node| sequence_node_contains_scalar_value(stream, node, value))
            })
        }),
        LosslessNodeKind::Alias { .. } => false,
    }
}

#[test]
fn lossless_resolve_path_navigates_mappings_and_sequences() {
    let input = "\
services:
  db:
    image: mariadb:10-focal
    ports:
      - 3306
      - 33060
";
    let stream = parse_lossless(input).expect("lossless parse");

    let image = stream
        .resolve_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("db"),
                PathSegment::from("image"),
            ],
        )
        .expect("resolve services.db.image");
    assert!(matches!(
        stream.node(image).expect("image node").kind(),
        LosslessNodeKind::Scalar { value, .. } if value == "mariadb:10-focal"
    ));

    let second_port = stream
        .resolve_path(
            0,
            &[
                PathSegment::Key("services".to_owned()),
                PathSegment::Key("db".to_owned()),
                PathSegment::Key("ports".to_owned()),
                PathSegment::Index(1),
            ],
        )
        .expect("resolve services.db.ports[1]");
    assert!(matches!(
        stream.node(second_port).expect("port node").kind(),
        LosslessNodeKind::Scalar { value, .. } if value == "33060"
    ));

    // An empty path returns the document root.
    let root = stream.resolve_path(0, &[]).expect("resolve root");
    assert_eq!(root, stream.documents()[0].root().expect("doc root"));
}

#[test]
fn lossless_replace_value_at_path_edits_addressed_node() {
    let input = "\
services:
  db:
    image: mariadb:10-focal
";
    let stream = parse_lossless(input).expect("lossless parse");
    let output = stream
        .replace_value_at_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("db"),
                PathSegment::from("image"),
            ],
            "mariadb:11-focal",
        )
        .expect("replace addressed value");
    assert_eq!(output, "services:\n  db:\n    image: mariadb:11-focal\n");
}

#[test]
fn lossless_resolve_path_reports_failing_segment() {
    let input = "\
services:
  db:
    image: mariadb:10-focal
    ports:
      - 3306
ref: &dup 1
copy: *dup
dup_keys:
  a: 1
  a: 2
";
    let stream = parse_lossless(input).expect("lossless parse");

    let missing = stream
        .resolve_path(
            0,
            &[PathSegment::from("services"), PathSegment::from("web")],
        )
        .expect_err("missing key");
    assert!(missing.to_string().contains("key \"web\" was not found"));

    let out_of_bounds = stream
        .resolve_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("db"),
                PathSegment::from("ports"),
                PathSegment::from(5usize),
            ],
        )
        .expect_err("index out of bounds");
    assert!(out_of_bounds.to_string().contains("out of bounds"));

    let key_on_scalar = stream
        .resolve_path(
            0,
            &[
                PathSegment::from("services"),
                PathSegment::from("db"),
                PathSegment::from("image"),
                PathSegment::from("nope"),
            ],
        )
        .expect_err("key into scalar");
    assert!(
        key_on_scalar
            .to_string()
            .contains("requires a mapping node")
    );

    let index_on_mapping = stream
        .resolve_path(
            0,
            &[PathSegment::from("services"), PathSegment::from(0usize)],
        )
        .expect_err("index into mapping");
    assert!(
        index_on_mapping
            .to_string()
            .contains("requires a sequence node")
    );

    // Aliases are not followed: stepping into an alias node is an error rather
    // than a silent traversal of the shared anchor.
    let through_alias = stream
        .resolve_path(
            0,
            &[PathSegment::from("copy"), PathSegment::from("anything")],
        )
        .expect_err("alias not followed");
    assert!(
        through_alias
            .to_string()
            .contains("requires a mapping node")
    );

    let ambiguous = stream
        .resolve_path(0, &[PathSegment::from("dup_keys"), PathSegment::from("a")])
        .expect_err("ambiguous duplicate key");
    assert!(ambiguous.to_string().contains("is ambiguous"));

    let bad_document = stream
        .resolve_path(2, &[PathSegment::from("services")])
        .expect_err("document out of range");
    assert!(bad_document.to_string().contains("out of range"));
}

#[test]
fn lossless_path_addressed_insert_and_delete_dispatch_by_style() {
    let input = "\
block:
  a: 1
  b: 2
flow_map: {x: 1}
block_seq:
  - one
  - two
flow_seq: [p, q]
";
    let stream = parse_lossless(input).expect("lossless parse");

    // Deletes: the block/flow style is detected from the resolved parent.
    let del_block_entry = stream
        .delete_at_path(0, &["block".into(), "b".into()])
        .expect("delete block mapping entry");
    assert!(del_block_entry.contains("block:\n  a: 1\nflow_map:"));

    let del_flow_entry = stream
        .delete_at_path(0, &["flow_map".into(), "x".into()])
        .expect("delete flow mapping entry");
    assert!(del_flow_entry.contains("flow_map: {}"));

    let del_block_item = stream
        .delete_at_path(0, &["block_seq".into(), 0usize.into()])
        .expect("delete block sequence item");
    assert!(del_block_item.contains("block_seq:\n  - two\n"));

    let del_flow_item = stream
        .delete_at_path(0, &["flow_seq".into(), 1usize.into()])
        .expect("delete flow sequence item");
    assert!(del_flow_item.contains("flow_seq: [p]"));

    // Inserts: one method each for mappings and sequences, style auto-detected.
    let ins_block_entry = stream
        .insert_entry_at_path(0, &["block".into()], "c: 3")
        .expect("insert block mapping entry");
    assert!(ins_block_entry.contains("  a: 1\n  b: 2\n  c: 3\n"));

    let ins_flow_entry = stream
        .insert_entry_at_path(0, &["flow_map".into()], "y: 2")
        .expect("insert flow mapping entry");
    assert!(ins_flow_entry.contains("flow_map: {x: 1, y: 2}"));

    let ins_block_item = stream
        .insert_item_at_path(0, &["block_seq".into()], 1, "mid")
        .expect("insert block sequence item");
    assert!(ins_block_item.contains("  - one\n  - mid\n  - two\n"));

    let ins_flow_item = stream
        .insert_item_at_path(0, &["flow_seq".into()], 2, "r")
        .expect("insert flow sequence item");
    assert!(ins_flow_item.contains("flow_seq: [p, q, r]"));
}

#[test]
fn lossless_path_addressed_mutations_reject_mismatched_targets() {
    let stream = parse_lossless("m:\n  a: 1\nseq:\n  - one\n").expect("lossless parse");

    let empty = stream
        .delete_at_path(0, &[])
        .expect_err("empty delete path");
    assert!(empty.to_string().contains("non-empty path"));

    let index_on_mapping = stream
        .delete_at_path(0, &["m".into(), 0usize.into()])
        .expect_err("index delete on mapping parent");
    assert!(
        index_on_mapping
            .to_string()
            .contains("requires a sequence parent")
    );

    let key_on_sequence = stream
        .delete_at_path(0, &["seq".into(), "a".into()])
        .expect_err("key delete on sequence parent");
    assert!(
        key_on_sequence
            .to_string()
            .contains("requires a mapping parent")
    );

    let entry_into_sequence = stream
        .insert_entry_at_path(0, &["seq".into()], "a: 1")
        .expect_err("entry insert into sequence");
    assert!(
        entry_into_sequence
            .to_string()
            .contains("requires a mapping node")
    );

    let item_into_mapping = stream
        .insert_item_at_path(0, &["m".into()], 0, "x")
        .expect_err("item insert into mapping");
    assert!(
        item_into_mapping
            .to_string()
            .contains("requires a sequence node")
    );
}
