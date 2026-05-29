#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::{CollectionStyle, Error, LosslessNodeKind, LosslessStream, NodeId, Span};

const REPLACEMENT_MARKER: &[u8] = b"=== yaml replacement ===\n";

fuzz_target!(|input: &[u8]| {
    assert_lossless_edit_invariants(input);
});

fn assert_lossless_edit_invariants(input: &[u8]) {
    let Some(edit_input) = split_edit_input(input) else {
        return;
    };

    let stream = match yaml::parse_lossless_bytes(edit_input.source) {
        Ok(stream) => stream,
        Err(error) => {
            assert_error_invariants_allowing_unspanned(edit_input.source, &error);
            return;
        }
    };

    if assert_structural_lossless_edit_invariants(&stream, edit_input) {
        return;
    }

    let Some(target) = select_target(&stream, edit_input) else {
        return;
    };
    let replacement = match edit_input.mode {
        EditMode::Delete => "",
        _ => edit_input.replacement,
    };
    let edited = edited_source(stream.as_source(), target.span, replacement)
        .expect("lossless node spans are valid source slices");

    let mut edit = stream.edit();
    let replace_result = match edit_input.mode {
        EditMode::Node => edit.replace_node_source(target.node.expect("node target"), replacement),
        EditMode::Scalar => {
            edit.replace_scalar_source(target.node.expect("scalar target"), replacement)
        }
        EditMode::Source => edit.replace_source_span(target.span, replacement),
        EditMode::Insert => edit.insert_source(target.span.start, replacement),
        EditMode::Delete => edit.delete_source_span(target.span),
        EditMode::MappingValue
        | EditMode::MappingInsert
        | EditMode::MappingDelete
        | EditMode::SequenceItem
        | EditMode::SequenceInsert
        | EditMode::SequenceDelete => {
            unreachable!("structural lossless edit modes are handled before raw edit dispatch")
        }
    };
    if let Err(error) = replace_result {
        assert_error_invariants_allowing_unspanned(edit_input.source, &error);
        return;
    }

    match edit.finish() {
        Ok(output) => {
            assert_eq!(output, edited);
            yaml::parse_lossless(&output).expect("successful edit output reparses losslessly");
        }
        Err(error) => assert_error_invariants_allowing_unspanned(edited.as_bytes(), &error),
    }
}

fn assert_structural_lossless_edit_invariants(
    stream: &LosslessStream,
    edit_input: EditInput<'_>,
) -> bool {
    let mut edit = stream.edit();
    let result = match edit_input.mode {
        EditMode::MappingValue => {
            let Some((mapping, key)) =
                select_scalar_keyed_mapping(stream, edit_input.selector, false)
            else {
                return true;
            };
            edit.replace_mapping_value_source(mapping, &key, edit_input.replacement)
        }
        EditMode::MappingInsert => {
            let Some(mapping) = select_mapping_insertion(stream, edit_input.selector) else {
                return true;
            };
            match mapping_style(stream, mapping) {
                Some(CollectionStyle::Block) => {
                    edit.insert_block_mapping_entry_source(mapping, edit_input.replacement)
                }
                Some(CollectionStyle::Flow) => {
                    edit.insert_flow_mapping_entry_source(mapping, edit_input.replacement)
                }
                None => return true,
            }
        }
        EditMode::MappingDelete => {
            let Some((mapping, key)) =
                select_scalar_keyed_mapping(stream, edit_input.selector, false)
            else {
                return true;
            };
            match mapping_style(stream, mapping) {
                Some(CollectionStyle::Block) => {
                    edit.delete_block_mapping_entry_source(mapping, &key)
                }
                Some(CollectionStyle::Flow) => edit.delete_flow_mapping_entry_source(mapping, &key),
                None => return true,
            }
        }
        EditMode::SequenceItem => {
            let Some((sequence, index)) = select_sequence_item(stream, edit_input.selector, false)
            else {
                return true;
            };
            edit.replace_sequence_item_source(sequence, index, edit_input.replacement)
        }
        EditMode::SequenceInsert => {
            let Some((sequence, index)) = select_sequence_insertion(stream, edit_input.selector)
            else {
                return true;
            };
            match sequence_style(stream, sequence) {
                Some(CollectionStyle::Block) => {
                    edit.insert_block_sequence_item_source(sequence, index, edit_input.replacement)
                }
                Some(CollectionStyle::Flow) => {
                    edit.insert_flow_sequence_item_source(sequence, index, edit_input.replacement)
                }
                None => return true,
            }
        }
        EditMode::SequenceDelete => {
            let Some((sequence, index)) = select_sequence_item(stream, edit_input.selector, false)
            else {
                return true;
            };
            match sequence_style(stream, sequence) {
                Some(CollectionStyle::Block) => {
                    edit.delete_block_sequence_item_source(sequence, index)
                }
                Some(CollectionStyle::Flow) => {
                    edit.delete_flow_sequence_item_source(sequence, index)
                }
                None => return true,
            }
        }
        _ => return false,
    };

    if let Err(error) = result {
        assert_error_invariants_allowing_unspanned(edit_input.source, &error);
        return true;
    }

    match edit.finish() {
        Ok(output) => {
            yaml::parse_lossless(&output).expect("structural edit output reparses losslessly");
        }
        Err(error) => {
            assert!(!error.to_string().is_empty());
        }
    }
    true
}

#[derive(Clone, Copy)]
struct EditInput<'a> {
    mode: EditMode,
    selector: usize,
    source: &'a [u8],
    replacement: &'a str,
}

#[derive(Clone, Copy)]
enum EditMode {
    Node,
    Scalar,
    Source,
    Insert,
    Delete,
    MappingValue,
    MappingInsert,
    MappingDelete,
    SequenceItem,
    SequenceInsert,
    SequenceDelete,
}

#[derive(Clone, Copy)]
struct EditTarget {
    node: Option<NodeId>,
    span: Span,
}

fn split_edit_input(input: &[u8]) -> Option<EditInput<'_>> {
    let line_end = input.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&input[..line_end]).ok()?;
    let body = &input[line_end + 1..];
    let split = find_subslice(body, REPLACEMENT_MARKER)?;
    let source = &body[..split];
    let replacement = std::str::from_utf8(&body[split + REPLACEMENT_MARKER.len()..]).ok()?;

    Some(EditInput {
        mode: if header.contains("mode=scalar") {
            EditMode::Scalar
        } else if header.contains("mode=map-replace") {
            EditMode::MappingValue
        } else if header.contains("mode=map-insert") {
            EditMode::MappingInsert
        } else if header.contains("mode=map-delete") {
            EditMode::MappingDelete
        } else if header.contains("mode=seq-replace") {
            EditMode::SequenceItem
        } else if header.contains("mode=seq-insert") {
            EditMode::SequenceInsert
        } else if header.contains("mode=seq-delete") {
            EditMode::SequenceDelete
        } else if header.contains("mode=source") {
            EditMode::Source
        } else if header.contains("mode=insert") {
            EditMode::Insert
        } else if header.contains("mode=delete") {
            EditMode::Delete
        } else {
            EditMode::Node
        },
        selector: selector_from_header(header),
        source,
        replacement,
    })
}

fn selector_from_header(header: &str) -> usize {
    for field in header.split_whitespace() {
        if let Some(value) = field.strip_prefix("index=")
            && let Ok(index) = value.parse()
        {
            return index;
        }
    }

    header.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(33).wrapping_add(byte as usize)
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn select_target(stream: &yaml::LosslessStream, input: EditInput<'_>) -> Option<EditTarget> {
    match input.mode {
        EditMode::Node | EditMode::Source | EditMode::Delete => {
            if stream.nodes().is_empty() {
                return None;
            }
            let node = stream.nodes().get(input.selector % stream.nodes().len())?;
            Some(EditTarget {
                node: Some(node.id()),
                span: node.span(),
            })
        }
        EditMode::Scalar => {
            let scalars = stream
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind(), LosslessNodeKind::Scalar { .. }))
                .collect::<Vec<_>>();
            if scalars.is_empty() {
                return None;
            }
            let node = scalars.get(input.selector % scalars.len())?;
            Some(EditTarget {
                node: Some(node.id()),
                span: node.span(),
            })
        }
        EditMode::Insert => {
            let offset = input.selector % (stream.as_source().len() + 1);
            let span = stream.source_span(offset, offset).ok()?;
            Some(EditTarget { node: None, span })
        }
        EditMode::MappingValue
        | EditMode::MappingInsert
        | EditMode::MappingDelete
        | EditMode::SequenceItem
        | EditMode::SequenceInsert
        | EditMode::SequenceDelete => None,
    }
}

fn select_scalar_keyed_mapping(
    stream: &LosslessStream,
    selector: usize,
    block_only: bool,
) -> Option<(NodeId, String)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Mapping { style, entries }
                if !block_only || *style == CollectionStyle::Block =>
            {
                Some((node.id(), entries))
            }
            _ => None,
        })
        .flat_map(|(mapping, entries)| {
            entries.iter().filter_map(move |(key, _)| {
                stream.node(*key).and_then(|node| match node.kind() {
                    LosslessNodeKind::Scalar { value, .. } => Some((mapping, value.clone())),
                    _ => None,
                })
            })
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    candidates.get(selector % candidates.len()).cloned()
}

fn select_mapping_insertion(stream: &LosslessStream, selector: usize) -> Option<NodeId> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Mapping { .. } => Some(node.id()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    candidates.get(selector % candidates.len()).copied()
}

fn select_sequence_item(
    stream: &LosslessStream,
    selector: usize,
    block_only: bool,
) -> Option<(NodeId, usize)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence { style, children }
                if (!block_only || *style == CollectionStyle::Block) && !children.is_empty() =>
            {
                Some((node.id(), children.len()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let (sequence, len) = candidates.get(selector % candidates.len()).copied()?;
    Some((sequence, selector % len))
}

fn select_sequence_insertion(stream: &LosslessStream, selector: usize) -> Option<(NodeId, usize)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence { children, .. } => Some((node.id(), children.len())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let (sequence, len) = candidates.get(selector % candidates.len()).copied()?;
    Some((sequence, selector % (len + 1)))
}

fn mapping_style(stream: &LosslessStream, mapping: NodeId) -> Option<CollectionStyle> {
    match stream.node(mapping)?.kind() {
        LosslessNodeKind::Mapping { style, .. } => Some(*style),
        _ => None,
    }
}

fn sequence_style(stream: &LosslessStream, sequence: NodeId) -> Option<CollectionStyle> {
    match stream.node(sequence)?.kind() {
        LosslessNodeKind::Sequence { style, .. } => Some(*style),
        _ => None,
    }
}

fn edited_source(source: &str, span: Span, replacement: &str) -> Option<String> {
    let prefix = source.get(..span.start)?;
    let suffix = source.get(span.end..)?;
    Some([prefix, replacement, suffix].concat())
}

fn assert_error_invariants_allowing_unspanned(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    assert_span_invariants_allowing_default(input, diagnostic.span);
    for related in &diagnostic.related {
        assert!(!related.message.is_empty());
        assert_span_invariants_allowing_default(input, related.span);
    }
}

fn assert_span_invariants_allowing_default(input: &[u8], span: Span) {
    if span == Span::default() {
        return;
    }
    assert!(
        span.start <= span.end,
        "span starts after it ends: {span:?}"
    );
    assert!(
        span.end <= input.len(),
        "span exceeds input length {}: {span:?}",
        input.len()
    );
    assert!(span.line >= 1, "span line must be one-based: {span:?}");
    assert!(span.column >= 1, "span column must be one-based: {span:?}");
}
