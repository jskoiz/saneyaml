use crate::{Error, Node, NodeValue as Value, Number, Result, Span, parse::MAX_DEPTH};
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DuplicateKey {
    Null,
    Bool(bool),
    Integer(i128),
    Unsigned(u128),
    Float(u64),
    String(String),
    Sequence(Vec<DuplicateKey>),
    Mapping(Vec<(DuplicateKey, DuplicateKey)>),
}

impl DuplicateKey {
    fn label(&self) -> String {
        match self {
            DuplicateKey::Null => "null".to_string(),
            DuplicateKey::Bool(value) => value.to_string(),
            DuplicateKey::Integer(value) => value.to_string(),
            DuplicateKey::Unsigned(value) => value.to_string(),
            DuplicateKey::Float(bits) => f64::from_bits(*bits).to_string(),
            DuplicateKey::String(value) => value.clone(),
            DuplicateKey::Sequence(items) => {
                let items = items
                    .iter()
                    .map(DuplicateKey::label)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{items}]")
            }
            DuplicateKey::Mapping(entries) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key.label(), value.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{entries}}}")
            }
        }
    }
}

pub(crate) fn check_duplicate_for_mode(
    recording_events: bool,
    seen: &mut HashMap<DuplicateKey, Span>,
    key: &Node,
) -> Result<()> {
    if recording_events {
        return Ok(());
    }
    check_duplicate(seen, key)
}

pub(crate) fn check_duplicate(seen: &mut HashMap<DuplicateKey, Span>, key: &Node) -> Result<()> {
    check_duplicate_at_depth(seen, key, 1)
}

pub(crate) fn check_duplicate_at_depth(
    seen: &mut HashMap<DuplicateKey, Span>,
    key: &Node,
    depth: usize,
) -> Result<()> {
    let Some(key_identity) = duplicate_key_identity_at(key, depth)? else {
        return Ok(());
    };
    if let Some(previous) = seen.insert(key_identity.clone(), key.span) {
        let key_text = key_identity.label();
        return Err(Error::with_related(
            format!("duplicate mapping key `{key_text}`"),
            key.span,
            "previous key is here",
            previous,
        ));
    }
    Ok(())
}

fn duplicate_key_identity_at(key: &Node, depth: usize) -> Result<Option<DuplicateKey>> {
    if depth > MAX_DEPTH {
        return Err(Error::new("maximum YAML nesting depth exceeded", key.span));
    }

    Ok(match &key.value {
        Value::Null => Some(DuplicateKey::Null),
        Value::Bool(value) => Some(DuplicateKey::Bool(*value)),
        Value::Number(Number::Integer(value)) if *value < 0 => Some(DuplicateKey::Integer(*value)),
        Value::Number(Number::Integer(value)) => Some(DuplicateKey::Unsigned(*value as u128)),
        Value::Number(Number::Unsigned(value)) => Some(DuplicateKey::Unsigned(*value)),
        Value::Number(Number::Float(value)) => Some(DuplicateKey::Float(value.to_bits())),
        Value::String(value) => Some(DuplicateKey::String(value.clone())),
        Value::Sequence(items) => duplicate_sequence_identity(items, next_depth(depth))?,
        Value::Mapping(entries) => duplicate_mapping_identity(entries, next_depth(depth))?,
        Value::Tagged(tagged) => duplicate_key_identity_at(&tagged.value, next_depth(depth))?,
    })
}

fn duplicate_sequence_identity(items: &[Node], depth: usize) -> Result<Option<DuplicateKey>> {
    let mut identities = Vec::with_capacity(items.len());
    for item in items {
        let Some(identity) = duplicate_key_identity_at(item, depth)? else {
            return Ok(None);
        };
        identities.push(identity);
    }
    Ok(Some(DuplicateKey::Sequence(identities)))
}

fn duplicate_mapping_identity(
    entries: &[(Node, Node)],
    depth: usize,
) -> Result<Option<DuplicateKey>> {
    let mut identities = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        let Some(key_identity) = duplicate_key_identity_at(key, depth)? else {
            return Ok(None);
        };
        let Some(value_identity) = duplicate_key_identity_at(value, depth)? else {
            return Ok(None);
        };
        identities.push((key_identity, value_identity));
    }
    identities.sort();
    Ok(Some(DuplicateKey::Mapping(identities)))
}

fn next_depth(depth: usize) -> usize {
    depth.saturating_add(1)
}
