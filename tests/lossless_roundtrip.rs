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
