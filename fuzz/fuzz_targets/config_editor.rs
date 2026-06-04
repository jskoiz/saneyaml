#![no_main]

use libfuzzer_sys::fuzz_target;
use saneyaml::{CollectionStyle, ConfigPath, LosslessNodeKind, LosslessStream, PathSegment};
use serde::Serialize;
use std::collections::BTreeMap;

const VALUE_MARKER: &[u8] = b"=== config value ===\n";

fuzz_target!(|input: &[u8]| {
    assert_config_editor_invariants(input);
});

fn assert_config_editor_invariants(input: &[u8]) {
    let Some(edit_input) = split_edit_input(input) else {
        return;
    };

    let stream = match saneyaml::parse_lossless_bytes(edit_input.source) {
        Ok(stream) => stream,
        Err(error) => {
            assert_error_invariants(edit_input.source, &error);
            return;
        }
    };
    let source = stream.as_source().to_owned();
    let mut editor = saneyaml::edit(source.clone()).expect("parsed source opens in ConfigEditor");

    let applied = apply_edit_plan(&mut editor, &stream, edit_input)
        .expect("selected ConfigEditor operation succeeds");
    if !applied {
        return;
    }

    let output = editor.finish().expect("successful edit validates");
    let edited = saneyaml::parse_lossless(&output).expect("edited config reparses losslessly");
    assert_eq!(edited.as_source(), output);
}

#[derive(Clone, Copy)]
struct EditInput<'a> {
    header: &'a str,
    selector: usize,
    source: &'a [u8],
    payload: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditMode {
    Set,
    Remove,
    Rename,
    Insert,
    Push,
    InsertItem,
    Chain,
}

#[derive(Clone)]
struct Candidate {
    document: usize,
    path: Vec<PathSegment>,
    kind: CandidateKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CandidateKind {
    Any,
    Mapping { can_insert: bool },
    Sequence { len: usize, can_insert: bool },
    MappingEntry,
    SequenceItem,
}

#[derive(Serialize)]
#[serde(untagged)]
enum EditValue {
    Scalar(String),
    Sequence(Vec<String>),
    Mapping(BTreeMap<String, String>),
    Nested(BTreeMap<String, Vec<String>>),
    Multiline(String),
}

fn apply_edit_plan(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    match mode_from_header(input.header, input.selector) {
        EditMode::Set => apply_set(editor, stream, input),
        EditMode::Remove => apply_remove(editor, stream, input),
        EditMode::Rename => apply_rename(editor, stream, input),
        EditMode::Insert => apply_insert(editor, stream, input),
        EditMode::Push => apply_push(editor, stream, input),
        EditMode::InsertItem => apply_insert_item(editor, stream, input),
        EditMode::Chain => apply_chain(editor, stream, input),
    }
}

fn apply_set(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream);
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.set_in_document(
        candidate.document,
        path,
        edit_value(input.header, input.payload),
    )?;
    Ok(true)
}

fn apply_remove(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream)
        .into_iter()
        .filter(|candidate| {
            matches!(
                candidate.kind,
                CandidateKind::MappingEntry | CandidateKind::SequenceItem
            )
        })
        .collect::<Vec<_>>();
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.remove_in_document(candidate.document, path)?;
    Ok(true)
}

fn apply_rename(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream)
        .into_iter()
        .filter(|candidate| candidate.kind == CandidateKind::MappingEntry)
        .collect::<Vec<_>>();
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.rename_in_document(candidate.document, path, edit_key(input.payload))?;
    Ok(true)
}

fn apply_insert(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream)
        .into_iter()
        .filter(|candidate| matches!(candidate.kind, CandidateKind::Mapping { can_insert: true }))
        .collect::<Vec<_>>();
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.insert_in_document(
        candidate.document,
        path,
        edit_key(input.payload),
        edit_value(input.header, input.payload),
    )?;
    Ok(true)
}

fn apply_push(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream)
        .into_iter()
        .filter(|candidate| {
            matches!(
                candidate.kind,
                CandidateKind::Sequence {
                    can_insert: true,
                    ..
                }
            )
        })
        .collect::<Vec<_>>();
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.push_in_document(
        candidate.document,
        path,
        edit_value(input.header, input.payload),
    )?;
    Ok(true)
}

fn apply_insert_item(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    let candidates = collect_candidates(stream)
        .into_iter()
        .filter(|candidate| {
            matches!(
                candidate.kind,
                CandidateKind::Sequence {
                    can_insert: true,
                    ..
                }
            )
        })
        .collect::<Vec<_>>();
    let Some(candidate) = choose(&candidates, input.selector) else {
        return Ok(false);
    };
    let CandidateKind::Sequence { len, .. } = candidate.kind else {
        return Ok(false);
    };
    let path = config_path(&candidate.path, input.header)?;
    editor.insert_item_in_document(
        candidate.document,
        path,
        input.selector % (len + 1),
        edit_value(input.header, input.payload),
    )?;
    Ok(true)
}

fn apply_chain(
    editor: &mut saneyaml::ConfigEditor,
    stream: &LosslessStream,
    input: EditInput<'_>,
) -> saneyaml::Result<bool> {
    if !apply_set(editor, stream, input)? {
        return Ok(false);
    }
    let current = saneyaml::parse_lossless(editor.as_source())?;
    let _ = apply_insert(editor, &current, input);
    let current = saneyaml::parse_lossless(editor.as_source())?;
    let _ = apply_push(editor, &current, input);
    Ok(true)
}

fn collect_candidates(stream: &LosslessStream) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for document in stream.documents() {
        if let Some(root) = document.root() {
            collect_node_candidates(stream, document.index(), root, Vec::new(), &mut candidates);
        }
    }
    candidates
}

fn collect_node_candidates(
    stream: &LosslessStream,
    document: usize,
    node_id: saneyaml::NodeId,
    path: Vec<PathSegment>,
    candidates: &mut Vec<Candidate>,
) {
    let Some(node) = stream.node(node_id) else {
        return;
    };
    candidates.push(Candidate {
        document,
        path: path.clone(),
        kind: CandidateKind::Any,
    });
    match node.kind() {
        LosslessNodeKind::Mapping { style, entries } => {
            candidates.push(Candidate {
                document,
                path: path.clone(),
                kind: CandidateKind::Mapping {
                    can_insert: *style == CollectionStyle::Flow || !entries.is_empty(),
                },
            });
            let unique_keys = unique_scalar_keys(stream, entries);
            for (key_id, value_id) in entries {
                let Some(key) = scalar_key(stream, *key_id) else {
                    continue;
                };
                if !unique_keys.contains_key(key) {
                    continue;
                }
                let mut child_path = path.clone();
                child_path.push(PathSegment::from(key.to_owned()));
                candidates.push(Candidate {
                    document,
                    path: child_path.clone(),
                    kind: CandidateKind::MappingEntry,
                });
                collect_node_candidates(stream, document, *value_id, child_path, candidates);
            }
        }
        LosslessNodeKind::Sequence { style, children } => {
            candidates.push(Candidate {
                document,
                path: path.clone(),
                kind: CandidateKind::Sequence {
                    len: children.len(),
                    can_insert: *style == CollectionStyle::Flow || !children.is_empty(),
                },
            });
            for (index, child) in children.iter().enumerate() {
                let mut child_path = path.clone();
                child_path.push(PathSegment::from(index));
                candidates.push(Candidate {
                    document,
                    path: child_path.clone(),
                    kind: CandidateKind::SequenceItem,
                });
                collect_node_candidates(stream, document, *child, child_path, candidates);
            }
        }
        LosslessNodeKind::Scalar { .. } | LosslessNodeKind::Alias { .. } => {}
    }
}

fn unique_scalar_keys<'a>(
    stream: &'a LosslessStream,
    entries: &'a [(saneyaml::NodeId, saneyaml::NodeId)],
) -> BTreeMap<&'a str, ()> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for (key, _) in entries {
        if let Some(key) = scalar_key(stream, *key) {
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(key, count)| (count == 1).then_some((key, ())))
        .collect()
}

fn scalar_key(stream: &LosslessStream, node: saneyaml::NodeId) -> Option<&str> {
    match stream.node(node)?.kind() {
        LosslessNodeKind::Scalar { value, .. } => Some(value),
        _ => None,
    }
}

fn config_path(path: &[PathSegment], header: &str) -> saneyaml::Result<ConfigPath> {
    if header.contains("path=json") {
        ConfigPath::json_pointer(&json_pointer(path))
    } else {
        Ok(ConfigPath::new(path.iter().cloned()))
    }
}

fn json_pointer(path: &[PathSegment]) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut pointer = String::new();
    for segment in path {
        pointer.push('/');
        match segment {
            PathSegment::Key(key) => pointer.push_str(&key.replace('~', "~0").replace('/', "~1")),
            PathSegment::Index(index) => pointer.push_str(&index.to_string()),
        }
    }
    pointer
}

fn choose<T>(items: &[T], selector: usize) -> Option<&T> {
    (!items.is_empty()).then(|| &items[selector % items.len()])
}

fn split_edit_input(input: &[u8]) -> Option<EditInput<'_>> {
    let line_end = input.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&input[..line_end]).ok()?;
    let body = &input[line_end + 1..];
    let split = find_subslice(body, VALUE_MARKER)?;
    let payload = std::str::from_utf8(&body[split + VALUE_MARKER.len()..]).ok()?;
    Some(EditInput {
        header,
        selector: selector_from_header(header, payload),
        source: &body[..split],
        payload,
    })
}

fn mode_from_header(header: &str, selector: usize) -> EditMode {
    if header.contains("mode=remove") {
        EditMode::Remove
    } else if header.contains("mode=rename") {
        EditMode::Rename
    } else if header.contains("mode=insert-item") {
        EditMode::InsertItem
    } else if header.contains("mode=insert") {
        EditMode::Insert
    } else if header.contains("mode=push") {
        EditMode::Push
    } else if header.contains("mode=chain") {
        EditMode::Chain
    } else if header.contains("mode=set") {
        EditMode::Set
    } else {
        match selector % 7 {
            0 => EditMode::Set,
            1 => EditMode::Remove,
            2 => EditMode::Rename,
            3 => EditMode::Insert,
            4 => EditMode::Push,
            5 => EditMode::InsertItem,
            _ => EditMode::Chain,
        }
    }
}

fn selector_from_header(header: &str, payload: &str) -> usize {
    header
        .bytes()
        .chain(payload.bytes())
        .fold(0usize, |acc, byte| {
            acc.wrapping_mul(33).wrapping_add(byte as usize)
        })
}

fn edit_value(header: &str, payload: &str) -> EditValue {
    let words = payload_words(payload);
    if header.contains("value=map") {
        EditValue::Mapping(BTreeMap::from([
            ("mode".to_owned(), words[0].clone()),
            ("tier".to_owned(), words[1].clone()),
        ]))
    } else if header.contains("value=nested") {
        EditValue::Nested(BTreeMap::from([(
            "items".to_owned(),
            vec![words[0].clone(), words[1].clone()],
        )]))
    } else if header.contains("value=multiline") {
        EditValue::Multiline(format!("{}\n{}", words[0], words[1]))
    } else if header.contains("value=seq") {
        EditValue::Sequence(vec![words[0].clone(), words[1].clone()])
    } else {
        EditValue::Scalar(words[0].clone())
    }
}

fn edit_key(payload: &str) -> String {
    format!("fuzz.{}.key/{}", payload_words(payload)[0], "renamed")
}

fn payload_words(payload: &str) -> Vec<String> {
    let mut words = payload
        .split_whitespace()
        .map(sanitize_word)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    while words.len() < 2 {
        words.push(format!("value{}", words.len() + 1));
    }
    words
}

fn sanitize_word(word: &str) -> String {
    let sanitized = word
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '~'))
        .take(32)
        .collect::<String>();
    if sanitized.is_empty() {
        "value".to_owned()
    } else {
        sanitized
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn assert_error_invariants(input: &[u8], error: &saneyaml::Error) {
    assert!(!error.to_string().is_empty());
    if let Some(location) = error.location() {
        assert!(location.index() <= input.len());
    }
}
