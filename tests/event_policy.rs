use yaml::{CollectionStyle, Event, ScalarStyle, parse_events, parse_str};

fn scalar_events(events: &[Event]) -> Vec<(&str, ScalarStyle, &yaml::EventMeta, yaml::Span)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar {
                value,
                style,
                meta,
                span,
            } => Some((value.as_str(), *style, meta, *span)),
            _ => None,
        })
        .collect()
}

fn event_source(input: &str, span: yaml::Span) -> &str {
    &input[span.start..span.end]
}

#[test]
fn event_policy_stream_is_structurally_balanced_for_supported_yaml() {
    let events =
        parse_events("root:\n  items:\n    - name: one\n    - name: two\n  enabled: true\n")
            .expect("events");
    assert!(matches!(events.first(), Some(Event::StreamStart)));
    assert!(matches!(events.last(), Some(Event::StreamEnd)));

    let mut docs = 0i32;
    let mut seqs = 0i32;
    let mut maps = 0i32;
    for event in events {
        match event {
            Event::DocumentStart { .. } => docs += 1,
            Event::DocumentEnd { .. } => docs -= 1,
            Event::SequenceStart { .. } => seqs += 1,
            Event::SequenceEnd { .. } => seqs -= 1,
            Event::MappingStart { .. } => maps += 1,
            Event::MappingEnd { .. } => maps -= 1,
            Event::StreamStart | Event::StreamEnd | Event::Alias { .. } | Event::Scalar { .. } => {}
        }
        assert!(docs >= 0);
        assert!(seqs >= 0);
        assert!(maps >= 0);
    }
    assert_eq!(docs, 0);
    assert_eq!(seqs, 0);
    assert_eq!(maps, 0);
}

#[test]
fn event_policy_raw_events_preserve_scalar_styles() {
    let events =
        parse_events("- plain\n- 'single'\n- \"double\"\n- |-\n  literal\n- >-\n  folded\n")
            .expect("events");
    let scalars = scalar_events(&events);
    let styles = scalars
        .iter()
        .map(|(_, style, _, _)| *style)
        .collect::<Vec<_>>();

    assert_eq!(
        styles,
        [
            ScalarStyle::Plain,
            ScalarStyle::SingleQuoted,
            ScalarStyle::DoubleQuoted,
            ScalarStyle::Literal,
            ScalarStyle::Folded,
        ]
    );
    assert_eq!(scalars[3].0, "literal");
    assert_eq!(scalars[4].0, "folded");
}

#[test]
fn event_policy_raw_events_preserve_collection_styles() {
    let events = parse_events("block:\n  - one\nflow: [a: b, {c: d}]\n").expect("events");
    let starts = events
        .iter()
        .filter_map(|event| match event {
            Event::SequenceStart { style, .. } => Some(("sequence", *style)),
            Event::MappingStart { style, .. } => Some(("mapping", *style)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        starts,
        [
            ("mapping", CollectionStyle::Block),
            ("sequence", CollectionStyle::Block),
            ("sequence", CollectionStyle::Flow),
            ("mapping", CollectionStyle::Flow),
            ("mapping", CollectionStyle::Flow),
        ]
    );
}

#[test]
fn event_policy_raw_events_preserve_yaml_double_quoted_escape_values() {
    let events = parse_events("root: [\"\\e\", \"\\N\", \"\\L\", \"\\P\"]\n").expect("events");
    let scalars = scalar_events(&events);
    let double_values = scalars
        .iter()
        .filter_map(|(value, style, _, _)| (*style == ScalarStyle::DoubleQuoted).then_some(*value))
        .collect::<Vec<_>>();

    assert_eq!(
        double_values,
        ["\u{001B}", "\u{0085}", "\u{2028}", "\u{2029}"]
    );
}

#[test]
fn event_policy_raw_events_expose_tag_metadata() {
    let node = parse_str("value: !Thing tagged\n").expect("tagged tree");
    let yaml::NodeValue::Mapping(entries) = &node.value else {
        panic!("expected mapping");
    };
    assert!(matches!(&entries[0].1.value, yaml::NodeValue::Tagged(_)));

    let events = parse_events("value: !Thing tagged\n").expect("events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                meta,
                ..
            } if value == "tagged"
                && meta.tag.as_ref().is_some_and(|tag| tag.tag == yaml::Tag::new("Thing"))
        )
    }));
}

#[test]
fn event_policy_raw_events_expose_anchors_and_aliases_without_expanding() {
    let events = parse_events("root: &root\n  child: 1\nref: *root\n").expect("events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "root")
        )
    }));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Alias { anchor } if anchor.name == "root"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::MappingStart { .. }))
            .count(),
        2,
        "raw events expose alias references without emitting an expanded mapping"
    );
}

#[test]
fn event_policy_raw_events_accept_aliases_as_distinct_block_mapping_keys() {
    let input = "a: &a one\nb: &b two\n? *a\n: first\n? *b\n: second\n";
    yaml::parse_documents(input).expect("tree parser accepts scalar alias keys");

    let events = parse_events(input).expect("raw events");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Alias { anchor } if anchor.name == "a" || anchor.name == "b"))
            .count(),
        2
    );
}

#[test]
fn event_policy_raw_events_accept_aliases_as_distinct_flow_mapping_keys() {
    let input = "a: &a one\nb: &b two\nroot: {? *a : first, ? *b : second}\n";
    yaml::parse_documents(input).expect("tree parser accepts scalar alias keys");

    let events = parse_events(input).expect("raw events");
    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some(anchor.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a", "b"]);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "one" || value == "two"))
            .count(),
        2,
        "anchor definitions are emitted once; alias key events do not expand scalar values"
    );
}

#[test]
fn event_policy_raw_events_do_not_reject_duplicate_tree_keys() {
    let block_input = "dup: one\ndup: two\n";
    yaml::parse_documents(block_input).expect_err("tree parser rejects duplicate keys");
    let block_events = parse_events(block_input).expect("raw block events");
    assert_eq!(
        block_events
            .iter()
            .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "dup"))
            .count(),
        2
    );

    let flow_input = "root: {dup: one, dup: two}\n";
    yaml::parse_documents(flow_input).expect_err("tree parser rejects duplicate flow keys");
    let flow_events = parse_events(flow_input).expect("raw flow events");
    assert_eq!(
        flow_events
            .iter()
            .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "dup"))
            .count(),
        2
    );

    let collection_input = "root: {? [a, b]: first, ? [a, b]: second}\n";
    yaml::parse_documents(collection_input)
        .expect_err("tree parser rejects duplicate collection keys");
    let collection_events = parse_events(collection_input).expect("raw collection-key events");
    assert!(
        collection_events
            .iter()
            .filter(|event| matches!(event, Event::SequenceStart { .. }))
            .count()
            >= 2,
        "raw events expose both sequence keys"
    );

    let tagged_input = "root: {!Tag dup: first, dup: second}\n";
    yaml::parse_documents(tagged_input).expect_err("tree parser rejects tagged duplicate keys");
    let tagged_events = parse_events(tagged_input).expect("raw tagged duplicate-key events");
    assert_eq!(
        tagged_events
            .iter()
            .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "dup"))
            .count(),
        2
    );
}

#[test]
fn event_policy_raw_events_allow_recursive_aliases_without_expanding() {
    let input = "root: &root [*root]\n";
    yaml::parse_documents(input).expect_err("tree parser rejects recursive alias expansion");

    let events = parse_events(input).expect("raw recursive alias events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "root")
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, Event::Alias { anchor } if anchor.name == "root") })
    );
}

#[test]
fn event_policy_raw_events_reject_unknown_aliases() {
    let block_error = parse_events("ref: *missing\n").expect_err("unknown block alias");
    assert!(block_error.to_string().contains("unknown anchor `missing`"));

    let flow_error = parse_events("root: [*missing]\n").expect_err("unknown flow alias");
    assert!(flow_error.to_string().contains("unknown anchor `missing`"));
}

#[test]
fn event_policy_raw_events_expose_flow_key_metadata() {
    let input = "\
%TAG !e! tag:example.com,2026:
---
scalar: &scalar scalar-key
root: {&direct direct-key: v, ? *scalar : alias-v, ? &seq [a, b] : seq-v, !e!Thing tagged-key: tagged-v}
";
    yaml::parse_documents(input).expect("tree parser accepts flow key metadata");

    let events = parse_events(input).expect("raw events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                ..
            } if value == "direct-key"
                && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "direct")
        )
    }));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Alias { anchor } if anchor.name == "scalar"))
            .count(),
        1
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "seq")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                ..
            } if value == "tagged-key"
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.handle == "!" && tag.tag.suffix == "tag:example.com,2026:Thing"
                })
        )
    }));
}

#[test]
fn event_policy_raw_events_expose_anchor_only_flow_nodes_as_null_scalars() {
    let input = "root: [&empty, *empty]\nkeyed: {? &key : value}\n";
    parse_str(input).expect("tree parser accepts anchor-only flow nodes");
    let events = parse_events(input).expect("events for anchor-only flow nodes");

    let anchored_nulls = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::Scalar {
                    value,
                    meta,
                    ..
                } if value == "null"
                    && meta.anchor.as_ref().is_some_and(|anchor| {
                        anchor.name == "empty" || anchor.name == "key"
                    })
            )
        })
        .count();
    assert_eq!(anchored_nulls, 2);
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, Event::Alias { anchor } if anchor.name == "empty") })
    );
}

#[test]
fn event_policy_raw_events_preserve_colon_anchor_and_alias_names() {
    let input = "root: [&a:, *a:]\n";
    parse_str(input).expect("tree parser accepts colon anchor names");
    let events = parse_events(input).expect("events for colon anchor names");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a:")
        )
    }));
    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some(anchor.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a:"]);
}

#[test]
fn event_policy_raw_events_track_document_boundary_explicitness() {
    let implicit = parse_events("a: b\n").expect("implicit events");
    assert!(matches!(
        implicit.get(1),
        Some(Event::DocumentStart {
            explicit: false,
            ..
        })
    ));
    assert!(matches!(
        implicit.iter().rev().nth(1),
        Some(Event::DocumentEnd {
            explicit: false,
            ..
        })
    ));

    let explicit = parse_events("---\na: b\n...\n").expect("explicit events");
    assert!(matches!(
        explicit.get(1),
        Some(Event::DocumentStart { explicit: true, .. })
    ));
    assert!(matches!(
        explicit.iter().rev().nth(1),
        Some(Event::DocumentEnd { explicit: true, .. })
    ));
}

#[test]
fn event_policy_document_start_exposes_directive_metadata() {
    let events = parse_events("%YAML 1.2\n%TAG !e! tag:example.com,2026:\n---\n!e!Thing value\n")
        .expect("events");

    let Some(Event::DocumentStart {
        explicit: true,
        directives,
        ..
    }) = events.get(1)
    else {
        panic!("expected explicit document start");
    };

    let version = directives
        .yaml_version
        .as_ref()
        .expect("YAML directive metadata");
    assert_eq!((version.major, version.minor), (1, 2));
    assert_eq!(version.span.line, 1);

    assert_eq!(directives.tag_directives.len(), 1);
    assert_eq!(directives.tag_directives[0].handle, "!e!");
    assert_eq!(directives.tag_directives[0].prefix, "tag:example.com,2026:");
    assert_eq!(directives.tag_directives[0].handle_span.line, 2);
    assert_eq!(directives.tag_directives[0].prefix_span.line, 2);

    let implicit = parse_events("a: b\n").expect("implicit events");
    let Some(Event::DocumentStart { directives, .. }) = implicit.get(1) else {
        panic!("expected implicit document start");
    };
    assert!(directives.yaml_version.is_none());
    assert!(directives.tag_directives.is_empty());
}

#[test]
fn event_policy_document_start_properties_attach_to_root_node_event() {
    let events = parse_events("--- &root !Thing\nchild: 1\n...\n").expect("events");

    assert!(matches!(
        events.get(1),
        Some(Event::DocumentStart { explicit: true, .. })
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "root")
                    && meta.tag.as_ref().is_some_and(|tag| tag.tag == yaml::Tag::new("Thing"))
        )
    }));
}

#[test]
fn event_policy_tag_directives_resolve_handles_for_flow_collections() {
    let events =
        parse_events("%TAG !e! tag:example.com,2026:\n---\nvalue: !e!Thing [x]\n").expect("events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.handle == "!" && tag.tag.suffix == "tag:example.com,2026:Thing"
                })
        )
    }));
}

#[test]
fn event_policy_verbatim_tags_attach_to_scalar_and_collection_events() {
    let input =
        "scalar: !<tag:example.com,2026:Scalar> value\nseq: !<tag:example.com,2026:Seq> [x]\n";
    let events = parse_events(input).expect("events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                span,
                ..
            } if value == "value"
                && event_source(input, *span) == "value"
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.suffix == "tag:example.com,2026:Scalar"
                        && event_source(input, tag.span)
                            == "!<tag:example.com,2026:Scalar>"
                })
        )
    }));

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.suffix == "tag:example.com,2026:Seq"
                        && event_source(input, tag.span) == "!<tag:example.com,2026:Seq>"
                })
        )
    }));
}

#[test]
fn event_policy_rejects_undeclared_named_tag_handles() {
    for input in [
        "!h!Thing value\n",
        "root: !h!Thing value\n",
        "root: [!h!Thing value]\n",
    ] {
        let error = parse_events(input).expect_err("undeclared handle rejected");
        assert!(
            error
                .to_string()
                .contains("undeclared TAG directive handle"),
            "unexpected error: {error}"
        );
    }
}
