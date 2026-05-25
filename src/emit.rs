use crate::{
    Error, Node, NodeValue as Value, Number, Result, key_identity::check_duplicate,
    parse::MAX_DEPTH,
};
use std::collections::HashMap;

pub fn to_string(node: &Node) -> Result<String> {
    validate_emittable(node)?;
    let mut out = String::new();
    emit_node(node, 0, Context::Root, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn validate_emittable(node: &Node) -> Result<()> {
    validate_emittable_at(node, 1)
}

fn validate_emittable_at(node: &Node, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::new("maximum YAML nesting depth exceeded", node.span));
    }
    match &node.value {
        Value::Mapping(entries) => {
            let mut seen = HashMap::new();
            for (key, value) in entries {
                check_duplicate(&mut seen, key)?;
                validate_emittable_at(key, depth + 1)?;
                validate_emittable_at(value, depth + 1)?;
            }
        }
        Value::Sequence(items) => {
            for item in items {
                validate_emittable_at(item, depth + 1)?;
            }
        }
        Value::Tagged(tagged) => {
            if let Value::Tagged(inner) = &tagged.value.value {
                return Err(Error::new(
                    "nested YAML tags cannot be emitted directly",
                    tag_span_or_node_span(&tagged.value, inner.tag_span),
                ));
            }
            validate_emittable_at(&tagged.value, depth + 1)?;
        }
        _ => {}
    }
    Ok(())
}

fn tag_span_or_node_span(node: &Node, tag_span: crate::Span) -> crate::Span {
    if tag_span.line > 0 && tag_span.column > 0 {
        tag_span
    } else {
        node.span
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Root,
    MappingValue,
    SequenceItem,
}

fn emit_node(node: &Node, indent: usize, context: Context, out: &mut String) {
    match &node.value {
        Value::Mapping(entries) => emit_mapping(entries, indent, context, out),
        Value::Sequence(items) => emit_sequence(items, indent, context, out),
        Value::String(value) if value.contains('\n') => {
            emit_block_string(value, indent, context, out)
        }
        Value::Tagged(_) => out.push_str(&format_inline(node)),
        _ => out.push_str(&format_inline(node)),
    }
}

fn emit_mapping(entries: &[(Node, Node)], indent: usize, context: Context, out: &mut String) {
    if entries.is_empty() {
        out.push_str("{}");
        return;
    }
    if matches!(context, Context::MappingValue | Context::SequenceItem) {
        out.push('\n');
    }
    for (idx, (key, value)) in entries.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&" ".repeat(indent));
        if needs_explicit_key(key) {
            out.push_str("? ");
            out.push_str(&format_inline(key));
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push(':');
        } else {
            out.push_str(&format_key(key));
            out.push(':');
        }
        emit_mapping_value(value, indent + 2, out);
    }
}

fn emit_sequence(items: &[Node], indent: usize, context: Context, out: &mut String) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    if matches!(context, Context::MappingValue | Context::SequenceItem) {
        out.push('\n');
    }
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&" ".repeat(indent));
        out.push('-');
        match &item.value {
            Value::Mapping(entries)
                if !entries.is_empty() && mapping_needs_explicit_keys(entries) =>
            {
                emit_node(item, indent + 2, Context::SequenceItem, out)
            }
            Value::Mapping(entries) if !entries.is_empty() => {
                let mut first = true;
                for (key, value) in entries {
                    if first {
                        out.push(' ');
                        out.push_str(&format_key(key));
                        out.push(':');
                        let value_indent = if mapping_value_needs_block_indent(value) {
                            indent + 4
                        } else {
                            indent + 2
                        };
                        emit_mapping_value(value, value_indent, out);
                        first = false;
                    } else {
                        out.push('\n');
                        out.push_str(&" ".repeat(indent + 2));
                        out.push_str(&format_key(key));
                        out.push(':');
                        emit_mapping_value(value, indent + 4, out);
                    }
                }
            }
            Value::Sequence(items) if !items.is_empty() => {
                emit_node(item, indent + 2, Context::SequenceItem, out)
            }
            Value::String(text) if text.contains('\n') => {
                emit_node(item, indent + 2, Context::SequenceItem, out)
            }
            _ => {
                out.push(' ');
                emit_node(item, indent + 2, Context::SequenceItem, out);
            }
        }
    }
}

fn emit_mapping_value(value: &Node, indent: usize, out: &mut String) {
    match &value.value {
        Value::Mapping(entries) if !entries.is_empty() => {
            emit_node(value, indent, Context::MappingValue, out)
        }
        Value::Sequence(items) if !items.is_empty() => {
            emit_node(value, indent, Context::MappingValue, out)
        }
        Value::String(text) if text.contains('\n') => {
            emit_node(value, indent, Context::MappingValue, out)
        }
        _ => {
            out.push(' ');
            emit_node(value, indent, Context::MappingValue, out);
        }
    }
}

fn mapping_value_needs_block_indent(value: &Node) -> bool {
    matches!(&value.value, Value::Mapping(entries) if !entries.is_empty())
        || matches!(&value.value, Value::Sequence(items) if !items.is_empty())
        || matches!(&value.value, Value::String(text) if text.contains('\n'))
}

fn mapping_needs_explicit_keys(entries: &[(Node, Node)]) -> bool {
    entries.iter().any(|(key, _)| needs_explicit_key(key))
}

fn needs_explicit_key(key: &Node) -> bool {
    matches!(
        key.value,
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)
    )
}

fn emit_block_string(value: &str, indent: usize, context: Context, out: &mut String) {
    if !matches!(context, Context::Root) {
        out.push(' ');
    }
    out.push('|');
    if needs_explicit_block_indent(value) {
        out.push('2');
    }
    let trailing_newlines = value.chars().rev().take_while(|ch| *ch == '\n').count();
    if trailing_newlines == 0 {
        out.push('-');
    } else if trailing_newlines > 1 {
        out.push('+');
    }
    let content = value.strip_suffix('\n').unwrap_or(value);
    let content_indent = if matches!(context, Context::Root) {
        indent + 2
    } else {
        indent
    };
    for line in content.split('\n') {
        out.push('\n');
        out.push_str(&" ".repeat(content_indent));
        out.push_str(line);
    }
}

fn needs_explicit_block_indent(value: &str) -> bool {
    value
        .split('\n')
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.starts_with(' '))
}

fn format_key(node: &Node) -> String {
    match &node.value {
        Value::String(value) => quote_if_needed(value),
        _ => format_inline(node),
    }
}

fn format_inline(node: &Node) -> String {
    match &node.value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(Number::Integer(value)) => value.to_string(),
        Value::Number(Number::Unsigned(value)) => value.to_string(),
        Value::Number(Number::Float(value)) if value.is_finite() => {
            let mut text = value.to_string();
            if !text.contains('.') && !text.contains('e') && !text.contains('E') {
                text.push_str(".0");
            }
            text
        }
        Value::Number(Number::Float(value)) if value.is_nan() => ".nan".to_string(),
        Value::Number(Number::Float(value)) if value.is_sign_negative() => "-.inf".to_string(),
        Value::Number(Number::Float(_)) => ".inf".to_string(),
        Value::String(value) => quote_if_needed(value),
        Value::Sequence(items) => {
            let mut out = String::from("[");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_inline(item));
            }
            out.push(']');
            out
        }
        Value::Mapping(entries) => {
            let mut out = String::from("{");
            for (idx, (key, value)) in entries.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_key(key));
                out.push_str(": ");
                out.push_str(&format_inline(value));
            }
            out.push('}');
            out
        }
        Value::Tagged(tagged) => {
            let mut out = tagged.tag.to_string();
            out.push(' ');
            out.push_str(&format_inline(&tagged.value));
            out
        }
    }
}

fn quote_if_needed(value: &str) -> String {
    if is_plain_safe(value) {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn is_plain_safe(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "null" | "~" | "true" | "false" | ".nan" | ".inf" | "+.inf"
    ) || looks_like_number(value)
    {
        return false;
    }
    if value == "..." || value.starts_with("... ") || value.starts_with('%') {
        return false;
    }
    if value.chars().any(char::is_control) {
        return false;
    }
    if value.ends_with(':')
        || has_colon_followed_by_whitespace(value)
        || value.contains(',')
        || value.contains(['"', '\'', '\\'])
        || value.contains(['[', ']', '{', '}'])
        || has_hash_preceded_by_whitespace(value)
        || value.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '@',
            '`',
        ])
    {
        return false;
    }
    true
}

fn has_colon_followed_by_whitespace(value: &str) -> bool {
    value.char_indices().any(|(idx, ch)| {
        ch == ':'
            && value[idx + ch.len_utf8()..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
    })
}

fn has_hash_preceded_by_whitespace(value: &str) -> bool {
    let mut previous = None;
    for ch in value.chars() {
        if ch == '#' && previous.is_some_and(char::is_whitespace) {
            return true;
        }
        previous = Some(ch);
    }
    false
}

fn looks_like_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut idx = usize::from(matches!(bytes[0], b'+' | b'-'));
    if idx >= bytes.len() {
        return false;
    }
    let mut digits = 0;
    while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'_') {
        if bytes[idx].is_ascii_digit() {
            digits += 1;
        }
        idx += 1;
    }
    if idx == bytes.len() && digits > 0 {
        return true;
    }
    if value.contains(['.', 'e', 'E']) {
        return value.replace('_', "").parse::<f64>().is_ok();
    }
    false
}
