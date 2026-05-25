use crate::{Error, Node, NodeValue as Value, Number, Result, Span};
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
    let Some(key_identity) = duplicate_key_identity(key) else {
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

fn duplicate_key_identity(key: &Node) -> Option<DuplicateKey> {
    match &key.value {
        Value::Null => Some(DuplicateKey::Null),
        Value::Bool(value) => Some(DuplicateKey::Bool(*value)),
        Value::Number(Number::Integer(value)) => Some(DuplicateKey::Integer(*value)),
        Value::Number(Number::Unsigned(value)) => Some(DuplicateKey::Unsigned(*value)),
        Value::Number(Number::Float(value)) => Some(DuplicateKey::Float(value.to_bits())),
        Value::String(value) => Some(DuplicateKey::String(value.clone())),
        Value::Sequence(items) => items
            .iter()
            .map(duplicate_key_identity)
            .collect::<Option<Vec<_>>>()
            .map(DuplicateKey::Sequence),
        Value::Mapping(entries) => entries
            .iter()
            .map(|(key, value)| {
                Some((duplicate_key_identity(key)?, duplicate_key_identity(value)?))
            })
            .collect::<Option<Vec<_>>>()
            .map(DuplicateKey::Mapping),
        Value::Tagged(tagged) => duplicate_key_identity(&tagged.value),
    }
}
