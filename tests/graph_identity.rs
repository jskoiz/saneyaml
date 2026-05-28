use yaml::{LosslessNodeKind, parse_lossless};

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
