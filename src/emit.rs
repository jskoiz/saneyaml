use crate::{
    Error, Node, NodeValue as Value, Number, Result, Tag, TaggedNode,
    key_identity::check_duplicate_at_depth, schema::DEFAULT_MAX_NESTING_DEPTH,
};
use std::{collections::HashMap, fmt::Write as _};

pub(crate) const BYTE_COMPATIBLE_SINGLE_QUOTED_SOURCE: &str =
    "\0yaml-byte-compatible-single-quoted";

/// YAML emission options.
///
/// The default is deterministic structural output: insertion-order mappings,
/// plain-where-safe scalars, literal block scalars, and block collections.
/// `ByteCompatible` remains an opt-in fidelity mode for the documented
/// `serde_yaml` writer corpus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmitOptions {
    fidelity: EmitFidelity,
    key_order: KeyOrder,
    scalar_quote_style: ScalarQuoteStyle,
    block_scalar_style: BlockScalarStyle,
    collection_style: EmitCollectionStyle,
}

/// Emission fidelity mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmitFidelity {
    /// Deterministic structural YAML whose parse tree is equivalent to the
    /// input tree.
    #[default]
    Structural,
    /// Opt-in byte compatibility with `serde_yaml` for the supported
    /// structural writer corpus.
    ByteCompatible,
}

/// Mapping key ordering policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyOrder {
    /// Keep mapping entries in their source or serialization order.
    #[default]
    Preserve,
    /// Sort mapping entries by their emitted key text.
    Sort,
}

/// Scalar quote policy for inline string scalars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScalarQuoteStyle {
    /// Emit plain scalars where YAML 1.2 can round-trip them safely, otherwise
    /// use a quoted style.
    #[default]
    PlainWhereSafe,
    /// Prefer single quotes where this crate can round-trip them safely,
    /// falling back to double quotes for controls, apostrophes, and multiline
    /// values.
    SingleQuoted,
    /// Emit inline string scalars with double quotes.
    DoubleQuoted,
}

/// Block scalar style policy for multiline string scalars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockScalarStyle {
    /// Emit multiline strings as literal block scalars where representable.
    #[default]
    Literal,
    /// Prefer folded block scalars where folding can preserve the value,
    /// otherwise fall back to literal block scalars.
    Folded,
}

/// Collection layout policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmitCollectionStyle {
    /// Emit non-empty collections in block layout.
    #[default]
    Block,
    /// Emit collections in flow layout.
    Flow,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self::structural()
    }
}

impl EmitOptions {
    /// Returns the currently implemented structural emission tier.
    pub fn structural() -> Self {
        Self {
            fidelity: EmitFidelity::Structural,
            key_order: KeyOrder::Preserve,
            scalar_quote_style: ScalarQuoteStyle::PlainWhereSafe,
            block_scalar_style: BlockScalarStyle::Literal,
            collection_style: EmitCollectionStyle::Block,
        }
    }

    /// Returns the byte-compatible target tier.
    pub fn byte_compatible() -> Self {
        Self {
            fidelity: EmitFidelity::ByteCompatible,
            ..Self::structural()
        }
    }

    /// Returns these options with an updated mapping key ordering policy.
    pub fn with_key_order(mut self, key_order: KeyOrder) -> Self {
        self.key_order = key_order;
        self
    }

    /// Returns these options with an updated inline scalar quote policy.
    pub fn with_scalar_quote_style(mut self, scalar_quote_style: ScalarQuoteStyle) -> Self {
        self.scalar_quote_style = scalar_quote_style;
        self
    }

    /// Returns these options with an updated block scalar style policy.
    pub fn with_block_scalar_style(mut self, block_scalar_style: BlockScalarStyle) -> Self {
        self.block_scalar_style = block_scalar_style;
        self
    }

    /// Returns these options with an updated collection layout policy.
    pub fn with_collection_style(mut self, collection_style: EmitCollectionStyle) -> Self {
        self.collection_style = collection_style;
        self
    }

    pub(crate) fn is_byte_compatible(self) -> bool {
        matches!(self.fidelity, EmitFidelity::ByteCompatible)
    }

    fn uses_flow_collections(self) -> bool {
        matches!(self.collection_style, EmitCollectionStyle::Flow)
    }

    fn sorts_keys(self) -> bool {
        matches!(self.key_order, KeyOrder::Sort)
    }
}

pub fn to_string(node: &Node) -> Result<String> {
    to_string_with_options(node, EmitOptions::structural())
}

pub fn to_string_with_options(node: &Node, options: EmitOptions) -> Result<String> {
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
    if depth > DEFAULT_MAX_NESTING_DEPTH {
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
        Value::Mapping(_) | Value::Sequence(_) if options.uses_flow_collections() => {
            out.push_str(&format_inline(node, options))
        }
        Value::Mapping(entries) => emit_mapping(entries, indent, context, options, out),
        Value::Sequence(items) => emit_sequence(items, indent, context, options, out),
        Value::String(value) if should_emit_block_string(value, options) => {
            emit_block_string(value, indent, context, options, out)
        }
        Value::Tagged(tagged)
            if tagged_value_needs_block_indent(&tagged.value, options)
                && matches!(context, Context::Root | Context::SequenceItem) =>
        {
            emit_tagged_structured(tagged, indent, context, options, out)
        }
        Value::Tagged(tagged) if options.is_byte_compatible() => {
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
    let ordered = ordered_entries(entries, options);
    for (idx, (key, value)) in ordered.into_iter().enumerate() {
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
            Value::String(text) if should_emit_block_string(text, options) => {
                emit_node(item, indent + 2, Context::SequenceItem, options, out)
            }
            Value::Tagged(tagged) if tagged_value_needs_block_indent(&tagged.value, options) => {
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
            let sequence_indent = if options.is_byte_compatible() {
                indent.saturating_sub(2)
            } else {
                indent
            };
            emit_node(value, sequence_indent, Context::MappingValue, options, out)
        }
        Value::String(text) if should_emit_block_string(text, options) => {
            emit_node(value, indent, Context::MappingValue, options, out)
        }
        Value::Tagged(tagged)
            if options.is_byte_compatible()
                && tagged_value_needs_block_indent(&tagged.value, options) =>
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
        || matches!(&value.value, Value::String(text) if should_emit_block_string(text, options))
        || (options.is_byte_compatible()
            && matches!(&value.value, Value::Tagged(tagged) if tagged_value_needs_block_indent(&tagged.value, options)))
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
        && should_emit_block_string(text, options)
    {
        out.push(' ');
        let content_indent = if matches!(context, Context::Root) {
            indent + 2
        } else {
            indent
        };
        emit_block_string_indicator_and_content(text, content_indent, options, out);
    } else if tagged_value_needs_block_indent(&tagged.value, options) {
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

fn tagged_value_needs_block_indent(value: &Node, options: EmitOptions) -> bool {
    matches!(&value.value, Value::Mapping(entries) if !entries.is_empty())
        || matches!(&value.value, Value::Sequence(items) if !items.is_empty())
        || matches!(&value.value, Value::String(text) if should_emit_block_string(text, options))
}

fn emit_block_string(
    value: &str,
    indent: usize,
    context: Context,
    options: EmitOptions,
    out: &mut String,
) {
    if !matches!(context, Context::Root) {
        out.push(' ');
    }
    let content_indent = if matches!(context, Context::Root) {
        indent + 2
    } else {
        indent
    };
    emit_block_string_indicator_and_content(value, content_indent, options, out);
}

fn emit_block_string_indicator_and_content(
    value: &str,
    content_indent: usize,
    options: EmitOptions,
    out: &mut String,
) {
    let folded = matches!(options.block_scalar_style, BlockScalarStyle::Folded)
        && folded_block_can_represent_literal_content(value);
    out.push(if folded { '>' } else { '|' });
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

fn should_emit_block_string(value: &str, options: EmitOptions) -> bool {
    !options.uses_flow_collections()
        && value.contains('\n')
        && block_string_can_represent_literal_content(value)
}

fn block_string_can_represent_literal_content(value: &str) -> bool {
    value
        .chars()
        .all(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        && !value.chars().any(|ch| is_yaml_line_break(ch) && ch != '\n')
}

fn needs_explicit_block_indent(value: &str) -> bool {
    value
        .split('\n')
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.starts_with(' '))
}

fn folded_block_can_represent_literal_content(value: &str) -> bool {
    if !block_string_can_represent_literal_content(value) {
        return false;
    }
    if value.ends_with("\n\n") || value.contains("\n\n") {
        return false;
    }
    let mut lines = value.strip_suffix('\n').unwrap_or(value).split('\n');
    let Some(_) = lines.next() else {
        return false;
    };
    lines.all(|line| line.is_empty() || line.starts_with([' ', '\t']))
}

fn ordered_entries(entries: &[(Node, Node)], options: EmitOptions) -> Vec<&(Node, Node)> {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    if options.sorts_keys() {
        ordered.sort_by(|(left, _), (right, _)| {
            format_inline(left, options).cmp(&format_inline(right, options))
        });
    }
    ordered
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
            let ordered = ordered_entries(entries, options);
            for (idx, (key, value)) in ordered.into_iter().enumerate() {
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
    options.is_byte_compatible()
        && node
            .scalar_source()
            .is_some_and(|source| source.raw() == BYTE_COMPATIBLE_SINGLE_QUOTED_SOURCE)
}

fn quote_if_needed(value: &str, options: EmitOptions) -> String {
    if options.is_byte_compatible() {
        return quote_byte_compatible_if_needed(value);
    }
    match options.scalar_quote_style {
        ScalarQuoteStyle::PlainWhereSafe => quote_structural_if_needed(value),
        ScalarQuoteStyle::SingleQuoted if single_quote_can_represent(value) => single_quote(value),
        ScalarQuoteStyle::SingleQuoted => double_quote(value),
        ScalarQuoteStyle::DoubleQuoted => double_quote(value),
    }
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
            ch if is_yaml_line_break(ch) => out.push_str(&format!("\\u{:04X}", ch as u32)),
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

fn single_quote_can_represent(value: &str) -> bool {
    !value.contains('\'')
        && !value.chars().any(is_yaml_line_break)
        && value.chars().all(|ch| !ch.is_control() || ch == '\t')
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
    if value
        .chars()
        .any(|ch| ch.is_control() || is_yaml_line_break(ch))
    {
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
    if value
        .chars()
        .any(|ch| ch.is_control() || is_yaml_line_break(ch))
    {
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

fn is_yaml_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}')
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
