use crate::{
    Error, Node, NodeValue as Value, Number, Result, Span, Tag, TaggedNode,
    key_identity::check_duplicate_at_depth, parse::MAX_DEPTH,
};
use std::{collections::HashMap, fmt::Write as _};

pub(crate) const BYTE_COMPATIBLE_SINGLE_QUOTED_SOURCE: &str =
    "\0yaml-byte-compatible-single-quoted";

/// YAML emission fidelity tier.
///
/// `Structural` is the default deterministic behavior. `ByteCompatible`
/// matches `serde_yaml` bytes for the documented structural writer corpus, and
/// `Preserving` remains a declared target tier for future lossless output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmitOptions {
    /// Deterministic structural YAML output whose parse tree is equivalent to
    /// the input tree.
    #[default]
    Structural,
    /// Opt-in tier for byte-for-byte compatibility with `serde_yaml` output
    /// across the supported structural writer corpus.
    ByteCompatible,
    /// Target tier for source-preserving output over lossless documents.
    Preserving,
}

impl EmitOptions {
    /// Returns the currently implemented structural emission tier.
    pub fn structural() -> Self {
        Self::Structural
    }

    /// Returns the declared byte-compatible target tier.
    pub fn byte_compatible() -> Self {
        Self::ByteCompatible
    }

    /// Returns the declared source-preserving target tier.
    pub fn preserving() -> Self {
        Self::Preserving
    }

    fn ensure_supported(self) -> Result<()> {
        match self {
            Self::Structural | Self::ByteCompatible => Ok(()),
            Self::Preserving => Err(Error::new(
                "`Preserving` emission is declared as a target tier but is not implemented in this preview",
                Span::default(),
            )),
        }
    }
}

pub fn to_string(node: &Node) -> Result<String> {
    to_string_with_options(node, EmitOptions::Structural)
}

pub fn to_string_with_options(node: &Node, options: EmitOptions) -> Result<String> {
    options.ensure_supported()?;
    validate_emittable(node)?;
    let mut out = String::new();
    emit_node(node, 0, Context::Root, options, &mut out);
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
                if is_literal_merge_key(key) {
                    return Err(Error::new(
                        "literal YAML merge keys cannot be emitted without an explicit tag",
                        key.span,
                    ));
                }
                check_duplicate_at_depth(&mut seen, key, depth.saturating_add(1))?;
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

fn is_literal_merge_key(key: &Node) -> bool {
    matches!(&key.value, Value::String(value) if value == "<<")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Root,
    MappingValue,
    SequenceItem,
}

fn emit_node(node: &Node, indent: usize, context: Context, options: EmitOptions, out: &mut String) {
    match &node.value {
        Value::Mapping(entries) => emit_mapping(entries, indent, context, options, out),
        Value::Sequence(items) => emit_sequence(items, indent, context, options, out),
        Value::String(value) if should_emit_block_string(value) => {
            emit_block_string(value, indent, context, out)
        }
        Value::Tagged(tagged)
            if tagged_value_needs_block_indent(&tagged.value)
                && matches!(context, Context::Root | Context::SequenceItem) =>
        {
            emit_tagged_structured(tagged, indent, context, options, out)
        }
        Value::Tagged(tagged) if matches!(options, EmitOptions::ByteCompatible) => {
            emit_tagged_structured(tagged, indent, context, options, out)
        }
        Value::Tagged(_) => out.push_str(&format_inline(node, options)),
        _ => out.push_str(&format_inline(node, options)),
    }
}

fn emit_mapping(
    entries: &[(Node, Node)],
    indent: usize,
    context: Context,
    options: EmitOptions,
    out: &mut String,
) {
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
            out.push_str(&format_inline(key, options));
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push(':');
        } else {
            out.push_str(&format_key(key, options));
            out.push(':');
        }
        emit_mapping_value(value, indent + 2, options, out);
    }
}

fn emit_sequence(
    items: &[Node],
    indent: usize,
    context: Context,
    options: EmitOptions,
    out: &mut String,
) {
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
                emit_node(item, indent + 2, Context::SequenceItem, options, out)
            }
            Value::Mapping(entries) if !entries.is_empty() => {
                let mut first = true;
                for (key, value) in entries {
                    if first {
                        out.push(' ');
                        out.push_str(&format_key(key, options));
                        out.push(':');
                        let value_indent = if mapping_value_needs_block_indent(value, options) {
                            indent + 4
                        } else {
                            indent + 2
                        };
                        emit_mapping_value(value, value_indent, options, out);
                        first = false;
                    } else {
                        out.push('\n');
                        out.push_str(&" ".repeat(indent + 2));
                        out.push_str(&format_key(key, options));
                        out.push(':');
                        emit_mapping_value(value, indent + 4, options, out);
                    }
                }
            }
            Value::Sequence(items) if !items.is_empty() => {
                emit_node(item, indent + 2, Context::SequenceItem, options, out)
            }
            Value::String(text) if should_emit_block_string(text) => {
                emit_node(item, indent + 2, Context::SequenceItem, options, out)
            }
            Value::Tagged(tagged) if tagged_value_needs_block_indent(&tagged.value) => {
                out.push(' ');
                emit_tagged_structured(tagged, indent + 2, Context::SequenceItem, options, out);
            }
            _ => {
                out.push(' ');
                emit_node(item, indent + 2, Context::SequenceItem, options, out);
            }
        }
    }
}

fn emit_mapping_value(value: &Node, indent: usize, options: EmitOptions, out: &mut String) {
    match &value.value {
        Value::Mapping(entries) if !entries.is_empty() => {
            emit_node(value, indent, Context::MappingValue, options, out)
        }
        Value::Sequence(items) if !items.is_empty() => {
            let sequence_indent = if matches!(options, EmitOptions::ByteCompatible) {
                indent.saturating_sub(2)
            } else {
                indent
            };
            emit_node(value, sequence_indent, Context::MappingValue, options, out)
        }
        Value::String(text) if should_emit_block_string(text) => {
            emit_node(value, indent, Context::MappingValue, options, out)
        }
        Value::Tagged(tagged)
            if matches!(options, EmitOptions::ByteCompatible)
                && tagged_value_needs_block_indent(&tagged.value) =>
        {
            out.push(' ');
            emit_node(value, indent, Context::MappingValue, options, out);
        }
        _ => {
            out.push(' ');
            emit_node(value, indent, Context::MappingValue, options, out);
        }
    }
}

fn mapping_value_needs_block_indent(value: &Node, options: EmitOptions) -> bool {
    matches!(&value.value, Value::Mapping(entries) if !entries.is_empty())
        || matches!(&value.value, Value::Sequence(items) if !items.is_empty())
        || matches!(&value.value, Value::String(text) if should_emit_block_string(text))
        || (matches!(options, EmitOptions::ByteCompatible)
            && matches!(&value.value, Value::Tagged(tagged) if tagged_value_needs_block_indent(&tagged.value)))
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

fn emit_tagged_structured(
    tagged: &TaggedNode,
    indent: usize,
    context: Context,
    options: EmitOptions,
    out: &mut String,
) {
    out.push_str(&format_tag(&tagged.tag));
    if let Value::String(text) = &tagged.value.value
        && should_emit_block_string(text)
    {
        out.push(' ');
        let content_indent = if matches!(context, Context::Root) {
            indent + 2
        } else {
            indent
        };
        emit_block_string_indicator_and_content(text, content_indent, out);
    } else if tagged_value_needs_block_indent(&tagged.value) {
        out.push('\n');
        let child_indent = match &tagged.value.value {
            Value::Sequence(items)
                if !items.is_empty() && matches!(context, Context::MappingValue) =>
            {
                indent.saturating_sub(2)
            }
            _ => indent,
        };
        emit_node(&tagged.value, child_indent, Context::Root, options, out);
    } else {
        out.push(' ');
        out.push_str(&format_inline(&tagged.value, options));
    }
}

fn tagged_value_needs_block_indent(value: &Node) -> bool {
    matches!(&value.value, Value::Mapping(entries) if !entries.is_empty())
        || matches!(&value.value, Value::Sequence(items) if !items.is_empty())
        || matches!(&value.value, Value::String(text) if should_emit_block_string(text))
}

fn emit_block_string(value: &str, indent: usize, context: Context, out: &mut String) {
    if !matches!(context, Context::Root) {
        out.push(' ');
    }
    let content_indent = if matches!(context, Context::Root) {
        indent + 2
    } else {
        indent
    };
    emit_block_string_indicator_and_content(value, content_indent, out);
}

fn emit_block_string_indicator_and_content(value: &str, content_indent: usize, out: &mut String) {
    out.push('|');
    if needs_explicit_block_indent(value) {
        out.push('2');
    }
    let trailing_newlines = value.chars().rev().take_while(|ch| *ch == '\n').count();
    let content = value.strip_suffix('\n').unwrap_or(value);
    if trailing_newlines == 0 {
        out.push('-');
    } else if trailing_newlines > 1 || content.is_empty() {
        out.push('+');
    }
    for line in content.split('\n') {
        out.push('\n');
        out.push_str(&" ".repeat(content_indent));
        out.push_str(line);
    }
}

fn should_emit_block_string(value: &str) -> bool {
    value.contains('\n') && block_string_can_represent_literal_content(value)
}

fn block_string_can_represent_literal_content(value: &str) -> bool {
    value
        .chars()
        .all(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
}

fn needs_explicit_block_indent(value: &str) -> bool {
    value
        .split('\n')
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.starts_with(' '))
}

fn format_key(node: &Node, options: EmitOptions) -> String {
    match &node.value {
        Value::String(value) if force_byte_compatible_single_quote(node, options) => {
            single_quote(value)
        }
        Value::String(value) => quote_if_needed(value, options),
        _ => format_inline(node, options),
    }
}

fn format_inline(node: &Node, options: EmitOptions) -> String {
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
        Value::String(value) if force_byte_compatible_single_quote(node, options) => {
            single_quote(value)
        }
        Value::String(value) => quote_if_needed(value, options),
        Value::Sequence(items) => {
            let mut out = String::from("[");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_inline(item, options));
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
                out.push_str(&format_key(key, options));
                out.push_str(": ");
                out.push_str(&format_inline(value, options));
            }
            out.push('}');
            out
        }
        Value::Tagged(tagged) => {
            let mut out = format_tag(&tagged.tag);
            out.push(' ');
            out.push_str(&format_inline(&tagged.value, options));
            out
        }
    }
}

fn format_tag(tag: &Tag) -> String {
    let mut out = String::new();
    if tag.handle == "!" && emitted_tag_suffix_needs_verbatim(&tag.suffix) {
        out.push_str("!<");
        push_uri_escaped_tag_suffix(&mut out, &tag.suffix);
        out.push('>');
    } else {
        out.push_str(&tag.handle);
        push_uri_escaped_tag_suffix(&mut out, &tag.suffix);
    }
    out
}

fn emitted_tag_suffix_needs_verbatim(suffix: &str) -> bool {
    !suffix.is_empty()
        && (suffix.starts_with("tag:")
            || suffix.starts_with(':')
            || suffix.starts_with('<')
            || suffix.ends_with(':')
            || suffix.contains('!')
            || suffix
                .chars()
                .any(|ch| ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}')))
}

fn push_uri_escaped_tag_suffix(out: &mut String, suffix: &str) {
    for ch in suffix.chars() {
        if ch == '%' || ch.is_control() {
            let mut bytes = [0; 4];
            for byte in ch.encode_utf8(&mut bytes).as_bytes() {
                write!(out, "%{byte:02X}").expect("writing to String cannot fail");
            }
        } else {
            out.push(ch);
        }
    }
}

fn force_byte_compatible_single_quote(node: &Node, options: EmitOptions) -> bool {
    matches!(options, EmitOptions::ByteCompatible)
        && node
            .scalar_source()
            .is_some_and(|source| source.raw() == BYTE_COMPATIBLE_SINGLE_QUOTED_SOURCE)
}

fn quote_if_needed(value: &str, options: EmitOptions) -> String {
    if matches!(options, EmitOptions::ByteCompatible) {
        return quote_byte_compatible_if_needed(value);
    }
    quote_structural_if_needed(value)
}

fn quote_structural_if_needed(value: &str) -> String {
    if is_structural_plain_safe(value) {
        return value.to_string();
    }
    double_quote(value)
}

fn quote_byte_compatible_if_needed(value: &str) -> String {
    if is_byte_compatible_plain_safe(value) {
        return value.to_string();
    }
    if value.chars().any(|ch| ch.is_control() && ch != '\n') {
        return double_quote(value);
    }
    single_quote(value)
}

fn double_quote(value: &str) -> String {
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

fn single_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn is_structural_plain_safe(value: &str) -> bool {
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

fn is_byte_compatible_plain_safe(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "null" | "~" | "true" | "false" | ".nan" | ".inf" | "+.inf"
    ) || looks_like_byte_compatible_number(value)
    {
        return false;
    }
    if value == "..." || value.starts_with("... ") || value.starts_with('%') {
        return false;
    }
    if value.chars().any(char::is_control) {
        return false;
    }
    if has_colon_followed_by_whitespace(value) || has_hash_preceded_by_whitespace(value) {
        return false;
    }
    if value.contains(['[', ']', '{', '}']) {
        return false;
    }
    if value.starts_with([
        '[', ']', '{', '}', ',', '#', '&', '*', '!', '|', '>', '\'', '"', '@', '`',
    ]) {
        return false;
    }
    if starts_with_indicator_and_whitespace(value, '-') {
        return false;
    }
    if starts_with_indicator_and_whitespace(value, '?') {
        return false;
    }
    if starts_with_indicator_and_whitespace(value, ':') {
        return false;
    }
    true
}

fn starts_with_indicator_and_whitespace(value: &str, indicator: char) -> bool {
    let mut chars = value.chars();
    if !matches!(chars.next(), Some(ch) if ch == indicator) {
        return false;
    }
    match chars.next() {
        Some(ch) => ch.is_whitespace(),
        None => true,
    }
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

fn looks_like_byte_compatible_number(value: &str) -> bool {
    if value.contains('_') {
        return false;
    }
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut idx = usize::from(matches!(bytes[0], b'+' | b'-'));
    if idx >= bytes.len() {
        return false;
    }
    let mut digits = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        digits += 1;
        idx += 1;
    }
    if idx == bytes.len() {
        return digits > 0;
    }
    if value.contains(['.', 'e', 'E']) {
        return match value.parse::<f64>() {
            Ok(value) => value.is_finite(),
            Err(_) => false,
        };
    }
    false
}
