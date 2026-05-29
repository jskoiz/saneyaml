use yaml::{
    CollectionStyle, LosslessNodeKind, LosslessTriviaKind, ScalarStyle, parse_lossless,
    parse_lossless_bytes,
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
