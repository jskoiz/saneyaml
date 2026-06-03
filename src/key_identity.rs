use crate::{
    Error, ErrorCategory, Node, NodeValue as Value, Number, Result, Span, ast::ScalarText,
    schema::DEFAULT_MAX_NESTING_DEPTH,
};
use std::collections::{HashMap, hash_map::Entry};
use std::fmt::{self, Write as _};

const INLINE_LIMIT: usize = 4;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DuplicateKey {
    Null,
    Bool(bool),
    Integer(i128),
    Unsigned(u128),
    Float(u64),
    String(ScalarText),
    Sequence(Vec<DuplicateKey>),
    Mapping(Vec<(DuplicateKey, DuplicateKey)>),
}

#[derive(Debug, Default)]
pub(crate) struct DuplicateKeyTracker {
    inline: [Option<(DuplicateKey, Span)>; INLINE_LIMIT],
    overflow: Option<HashMap<DuplicateKey, Span>>,
}

impl DuplicateKeyTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, key_identity: DuplicateKey, span: Span) -> Option<(DuplicateKey, Span)> {
        if let Some(seen) = &mut self.overflow {
            return insert_into_hashmap(seen, key_identity, span);
        }

        for entry in self.inline.iter_mut().flatten() {
            if entry.0 == key_identity {
                return Some((key_identity, entry.1));
            }
        }

        if let Some(slot) = self.inline.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some((key_identity, span));
            return None;
        }

        let mut seen = HashMap::with_capacity(INLINE_LIMIT + 1);
        for slot in &mut self.inline {
            if let Some((key, span)) = slot.take() {
                seen.insert(key, span);
            }
        }
        let duplicate = insert_into_hashmap(&mut seen, key_identity, span);
        self.overflow = Some(seen);
        duplicate
    }
}

impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DuplicateKey::Null => f.write_str("null"),
            DuplicateKey::Bool(value) => write!(f, "{value}"),
            DuplicateKey::Integer(value) => write!(f, "{value}"),
            DuplicateKey::Unsigned(value) => write!(f, "{value}"),
            DuplicateKey::Float(bits) => write!(f, "{}", f64::from_bits(*bits)),
            DuplicateKey::String(value) => f.write_str(value.as_str()),
            DuplicateKey::Sequence(items) => {
                f.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            DuplicateKey::Mapping(entries) => {
                f.write_str("{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                f.write_str("}")
            }
        }
    }
}

pub(crate) fn same_key_identity(left: &Node, right: &Node) -> Result<bool> {
    Ok(duplicate_key_identity_at(left, 1)? == duplicate_key_identity_at(right, 1)?)
}

pub(crate) fn check_duplicate_at_depth(
    seen: &mut HashMap<DuplicateKey, Span>,
    key: &Node,
    depth: usize,
) -> Result<()> {
    check_duplicate_at_depth_limit(seen, key, depth, Some(DEFAULT_MAX_NESTING_DEPTH))
}

pub(crate) fn check_duplicate_at_depth_limit(
    seen: &mut HashMap<DuplicateKey, Span>,
    key: &Node,
    depth: usize,
    max_depth: Option<usize>,
) -> Result<()> {
    let Some(key_identity) = duplicate_key_identity_with_limit(key, depth, max_depth)? else {
        return Ok(());
    };
    if let Some((duplicate, previous)) = insert_into_hashmap(seen, key_identity, key.span) {
        return duplicate_key_error(&duplicate, key.span, previous);
    }
    Ok(())
}

pub(crate) fn check_duplicate_with_tracker_at_depth_limit(
    seen: &mut DuplicateKeyTracker,
    key: &Node,
    depth: usize,
    max_depth: Option<usize>,
) -> Result<()> {
    let Some(key_identity) = duplicate_key_identity_with_limit(key, depth, max_depth)? else {
        return Ok(());
    };
    if let Some((duplicate, previous)) = seen.insert(key_identity, key.span) {
        return duplicate_key_error(&duplicate, key.span, previous);
    }
    Ok(())
}

fn insert_into_hashmap(
    seen: &mut HashMap<DuplicateKey, Span>,
    key_identity: DuplicateKey,
    span: Span,
) -> Option<(DuplicateKey, Span)> {
    match seen.entry(key_identity) {
        Entry::Vacant(entry) => {
            entry.insert(span);
            None
        }
        Entry::Occupied(entry) => Some((entry.key().clone(), *entry.get())),
    }
}

fn duplicate_key_error(duplicate: &DuplicateKey, span: Span, previous: Span) -> Result<()> {
    let mut message = String::from("duplicate mapping key `");
    let _ = write!(message, "{duplicate}");
    message.push('`');
    Err(Error::with_related_category(
        message,
        span,
        "previous key is here",
        previous,
        ErrorCategory::DuplicateKey,
    ))
}

fn duplicate_key_identity_at(key: &Node, depth: usize) -> Result<Option<DuplicateKey>> {
    duplicate_key_identity_with_limit(key, depth, Some(DEFAULT_MAX_NESTING_DEPTH))
}

fn duplicate_key_identity_with_limit(
    key: &Node,
    depth: usize,
    max_depth: Option<usize>,
) -> Result<Option<DuplicateKey>> {
    if max_depth.is_some_and(|max| depth > max) {
        return Err(Error::limit(
            "maximum YAML nesting depth exceeded",
            key.span,
        ));
    }

    Ok(match &key.value {
        Value::Null => Some(DuplicateKey::Null),
        Value::Bool(value) => Some(DuplicateKey::Bool(*value)),
        Value::Number(Number::Integer(value)) if *value < 0 => Some(DuplicateKey::Integer(*value)),
        Value::Number(Number::Integer(value)) => Some(DuplicateKey::Unsigned(*value as u128)),
        Value::Number(Number::Unsigned(value)) => Some(DuplicateKey::Unsigned(*value)),
        Value::Number(Number::Float(value)) => Some(DuplicateKey::Float(float_key_bits(*value))),
        Value::String(_) => key.scalar_string_text().map(DuplicateKey::String),
        Value::Sequence(items) => duplicate_sequence_identity(items, next_depth(depth), max_depth)?,
        Value::Mapping(entries) => {
            duplicate_mapping_identity(entries, next_depth(depth), max_depth)?
        }
        Value::Tagged(tagged) => {
            duplicate_key_identity_with_limit(&tagged.value, next_depth(depth), max_depth)?
        }
    })
}

fn duplicate_sequence_identity(
    items: &[Node],
    depth: usize,
    max_depth: Option<usize>,
) -> Result<Option<DuplicateKey>> {
    let mut identities = Vec::with_capacity(items.len());
    for item in items {
        let Some(identity) = duplicate_key_identity_with_limit(item, depth, max_depth)? else {
            return Ok(None);
        };
        identities.push(identity);
    }
    Ok(Some(DuplicateKey::Sequence(identities)))
}

fn duplicate_mapping_identity(
    entries: &[(Node, Node)],
    depth: usize,
    max_depth: Option<usize>,
) -> Result<Option<DuplicateKey>> {
    let mut identities = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        let Some(key_identity) = duplicate_key_identity_with_limit(key, depth, max_depth)? else {
            return Ok(None);
        };
        let Some(value_identity) = duplicate_key_identity_with_limit(value, depth, max_depth)?
        else {
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

fn float_key_bits(value: f64) -> u64 {
    if value == 0.0 {
        return 0.0f64.to_bits();
    }
    if value.is_nan() {
        return f64::NAN.to_bits();
    }
    value.to_bits()
}
