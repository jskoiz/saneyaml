//! YAML parser entrypoints and raw event types.
//!
//! ```rust
//! let events = saneyaml::parse_events("items:\n  - one\n")?;
//! assert!(matches!(events.first(), Some(saneyaml::Event::StreamStart)));
//! assert!(events.iter().any(|event| {
//!     matches!(event, saneyaml::Event::Scalar { value, .. } if value == "items")
//! }));
//! # Ok::<(), saneyaml::Error>(())
//! ```

use crate::{
    BorrowedNode, Error, Node, NodeValue as Value, Number, Result, Span, Tag, TaggedNode,
    Timestamp,
    ast::MergePolicy,
    de::read_to_end_with_options,
    error::utf8_error_span,
    key_identity::{DuplicateKeyTracker, check_duplicate_with_tracker_at_depth_limit},
    schema::{LoadOptions, Schema},
    yaml11,
};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    io::Read,
    mem,
    rc::Rc,
};

/// A raw parser event emitted while reading a YAML stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Start of a YAML stream.
    StreamStart,
    /// End of a YAML stream.
    StreamEnd,
    /// Start of a document.
    DocumentStart {
        /// Whether the document start marker was explicit.
        explicit: bool,
        /// Directives attached to this document.
        directives: EventDocumentDirectives,
        /// Source span for the document start.
        span: Span,
    },
    /// End of a document.
    DocumentEnd {
        /// Whether the document end marker was explicit.
        explicit: bool,
        /// Source span for the document end.
        span: Span,
    },
    /// Start of a sequence.
    SequenceStart {
        /// Anchor and tag metadata for the sequence.
        meta: EventMeta,
        /// Block or flow collection style.
        style: CollectionStyle,
        /// Source span for the sequence start.
        span: Span,
    },
    /// End of a sequence.
    SequenceEnd {
        /// Source span for the sequence end.
        span: Span,
    },
    /// Start of a mapping.
    MappingStart {
        /// Anchor and tag metadata for the mapping.
        meta: EventMeta,
        /// Block or flow collection style.
        style: CollectionStyle,
        /// Source span for the mapping start.
        span: Span,
    },
    /// End of a mapping.
    MappingEnd {
        /// Source span for the mapping end.
        span: Span,
    },
    /// Alias reference.
    Alias {
        /// Referenced anchor name and span.
        anchor: EventAnchor,
    },
    /// Scalar value.
    Scalar {
        /// Resolved scalar text.
        value: String,
        /// Original scalar style.
        style: ScalarStyle,
        /// Anchor and tag metadata for the scalar.
        meta: EventMeta,
        /// Source span for the scalar.
        span: Span,
    },
}

/// Directive metadata attached to a document start event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventDocumentDirectives {
    /// YAML version directive, if present.
    pub yaml_version: Option<EventYamlVersion>,
    /// Tag directives active for the document.
    pub tag_directives: Vec<EventTagDirective>,
}

/// YAML version directive captured from a document header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventYamlVersion {
    /// Major version number.
    pub major: u8,
    /// Minor version number.
    pub minor: u8,
    /// Source span of the version declaration.
    pub span: Span,
}

/// `%TAG` directive captured from a document header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventTagDirective {
    /// Tag handle such as `!` or `!!`.
    pub handle: String,
    /// Tag prefix bound to the handle.
    pub prefix: String,
    /// Source span for the handle.
    pub handle_span: Span,
    /// Source span for the prefix.
    pub prefix_span: Span,
    /// Source span for the full directive.
    pub span: Span,
}

/// Anchor and tag metadata attached to a raw event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventMeta {
    /// Anchor metadata, if present.
    pub anchor: Option<EventAnchor>,
    /// Tag metadata, if present.
    pub tag: Option<EventTag>,
}

/// Anchor metadata captured from a raw event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAnchor {
    /// Anchor name without the leading `&`.
    pub name: String,
    /// Source span of the anchor name.
    pub span: Span,
}

/// Tag metadata captured from a raw event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventTag {
    /// Parsed YAML tag.
    pub tag: Tag,
    /// Source span of the tag token.
    pub span: Span,
}

/// Scalar style preserved by [`parse_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarStyle {
    /// Plain scalar style.
    Plain,
    /// Single-quoted scalar style.
    SingleQuoted,
    /// Double-quoted scalar style.
    DoubleQuoted,
    /// Literal block scalar style.
    Literal,
    /// Folded block scalar style.
    Folded,
}

/// Collection style preserved by [`parse_events`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionStyle {
    /// Block-style collection.
    Block,
    /// Flow-style collection.
    Flow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineKind {
    Blank,
    Content,
    Directive,
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Copy, Debug)]
struct LineText {
    start: u32,
    end: u32,
}

impl LineText {
    #[inline]
    fn new(start: usize, end: usize) -> Result<Self> {
        Ok(Self {
            start: compact_offset(start)?,
            end: compact_offset(end)?,
        })
    }

    #[inline]
    fn len(&self) -> usize {
        (self.end - self.start) as usize
    }

    #[inline]
    fn as_str<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }
}

#[derive(Clone, Copy, Debug)]
struct Line {
    raw: LineText,
    no: u32,
    indent: u32,
    content_start: u32,
    content_end: u32,
    kind: LineKind,
    had_comment: bool,
}

impl Line {
    #[inline]
    fn start(&self) -> usize {
        self.raw.start as usize
    }

    #[inline]
    fn no(&self) -> usize {
        self.no as usize
    }

    #[inline]
    fn indent(&self) -> usize {
        self.indent as usize
    }

    #[inline]
    fn content_start(&self) -> usize {
        self.content_start as usize
    }

    #[inline]
    fn content_end(&self) -> usize {
        self.content_end as usize
    }

    #[inline]
    fn raw<'a>(&self, source: &'a str) -> &'a str {
        self.raw.as_str(source)
    }

    #[inline]
    fn content<'a>(&self, source: &'a str) -> &'a str {
        &self.raw(source)[self.content_start()..self.content_end()]
    }

    #[inline]
    fn raw_len(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    fn content_len(&self) -> usize {
        self.content_end() - self.content_start()
    }

    #[inline]
    fn span(&self) -> Span {
        Span::new(
            self.start() + self.content_start(),
            self.start() + self.content_start() + self.content_len(),
            self.no(),
            self.content_start() + 1,
        )
    }

    #[inline]
    fn local_span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.start() + self.content_start() + start,
            self.start() + self.content_start() + end,
            self.no(),
            self.content_start() + start + 1,
        )
    }

    #[inline]
    fn raw_from<'a>(&self, source: &'a str, indent: usize) -> &'a str {
        let raw = self.raw(source);
        if indent >= self.raw_len() {
            ""
        } else {
            &raw[indent..]
        }
    }

    #[inline]
    fn raw_content_from<'a>(&self, source: &'a str, local_start: usize) -> &'a str {
        let raw = self.raw(source);
        let start = self.content_start() + local_start;
        if start >= self.raw_len() {
            ""
        } else {
            &raw[start..]
        }
    }
}

struct LineBuffer {
    lines: Vec<Line>,
    base: usize,
    next_start: usize,
    next_no: usize,
    exhausted: bool,
    reclaim_consumed: bool,
    #[cfg(test)]
    max_retained: usize,
}

impl LineBuffer {
    #[inline]
    fn new_lazy() -> Self {
        Self {
            lines: Vec::new(),
            base: 0,
            next_start: 0,
            next_no: 1,
            exhausted: false,
            reclaim_consumed: true,
            #[cfg(test)]
            max_retained: 0,
        }
    }

    fn new_eager(source: &str) -> Result<Self> {
        let line_estimate = source.bytes().filter(|&byte| byte == b'\n').count() + 1;
        let mut buffer = Self {
            lines: Vec::with_capacity(line_estimate),
            base: 0,
            next_start: 0,
            next_no: 1,
            exhausted: false,
            reclaim_consumed: false,
            #[cfg(test)]
            max_retained: 0,
        };
        while !buffer.exhausted {
            buffer.push_next(source)?;
        }
        Ok(buffer)
    }

    #[inline]
    fn get(&mut self, source: &str, pos: usize) -> Result<Option<&Line>> {
        debug_assert!(
            pos >= self.base,
            "line position {pos} was requested after it was discarded"
        );
        if pos < self.base {
            return Err(Error::new(
                "internal parser error: requested a source line that was already reclaimed",
                None,
            ));
        }
        self.fill_to(source, pos)?;
        Ok(self.lines.get(pos - self.base))
    }

    #[inline]
    fn discard_before(&mut self, pos: usize) {
        if !self.reclaim_consumed {
            return;
        }
        let drop = pos.saturating_sub(self.base).min(self.lines.len());
        if drop > 0 {
            self.lines.drain(..drop);
            self.base += drop;
        }
    }

    #[cfg(test)]
    fn retained_len(&self) -> usize {
        self.lines.len()
    }

    #[cfg(test)]
    fn max_retained_len(&self) -> usize {
        self.max_retained
    }

    #[inline]
    fn fill_to(&mut self, source: &str, pos: usize) -> Result<()> {
        while !self.exhausted && self.base + self.lines.len() <= pos {
            self.push_next(source)?;
        }
        Ok(())
    }

    #[inline]
    fn push_next(&mut self, source: &str) -> Result<()> {
        if self.next_start >= source.len() {
            self.exhausted = true;
            return Ok(());
        }

        let bytes = source.as_bytes();
        let start = self.next_start;
        let mut newline = start;
        while newline < bytes.len() && bytes[newline] != b'\n' {
            newline += 1;
        }
        let (raw_len, next_start, exhausted) = if newline < bytes.len() {
            let mut raw_len = newline - start;
            if raw_len > 0 && bytes[newline - 1] == b'\r' {
                raw_len -= 1;
            }
            (raw_len, newline + 1, false)
        } else {
            (source.len() - start, source.len(), true)
        };
        self.lines
            .push(preprocess_line(source, self.next_no, start, raw_len)?);
        #[cfg(test)]
        {
            self.max_retained = self.max_retained.max(self.lines.len());
        }
        self.next_start = next_start;
        self.next_no += 1;
        self.exhausted = exhausted;
        Ok(())
    }
}

fn compact_offset(offset: usize) -> Result<u32> {
    u32::try_from(offset)
        .map_err(|_| Error::limit("input is too large for the compact parser line table", None))
}

fn quoted_line_text(
    source: &str,
    line: &Line,
    local_start: usize,
    trimmed_text: &str,
    quote: char,
) -> String {
    let mut text = trimmed_text.to_string();
    if quote != '"' || trailing_backslash_count(&text).is_multiple_of(2) {
        return text;
    }

    let raw_text = line.raw_content_from(source, local_start);
    let Some(stripped) = raw_text.strip_prefix(trimmed_text) else {
        return text;
    };
    if let Some(ch @ (' ' | '\t')) = stripped.chars().next() {
        text.push(ch);
    }
    text
}

fn tab_indentation_error(line: &Line) -> Error {
    Error::syntax(
        "tabs are not allowed for indentation",
        Span::point(line.start() + line.indent(), line.no(), line.indent() + 1),
    )
}

/// Parses a single YAML document from UTF-8 bytes.
pub fn parse_bytes(input: &[u8]) -> Result<Node> {
    parse_bytes_with_options(input, LoadOptions::new())
}

pub(crate) fn parse_bytes_with_options(input: &[u8], options: LoadOptions) -> Result<Node> {
    options.check_input_len(input.len())?;
    match std::str::from_utf8(input) {
        Ok(input) => parse_str_with_options(input, options),
        Err(err) => Err(Error::encoding(
            "input is not valid UTF-8",
            utf8_error_span(input, err),
        )),
    }
}

/// Parses a single YAML document from a string.
pub fn parse_str(input: &str) -> Result<Node> {
    parse_str_with_options(input, LoadOptions::new())
}

pub(crate) fn parse_str_with_options(input: &str, options: LoadOptions) -> Result<Node> {
    let docs = parse_documents_with_options(input, options)?;
    match docs.len() {
        0 => Ok(Node::null(Span::point(0, 1, 1))),
        1 => Ok(docs.into_iter().next().expect("length checked")),
        _ => Err(Error::new(
            "expected a single YAML document; use parse_documents for streams",
            docs[1].span,
        )),
    }
}

/// Parses all documents in a YAML stream.
pub fn parse_documents(input: &str) -> Result<Vec<Node>> {
    parse_documents_with_options(input, LoadOptions::new())
}

pub(crate) fn parse_documents_with_options(input: &str, options: LoadOptions) -> Result<Vec<Node>> {
    parse_document_results_with_options(input, options)
        .into_iter()
        .collect()
}

/// Parses all documents into spanless trees that can borrow scalar strings from `input`.
///
/// This additive retained-output path keeps semantic loading behavior aligned
/// with [`parse_documents`], including merge-key expansion, duplicate-key
/// checks, alias budgets, and schema selection. Unlike [`Node`], returned
/// [`BorrowedNode`] values do not retain spans or raw scalar source spellings.
pub fn parse_borrowed_documents(input: &str) -> Result<Vec<BorrowedNode<'_>>> {
    parse_borrowed_documents_with_options(input, LoadOptions::new())
}

pub(crate) fn parse_borrowed_documents_with_options(
    input: &str,
    options: LoadOptions,
) -> Result<Vec<BorrowedNode<'_>>> {
    let mut parser =
        StreamingParser::new_with_scalar_storage(input, options, ScalarStorage::SourceBacked)?;
    let mut docs = Vec::new();
    while let Some(result) = parser.next_raw_document() {
        let mut node = result?;
        let schema = parser.last_document_schema();
        if parser.last_document_has_merge_key() {
            node.apply_merge_keys_with_policy(merge_policy_for_schema(schema))?;
        }
        docs.push(BorrowedNode::from_node(input, node));
    }
    docs.shrink_to_fit();
    Ok(docs)
}

pub(crate) fn parse_document_results_with_options(
    input: &str,
    options: LoadOptions,
) -> Vec<Result<Node>> {
    parse_document_results_with_scalar_storage(input, options, ScalarStorage::Owned)
}

fn parse_document_results_with_scalar_storage(
    input: &str,
    options: LoadOptions,
    scalar_storage: ScalarStorage,
) -> Vec<Result<Node>> {
    match Parser::new_eager_with_scalar_storage(input, options, scalar_storage) {
        Ok(mut parser) => {
            let results = parser.parse_document_results();
            let schemas = mem::take(&mut parser.document_schemas);
            let has_merge_keys = mem::take(&mut parser.document_has_merge_keys);
            let results = apply_merge_keys_to_document_results(results, schemas, has_merge_keys);
            if scalar_storage == ScalarStorage::Owned {
                results
                    .into_iter()
                    .map(|result| result.map(Node::into_public))
                    .collect()
            } else {
                results
            }
        }
        Err(error) => vec![Err(error)],
    }
}

fn apply_merge_keys_to_document_results(
    results: Vec<Result<Node>>,
    schemas: Vec<Schema>,
    has_merge_keys: Vec<bool>,
) -> Vec<Result<Node>> {
    let mut schemas = schemas.into_iter();
    let mut has_merge_keys = has_merge_keys.into_iter();
    results
        .into_iter()
        .map(|result| {
            result.and_then(|mut node| {
                let schema = schemas.next().unwrap_or(Schema::Yaml12);
                if has_merge_keys.next().unwrap_or(true) {
                    node.apply_merge_keys_with_policy(merge_policy_for_schema(schema))?;
                }
                Ok(node)
            })
        })
        .collect()
}

const INLINE_COLLECTION_LIMIT: usize = 4;

struct NodeItems {
    inline: [Option<Node>; INLINE_COLLECTION_LIMIT],
    len: usize,
    overflow: Option<Vec<Node>>,
}

impl NodeItems {
    fn new() -> Self {
        Self {
            inline: [None, None, None, None],
            len: 0,
            overflow: None,
        }
    }

    fn len(&self) -> usize {
        self.overflow.as_ref().map_or(self.len, |items| items.len())
    }

    fn last(&self) -> Option<&Node> {
        if let Some(items) = &self.overflow {
            return items.last();
        }
        self.len
            .checked_sub(1)
            .and_then(|index| self.inline[index].as_ref())
    }

    fn push(&mut self, item: Node) {
        if let Some(items) = &mut self.overflow {
            items.push(item);
            return;
        }
        if self.len < INLINE_COLLECTION_LIMIT {
            self.inline[self.len] = Some(item);
            self.len += 1;
            return;
        }
        let mut items = Vec::with_capacity(INLINE_COLLECTION_LIMIT * 2);
        for slot in &mut self.inline {
            if let Some(item) = slot.take() {
                items.push(item);
            }
        }
        items.push(item);
        self.overflow = Some(items);
    }

    fn into_vec(mut self) -> Vec<Node> {
        if let Some(items) = self.overflow {
            return items;
        }
        let mut items = Vec::with_capacity(self.len);
        for slot in &mut self.inline {
            if let Some(item) = slot.take() {
                items.push(item);
            }
        }
        items
    }
}

struct NodeEntries {
    inline: [Option<(Node, Node)>; INLINE_COLLECTION_LIMIT],
    len: usize,
    overflow: Option<Vec<(Node, Node)>>,
}

impl NodeEntries {
    fn new() -> Self {
        Self {
            inline: [None, None, None, None],
            len: 0,
            overflow: None,
        }
    }

    fn len(&self) -> usize {
        self.overflow
            .as_ref()
            .map_or(self.len, |entries| entries.len())
    }

    fn last(&self) -> Option<&(Node, Node)> {
        if let Some(entries) = &self.overflow {
            return entries.last();
        }
        self.len
            .checked_sub(1)
            .and_then(|index| self.inline[index].as_ref())
    }

    fn push(&mut self, entry: (Node, Node)) {
        if let Some(entries) = &mut self.overflow {
            entries.push(entry);
            return;
        }
        if self.len < INLINE_COLLECTION_LIMIT {
            self.inline[self.len] = Some(entry);
            self.len += 1;
            return;
        }
        let mut entries = Vec::with_capacity(INLINE_COLLECTION_LIMIT * 2);
        for slot in &mut self.inline {
            if let Some(entry) = slot.take() {
                entries.push(entry);
            }
        }
        entries.push(entry);
        self.overflow = Some(entries);
    }

    fn into_vec(mut self) -> Vec<(Node, Node)> {
        if let Some(entries) = self.overflow {
            return entries;
        }
        let mut entries = Vec::with_capacity(self.len);
        for slot in &mut self.inline {
            if let Some(entry) = slot.take() {
                entries.push(entry);
            }
        }
        entries
    }
}

fn sequence_node(mut items: Vec<Node>, span: Span) -> Node {
    items.shrink_to_fit();
    Node::new(Value::Sequence(items), span)
}

fn mapping_node(mut entries: Vec<(Node, Node)>, span: Span) -> Node {
    entries.shrink_to_fit();
    Node::new(Value::Mapping(entries), span)
}

/// Parses a YAML stream and returns raw structural events.
pub fn parse_events(input: &str) -> Result<Vec<Event>> {
    EventStream::from_str(input)?.collect()
}

/// Creates a pull-based stream of raw parser events from a YAML string.
pub fn stream_events(input: &str) -> Result<EventStream> {
    EventStream::from_str(input)
}

/// Creates a pull-based stream of raw parser events from UTF-8 YAML bytes.
pub fn stream_events_slice(input: &[u8]) -> Result<EventStream> {
    EventStream::from_slice(input)
}

/// Reads YAML bytes and creates a pull-based stream of raw parser events.
pub fn stream_events_reader<R>(reader: R) -> Result<EventStream>
where
    R: Read,
{
    EventStream::from_reader(reader)
}

/// Creates a pull-based stream of parsed YAML documents from a YAML string.
pub fn stream_documents(input: &str) -> Result<DocumentStream> {
    DocumentStream::from_str(input)
}

/// Creates a pull-based stream of parsed YAML documents from UTF-8 YAML bytes.
pub fn stream_documents_slice(input: &[u8]) -> Result<DocumentStream> {
    DocumentStream::from_slice(input)
}

/// Reads YAML bytes and creates a pull-based stream of parsed YAML documents.
pub fn stream_documents_reader<R>(reader: R) -> Result<DocumentStream>
where
    R: Read,
{
    DocumentStream::from_reader(reader)
}

/// Pull-based iterator over raw YAML parser events.
///
/// `EventStream` yields the same event sequence as [`parse_events`] for a
/// successfully parsed input without retaining the completed event vector.
/// Invalid inputs may yield already-parsed prefix events before the terminal
/// error. Event streaming validates aliases but never expands them, so
/// `LoadOptions` input limits apply while alias expansion budgets are reserved
/// for semantic document loading. The source input is still fully buffered;
/// streaming bounds the retained parsed representation, not source bytes.
pub struct EventStream {
    parser: StreamingParser,
    pending: VecDeque<Event>,
    pending_error: Option<Error>,
    finished: bool,
}

impl EventStream {
    /// Creates an event stream from a YAML string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &str) -> Result<Self> {
        Self::from_str_with_options(input, LoadOptions::new())
    }

    /// Creates an event stream from a YAML string with explicit load options.
    pub fn from_str_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        let mut parser = StreamingParser::new(input, options)?;
        parser.enable_events();
        let pending = parser.take_events();
        Ok(Self {
            parser,
            pending,
            pending_error: None,
            finished: false,
        })
    }

    /// Creates an event stream from UTF-8 YAML bytes.
    pub fn from_slice(input: &[u8]) -> Result<Self> {
        Self::from_slice_with_options(input, LoadOptions::new())
    }

    /// Creates an event stream from UTF-8 YAML bytes with explicit load options.
    pub fn from_slice_with_options(input: &[u8], options: LoadOptions) -> Result<Self> {
        options.check_input_len(input.len())?;
        let input = std::str::from_utf8(input).map_err(|err| {
            Error::encoding("input is not valid UTF-8", utf8_error_span(input, err))
        })?;
        Self::from_str_with_options(input, options)
    }

    /// Reads YAML bytes and creates an event stream.
    pub fn from_reader<R>(reader: R) -> Result<Self>
    where
        R: Read,
    {
        Self::from_reader_with_options(reader, LoadOptions::new())
    }

    /// Reads YAML bytes and creates an event stream with explicit load options.
    pub fn from_reader_with_options<R>(reader: R, options: LoadOptions) -> Result<Self>
    where
        R: Read,
    {
        let input = read_to_end_with_options(reader, options)?;
        Self::from_slice_with_options(&input, options)
    }
}

impl Iterator for EventStream {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(Ok(event));
            }
            if let Some(error) = self.pending_error.take() {
                return Some(Err(error));
            }
            if self.finished {
                return None;
            }

            match self.parser.next_raw_document() {
                Some(Ok(_)) => {
                    self.pending = self.parser.take_events();
                }
                Some(Err(error)) => {
                    self.pending = self.parser.take_events();
                    self.pending_error = Some(error);
                    self.finished = true;
                }
                None => {
                    self.pending = self.parser.take_events();
                    self.pending.push_back(Event::StreamEnd);
                    self.finished = true;
                }
            }
        }
    }
}

/// Pull-based iterator over semantic YAML documents.
///
/// `DocumentStream` applies the same scalar schema, merge-key behavior,
/// duplicate-key checks, input byte ceiling, and alias expansion budget as
/// [`parse_documents`], but yields one completed document at a time instead of
/// retaining a `Vec<Node>`. The source input is still fully buffered; streaming
/// bounds the retained parsed representation, not source bytes.
pub struct DocumentStream {
    parser: StreamingParser,
    finished: bool,
}

impl DocumentStream {
    /// Creates a document stream from a YAML string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &str) -> Result<Self> {
        Self::from_str_with_options(input, LoadOptions::new())
    }

    /// Creates a document stream from a YAML string with explicit load options.
    pub fn from_str_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        Ok(Self {
            parser: StreamingParser::new(input, options)?,
            finished: false,
        })
    }

    /// Creates a document stream from UTF-8 YAML bytes.
    pub fn from_slice(input: &[u8]) -> Result<Self> {
        Self::from_slice_with_options(input, LoadOptions::new())
    }

    /// Creates a document stream from UTF-8 YAML bytes with explicit load options.
    pub fn from_slice_with_options(input: &[u8], options: LoadOptions) -> Result<Self> {
        options.check_input_len(input.len())?;
        let input = std::str::from_utf8(input).map_err(|err| {
            Error::encoding("input is not valid UTF-8", utf8_error_span(input, err))
        })?;
        Self::from_str_with_options(input, options)
    }

    /// Reads YAML bytes and creates a document stream.
    pub fn from_reader<R>(reader: R) -> Result<Self>
    where
        R: Read,
    {
        Self::from_reader_with_options(reader, LoadOptions::new())
    }

    /// Reads YAML bytes and creates a document stream with explicit load options.
    pub fn from_reader_with_options<R>(reader: R, options: LoadOptions) -> Result<Self>
    where
        R: Read,
    {
        let input = read_to_end_with_options(reader, options)?;
        Self::from_slice_with_options(&input, options)
    }
}

impl Iterator for DocumentStream {
    type Item = Result<Node>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        match self.parser.next_raw_document() {
            Some(Ok(mut node)) => {
                let schema = self.parser.last_document_schema();
                if self.parser.last_document_has_merge_key() {
                    Some(
                        node.apply_merge_keys_with_policy(merge_policy_for_schema(schema))
                            .map(|()| node),
                    )
                } else {
                    Some(Ok(node))
                }
            }
            Some(Err(error)) => {
                self.finished = true;
                Some(Err(error))
            }
            None => {
                self.finished = true;
                None
            }
        }
    }
}

struct StreamingParser {
    parser: Parser,
    state: DocumentParseState,
    current_schema: Schema,
    current_document_has_merge_key: bool,
}

impl StreamingParser {
    fn new(input: &str, options: LoadOptions) -> Result<Self> {
        Self::new_with_scalar_storage(input, options, ScalarStorage::Owned)
    }

    fn new_with_scalar_storage(
        input: &str,
        options: LoadOptions,
        scalar_storage: ScalarStorage,
    ) -> Result<Self> {
        Ok(Self {
            parser: Parser::new_with_scalar_storage(input, options, scalar_storage)?,
            state: DocumentParseState::default(),
            current_schema: Schema::Yaml12,
            current_document_has_merge_key: false,
        })
    }

    fn enable_events(&mut self) {
        self.parser.enable_events();
    }

    fn next_raw_document(&mut self) -> Option<Result<Node>> {
        let schema_count = self.parser.document_schemas.len();
        let merge_key_count = self.parser.document_has_merge_keys.len();
        let result = self.parser.parse_next_document_result(&mut self.state);
        if matches!(result, Some(Ok(_))) {
            self.current_schema = self
                .parser
                .document_schemas
                .pop()
                .expect("parsed document records schema");
            self.current_document_has_merge_key = self
                .parser
                .document_has_merge_keys
                .pop()
                .expect("parsed document records merge-key status");
        }
        debug_assert_eq!(self.parser.document_schemas.len(), schema_count);
        debug_assert_eq!(self.parser.document_has_merge_keys.len(), merge_key_count);
        result
    }

    fn take_events(&mut self) -> VecDeque<Event> {
        self.parser
            .events
            .as_mut()
            .map(|recorder| recorder.events.drain(..).collect())
            .unwrap_or_default()
    }

    fn last_document_schema(&self) -> Schema {
        self.current_schema
    }

    fn last_document_has_merge_key(&self) -> bool {
        self.current_document_has_merge_key
    }
}

#[derive(Default)]
struct DocumentParseState {
    pending_start: Option<(Span, bool, EventDocumentDirectives)>,
    open_document: Option<Span>,
    pending_directive_span: Option<Span>,
    finished: bool,
}

struct Parser {
    source: Rc<str>,
    lines: LineBuffer,
    pos: usize,
    input_len: usize,
    options: LoadOptions,
    schema: Schema,
    active_schema: Schema,
    anchors: AnchorRegistry,
    events: Option<EventRecorder>,
    active_tag_handles: HashMap<String, String>,
    pending_tag_handles: HashMap<String, String>,
    pending_document_directives: EventDocumentDirectives,
    pending_directives: bool,
    document_schemas: Vec<Schema>,
    document_has_merge_keys: Vec<bool>,
    current_document_has_merge_key: bool,
    scalar_storage: ScalarStorage,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ScalarStorage {
    Owned,
    SourceBacked,
}

#[derive(Clone, Copy)]
enum LinePreprocessing {
    Eager,
    Lazy,
}

#[derive(Default)]
struct EventRecorder {
    events: Vec<Event>,
    pending_meta: EventMeta,
}

impl EventRecorder {
    fn take_meta(&mut self) -> EventMeta {
        mem::take(&mut self.pending_meta)
    }
}

struct MetadataToken<'a> {
    name: String,
    span: Span,
    rest: &'a str,
    rest_start: usize,
}

struct TagToken<'a> {
    tag: Tag,
    span: Span,
    rest: &'a str,
    rest_start: usize,
}

#[derive(Default)]
struct NodePropertyState {
    anchor: Option<Span>,
    tag: Option<Span>,
    latest: Option<Span>,
    allow_document_root_continuation: bool,
    pending_anchor: Option<PendingAnchor>,
}

struct PendingAnchor {
    name: String,
    generation: usize,
}

impl NodePropertyState {
    fn record_anchor(&mut self, span: Span) -> Result<()> {
        if self.anchor.is_some() {
            return Err(Error::new(
                "duplicate anchor property on the same node",
                span,
            ));
        }
        self.anchor = Some(span);
        self.latest = Some(span);
        Ok(())
    }

    fn record_tag(&mut self, span: Span) -> Result<()> {
        if self.tag.is_some() {
            return Err(Error::new("duplicate tag property on the same node", span));
        }
        self.tag = Some(span);
        self.latest = Some(span);
        Ok(())
    }

    fn defer_anchor_finish(&mut self, name: String, generation: usize) {
        debug_assert!(self.pending_anchor.is_none());
        self.pending_anchor = Some(PendingAnchor { name, generation });
    }
}

#[derive(Clone)]
enum AnchorEntry {
    InProgress { span: Span, generation: usize },
    Complete { node: Node },
}

struct AnchorRegistry {
    entries: HashMap<String, AnchorEntry>,
    generation: usize,
    expanded_nodes: usize,
    expansion_budget: usize,
    options: LoadOptions,
}

impl AnchorRegistry {
    fn new(expansion_budget: usize, options: LoadOptions) -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            expanded_nodes: 0,
            expansion_budget,
            options,
        }
    }

    fn reset_document(&mut self) {
        self.entries.clear();
        self.expanded_nodes = 0;
    }

    fn begin(&mut self, name: String, span: Span) -> usize {
        self.generation += 1;
        let generation = self.generation;
        self.entries
            .insert(name, AnchorEntry::InProgress { span, generation });
        generation
    }

    fn finish(&mut self, name: &str, generation: usize, node: Node) {
        if matches!(
            self.entries.get(name),
            Some(AnchorEntry::InProgress {
                generation: current,
                ..
            }) if *current == generation
        ) {
            self.entries
                .insert(name.to_string(), AnchorEntry::Complete { node });
        }
    }

    fn resolve(&mut self, name: &str, span: Span, depth: usize) -> Result<Node> {
        let target = match self.entries.get(name) {
            Some(AnchorEntry::Complete { node }) => node,
            Some(AnchorEntry::InProgress {
                span: anchor_span, ..
            }) => {
                return Err(Error::with_related_category(
                    format!("recursive alias `{name}` is not supported"),
                    span,
                    "anchor is still being parsed here",
                    *anchor_span,
                    crate::ErrorCategory::Reference,
                ));
            }
            None => {
                return Err(Error::reference(format!("unknown anchor `{name}`"), span));
            }
        };

        let node_count = count_nodes(target);
        self.expanded_nodes = self.expanded_nodes.saturating_add(node_count);
        if self.expanded_nodes > self.expansion_budget {
            return Err(Error::limit("alias expansion limit exceeded", span));
        }
        if self
            .options
            .selected_max_nesting_depth()
            .is_some_and(|max| depth.saturating_add(node_depth(target)) > max)
        {
            return Err(Error::limit(
                "maximum YAML nesting depth exceeded while expanding alias",
                span,
            ));
        }

        let mut node = target.clone();
        node.span = span;
        Ok(node)
    }

    fn validate_alias(&self, name: &str, span: Span) -> Result<()> {
        if self.entries.contains_key(name) {
            return Ok(());
        }
        Err(Error::reference(format!("unknown anchor `{name}`"), span))
    }
}

impl Parser {
    fn new_eager_with_scalar_storage(
        input: &str,
        options: LoadOptions,
        scalar_storage: ScalarStorage,
    ) -> Result<Self> {
        Self::new_with_scalar_storage_and_lines(
            input,
            options,
            scalar_storage,
            LinePreprocessing::Eager,
        )
    }

    fn new_with_scalar_storage(
        input: &str,
        options: LoadOptions,
        scalar_storage: ScalarStorage,
    ) -> Result<Self> {
        Self::new_with_scalar_storage_and_lines(
            input,
            options,
            scalar_storage,
            LinePreprocessing::Lazy,
        )
    }

    fn new_with_scalar_storage_and_lines(
        input: &str,
        options: LoadOptions,
        scalar_storage: ScalarStorage,
        line_preprocessing: LinePreprocessing,
    ) -> Result<Self> {
        options.check_input_len(input.len())?;
        let source = Rc::<str>::from(input);
        let schema = options.selected_schema();
        let alias_expansion_budget = options.alias_expansion_budget(input.len());
        let lines = match line_preprocessing {
            LinePreprocessing::Eager => LineBuffer::new_eager(&source)?,
            LinePreprocessing::Lazy => LineBuffer::new_lazy(),
        };
        Ok(Self {
            source,
            lines,
            pos: 0,
            input_len: input.len(),
            options,
            schema,
            active_schema: default_construction_schema(schema),
            anchors: AnchorRegistry::new(alias_expansion_budget, options),
            events: None,
            active_tag_handles: HashMap::new(),
            pending_tag_handles: HashMap::new(),
            pending_document_directives: EventDocumentDirectives::default(),
            pending_directives: false,
            document_schemas: Vec::new(),
            document_has_merge_keys: Vec::new(),
            current_document_has_merge_key: false,
            scalar_storage,
        })
    }

    fn enable_events(&mut self) {
        self.events = Some(EventRecorder {
            events: vec![Event::StreamStart],
            pending_meta: EventMeta::default(),
        });
    }

    fn emit_document_start(
        &mut self,
        explicit: bool,
        directives: EventDocumentDirectives,
        span: Span,
    ) {
        if let Some(recorder) = &mut self.events {
            recorder.events.push(Event::DocumentStart {
                explicit,
                directives,
                span,
            });
        }
    }

    fn emit_document_end(&mut self, explicit: bool, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.events.push(Event::DocumentEnd { explicit, span });
        }
    }

    fn emit_sequence_start(&mut self, style: CollectionStyle, span: Span) {
        if let Some(recorder) = &mut self.events {
            let meta = recorder.take_meta();
            recorder
                .events
                .push(Event::SequenceStart { meta, style, span });
        }
    }

    fn emit_sequence_end(&mut self, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.events.push(Event::SequenceEnd { span });
        }
    }

    fn emit_mapping_start(&mut self, style: CollectionStyle, span: Span) {
        if let Some(recorder) = &mut self.events {
            let meta = recorder.take_meta();
            recorder
                .events
                .push(Event::MappingStart { meta, style, span });
        }
    }

    fn emit_mapping_end(&mut self, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.events.push(Event::MappingEnd { span });
        }
    }

    fn emit_alias(&mut self, name: String, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.events.push(Event::Alias {
                anchor: EventAnchor { name, span },
            });
        }
    }

    fn emit_null_scalar(&mut self, span: Span) {
        if let Some(recorder) = &mut self.events {
            let meta = recorder.take_meta();
            recorder.events.push(Event::Scalar {
                value: "null".to_string(),
                style: ScalarStyle::Plain,
                meta,
                span,
            });
        }
    }

    fn emit_scalar_node(&mut self, node: &Node, style: ScalarStyle) {
        if let Some(recorder) = &mut self.events {
            let meta = recorder.take_meta();
            recorder.events.push(Event::Scalar {
                value: event_scalar_value(node),
                style,
                meta,
                span: node.span,
            });
        }
    }

    fn push_anchor_meta(&mut self, name: String, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.pending_meta.anchor = Some(EventAnchor { name, span });
        }
    }

    fn push_tag_meta(&mut self, tag: Tag, span: Span) {
        if let Some(recorder) = &mut self.events {
            recorder.pending_meta.tag = Some(EventTag { tag, span });
        }
    }

    fn resolve_tag(&self, tag: Tag, span: Span) -> Result<Tag> {
        resolve_tag(&self.active_tag_handles, tag, span)
    }

    fn recording_events(&self) -> bool {
        self.events.is_some()
    }

    fn record_merge_key(&mut self, key: &Node) {
        if node_is_merge_key(key) {
            self.current_document_has_merge_key = true;
        }
    }

    #[inline]
    fn line_at(&mut self, pos: usize) -> Result<Option<Line>> {
        Ok(self.lines.get(&self.source, pos)?.copied())
    }

    #[inline]
    fn line_kind_at(&mut self, pos: usize) -> Result<Option<LineKind>> {
        Ok(self.lines.get(&self.source, pos)?.map(|line| line.kind))
    }

    fn check_duplicate_key(&mut self, seen: &mut DuplicateKeyTracker, key: &Node) -> Result<()> {
        self.record_merge_key(key);
        check_duplicate_for_schema(
            self.recording_events(),
            self.active_schema,
            self.options,
            seen,
            key,
        )
    }

    fn parse_document_results(&mut self) -> Vec<Result<Node>> {
        let mut docs = Vec::new();
        let mut state = DocumentParseState::default();
        while let Some(result) = self.parse_next_document_result(&mut state) {
            docs.push(result);
        }
        docs.shrink_to_fit();
        docs
    }

    fn parse_next_document_result(
        &mut self,
        state: &mut DocumentParseState,
    ) -> Option<Result<Node>> {
        if state.finished {
            return None;
        }

        loop {
            let line = match self.line_after_blanks() {
                Ok(Some(line)) => line,
                Ok(None) => {
                    if self.pending_directives {
                        state.finished = true;
                        return Some(Err(Error::new(
                            "directives must be followed by an explicit document start marker",
                            state.pending_directive_span.unwrap_or_default(),
                        )));
                    }
                    if let Some((span, explicit, directives)) = state.pending_start.take() {
                        self.activate_document_schema(&directives);
                        self.emit_document_start(explicit, directives, span);
                        self.emit_null_scalar(span);
                        self.emit_document_end(false, span);
                        return Some(Ok(self.finish_parsed_document(Node::null(span))));
                    }
                    if let Some(span) = state.open_document.take() {
                        self.emit_document_end(false, span);
                    }
                    state.finished = true;
                    return None;
                }
                Err(error) => {
                    state.finished = true;
                    return Some(Err(error));
                }
            };
            match line.kind {
                LineKind::Blank => unreachable!("blank lines are skipped above"),
                LineKind::Directive => {
                    if state.pending_start.is_some() {
                        state.finished = true;
                        return Some(Err(Error::new(
                            "directives must appear before the document start marker",
                            line.span(),
                        )));
                    }
                    if state.open_document.is_some() {
                        state.finished = true;
                        return Some(Err(Error::new(
                            "directives must appear before the document start marker",
                            line.span(),
                        )));
                    }
                    if let Err(error) = self.parse_directive(&line) {
                        state.finished = true;
                        return Some(Err(error));
                    }
                    if self.pending_directives {
                        state.pending_directive_span = Some(line.span());
                    }
                    self.pos += 1;
                }
                LineKind::DocumentStart => {
                    if let Some((span, explicit, directives)) = state.pending_start.take() {
                        self.emit_document_start(explicit, directives, span);
                        self.emit_null_scalar(span);
                        self.emit_document_end(false, span);
                        return Some(Ok(self.finish_parsed_document(Node::null(span))));
                    }
                    if let Some(span) = state.open_document.take() {
                        self.emit_document_end(false, span);
                    }
                    let directives = self.activate_pending_directives();
                    state.pending_directive_span = None;
                    let marker_span = line.local_span(0, 3);
                    self.pos += 1;
                    let source = Rc::clone(&self.source);
                    let content = line.content(&source);
                    if let Some((rest_start, rest)) = document_start_rest(content) {
                        self.activate_document_schema(&directives);
                        self.emit_document_start(true, directives, marker_span);
                        self.anchors.reset_document();
                        let doc = match self.parse_document_start_value(
                            &line,
                            rest_start,
                            rest,
                            line.indent(),
                            0,
                        ) {
                            Ok(doc) => doc,
                            Err(error) => {
                                state.finished = true;
                                return Some(Err(error));
                            }
                        };
                        state.open_document = Some(doc.span);
                        if let Err(error) = self.reject_trailing_content_after_document_node() {
                            state.finished = true;
                            return Some(Err(error));
                        }
                        return Some(Ok(self.finish_parsed_document(doc)));
                    } else {
                        state.pending_start = Some((marker_span, true, directives));
                    }
                }
                LineKind::DocumentEnd => {
                    if self.pending_directives {
                        state.finished = true;
                        return Some(Err(Error::new(
                            "directives must be followed by an explicit document start marker",
                            line.span(),
                        )));
                    }
                    if let Some((span, explicit, directives)) = state.pending_start.take() {
                        self.activate_document_schema(&directives);
                        self.emit_document_start(explicit, directives, span);
                        self.emit_null_scalar(span);
                        self.emit_document_end(true, line.span());
                        self.pos += 1;
                        return Some(Ok(self.finish_parsed_document(Node::null(span))));
                    }
                    if state.open_document.take().is_some() {
                        self.emit_document_end(true, line.span());
                    }
                    self.pos += 1;
                }
                LineKind::Content => {
                    if self.pending_directives {
                        state.finished = true;
                        return Some(Err(Error::new(
                            "directives must be followed by an explicit document start marker",
                            line.span(),
                        )));
                    }
                    if let Some(span) = state.open_document.take() {
                        self.emit_document_end(false, span);
                    }
                    let (start_span, explicit, directives) =
                        if let Some((span, explicit, directives)) = state.pending_start.take() {
                            (span, explicit, directives)
                        } else {
                            self.active_tag_handles.clear();
                            (line.span(), false, EventDocumentDirectives::default())
                        };
                    self.activate_document_schema(&directives);
                    self.emit_document_start(explicit, directives, start_span);
                    self.anchors.reset_document();
                    let doc = match self.parse_node(0) {
                        Ok(doc) => doc,
                        Err(error) => {
                            state.finished = true;
                            return Some(Err(error));
                        }
                    };
                    state.open_document = Some(doc.span);
                    if let Err(error) = self.reject_trailing_content_after_document_node() {
                        state.finished = true;
                        return Some(Err(error));
                    }
                    return Some(Ok(self.finish_parsed_document(doc)));
                }
            }
        }
    }

    fn finish_parsed_document(&mut self, doc: Node) -> Node {
        self.document_schemas.push(self.active_schema);
        self.document_has_merge_keys
            .push(self.current_document_has_merge_key);
        self.current_document_has_merge_key = false;
        self.lines.discard_before(self.pos);
        doc
    }

    fn parse_directive(&mut self, line: &Line) -> Result<()> {
        let fields = directive_fields(line, &self.source);
        match fields.as_slice() {
            [name, version] if name.text == "%YAML" => {
                let Some((major, minor)) = parse_yaml_version(version.text) else {
                    return Err(Error::new("invalid YAML directive", line.span()));
                };
                self.pending_directives = true;
                if self.pending_document_directives.yaml_version.is_some() {
                    return Err(Error::new("duplicate YAML directive", line.span()));
                }
                self.pending_document_directives.yaml_version = Some(EventYamlVersion {
                    major,
                    minor,
                    span: line.local_span(version.start, version.end),
                });
                Ok(())
            }
            [name, ..] if name.text == "%YAML" => {
                Err(Error::new("invalid YAML directive", line.span()))
            }
            [name, handle, prefix]
                if name.text == "%TAG"
                    && valid_tag_handle(handle.text)
                    && !prefix.text.is_empty() =>
            {
                self.pending_directives = true;
                if self.pending_tag_handles.contains_key(handle.text) {
                    return Err(Error::new("duplicate TAG directive handle", line.span()));
                }
                self.pending_tag_handles
                    .insert(handle.text.to_string(), prefix.text.to_string());
                self.pending_document_directives
                    .tag_directives
                    .push(EventTagDirective {
                        handle: handle.text.to_string(),
                        prefix: prefix.text.to_string(),
                        handle_span: line.local_span(handle.start, handle.end),
                        prefix_span: line.local_span(prefix.start, prefix.end),
                        span: line.span(),
                    });
                Ok(())
            }
            [name, ..] if name.text == "%TAG" => {
                Err(Error::new("invalid TAG directive", line.span()))
            }
            [_, ..] => {
                self.pending_directives = true;
                Ok(())
            }
            [] => unreachable!("directive lines are not empty"),
        }
    }

    fn activate_pending_directives(&mut self) -> EventDocumentDirectives {
        self.active_tag_handles = mem::take(&mut self.pending_tag_handles);
        self.pending_directives = false;
        mem::take(&mut self.pending_document_directives)
    }

    fn activate_document_schema(&mut self, directives: &EventDocumentDirectives) {
        self.active_schema = schema_for_directives(self.schema, directives);
    }

    fn reject_trailing_content_after_document_node(&mut self) -> Result<()> {
        if let Some(line) = self.peek_content()? {
            return Err(Error::new(
                "unexpected content after root document node",
                line.span(),
            ));
        }
        Ok(())
    }

    fn parse_document_start_value(
        &mut self,
        line: &Line,
        local_start: usize,
        text: &str,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        let mut properties = NodePropertyState::default();
        self.parse_document_start_value_with_properties(
            line,
            local_start,
            text,
            parent_indent,
            depth,
            &mut properties,
        )
    }

    fn parse_document_start_value_with_properties(
        &mut self,
        line: &Line,
        local_start: usize,
        text: &str,
        parent_indent: usize,
        depth: usize,
        properties: &mut NodePropertyState,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let leading_ws = text.len() - text.trim_start().len();
        let text = text.trim();
        let local_start = local_start + leading_ws;

        if text.is_empty() {
            return self.parse_document_root_or_null(line.span(), depth + 1);
        }
        if let Some(alias) = parse_metadata_token(text, line, local_start, '*')? {
            if !alias.rest.trim().is_empty() {
                return Err(Error::new(
                    "unexpected content after alias",
                    line.local_span(alias.rest_start, alias.rest_start + alias.rest.len()),
                ));
            }
            self.emit_alias(alias.name.clone(), alias.span);
            if self.recording_events() {
                self.anchors.validate_alias(&alias.name, alias.span)?;
                return Ok(Node::null(alias.span));
            }
            return self.anchors.resolve(&alias.name, alias.span, depth);
        }
        if let Some(anchor) = parse_metadata_token(text, line, local_start, '&')? {
            let anchor_after_tag = properties.tag.is_some();
            properties.record_anchor(anchor.span)?;
            reject_alias_with_node_properties(anchor.rest, line, anchor.rest_start)?;
            reject_same_line_block_sequence_after_property(anchor.rest, line, anchor.rest_start)?;
            self.push_anchor_meta(anchor.name.clone(), anchor.span);
            let generation = self.anchors.begin(anchor.name.clone(), anchor.span);
            let node = if anchor.rest.trim().is_empty() {
                self.parse_document_root_or_null(anchor.span, depth + 1)?
            } else {
                self.parse_document_start_value_with_properties(
                    line,
                    anchor.rest_start,
                    anchor.rest,
                    parent_indent,
                    depth + 1,
                    properties,
                )?
            };
            if anchor_after_tag {
                properties.defer_anchor_finish(anchor.name, generation);
                return Ok(node);
            }
            self.anchors.finish(&anchor.name, generation, node.clone());
            return Ok(node);
        }
        if let Some(tag) = parse_tag_token(text, line, local_start)? {
            let tag_value = self.resolve_tag(tag.tag, tag.span)?;
            properties.record_tag(tag.span)?;
            reject_alias_with_node_properties(tag.rest, line, tag.rest_start)?;
            reject_same_line_block_sequence_after_property(tag.rest, line, tag.rest_start)?;
            self.push_tag_meta(tag_value.clone(), tag.span);
            let node = if tag.rest.trim().is_empty() {
                self.parse_document_root_or_null(tag.span, depth + 1)?
            } else {
                self.parse_document_start_value_with_properties(
                    line,
                    tag.rest_start,
                    tag.rest,
                    parent_indent,
                    depth + 1,
                    properties,
                )?
            };
            let node = tagged_node(tag_value, tag.span, node);
            self.finish_deferred_anchor(properties, &node);
            return Ok(node);
        }
        if let Some(header) = parse_block_scalar_header(text, line, local_start)? {
            return self.parse_block_scalar(
                header,
                parent_indent,
                line.local_span(local_start, local_start + text.len()),
                depth + 1,
                parent_indent == 0 && matches!(line.kind, LineKind::DocumentStart),
            );
        }
        if sequence_rest(text).is_some() {
            return self.parse_inline_sequence_item(line, local_start, text, depth + 1);
        }
        if let Some(colon) = find_mapping_col(text) {
            return Err(Error::new(
                "mapping values are not allowed in this context",
                line.local_span(local_start + colon, local_start + colon + 1),
            ));
        }
        if let Some(quote @ ('"' | '\'')) = text.chars().next() {
            if let Some(trailing_start) = quoted_scalar_trailing_start(text, quote) {
                return Err(Error::new(
                    "unexpected trailing characters after quoted scalar",
                    line.local_span(local_start + trailing_start, local_start + text.len()),
                ));
            }
            if !quoted_scalar_is_closed(text, quote) {
                return self.parse_multiline_quoted_scalar(
                    text,
                    line,
                    local_start,
                    parent_indent,
                    quote,
                    true,
                );
            }
        }
        self.parse_inline_value(text, line, local_start, parent_indent, depth + 1)
    }

    fn finish_deferred_anchor(&mut self, properties: &mut NodePropertyState, node: &Node) {
        if let Some(anchor) = properties.pending_anchor.take() {
            self.anchors
                .finish(&anchor.name, anchor.generation, node.clone());
        }
    }

    fn parse_document_root_or_null(&mut self, span: Span, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        self.skip_blanks()?;
        if self.peek_content()?.is_some() {
            self.parse_node(depth)
        } else {
            self.emit_null_scalar(span);
            Ok(Node::null(span))
        }
    }

    fn parse_node(&mut self, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let line = self.current_content()?;
        let source = Rc::clone(&self.source);
        let content = line.content(&source);
        if content.starts_with('\t') {
            if line.indent() == 0 && root_tab_separated_flow_collection(content) {
                self.pos += 1;
                return self.parse_document_root_inline_value(
                    content,
                    &line,
                    0,
                    line.indent(),
                    depth,
                );
            }
            return Err(tab_indentation_error(&line));
        }
        if sequence_rest(content).is_some() {
            return self.parse_sequence(line.indent(), depth);
        }
        if starts_mapping_entry(content) {
            return self.parse_mapping(line.indent(), depth);
        }
        self.pos += 1;
        if let Some(header) = parse_block_scalar_header(content, &line, 0)? {
            return self.parse_block_scalar(
                header,
                line.indent(),
                line.span(),
                depth + 1,
                line.indent() == 0,
            );
        }
        if is_plain_scalar_text(content) {
            return self.parse_plain_scalar(content, &line, 0, line.indent(), depth, true);
        }
        if line.indent() == 0 {
            self.parse_document_root_inline_value(content, &line, 0, line.indent(), depth)
        } else {
            self.parse_inline_value(content, &line, 0, line.indent(), depth)
        }
    }

    fn parse_sequence(&mut self, indent: usize, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        let start = self.current_content()?.span();
        self.emit_sequence_start(CollectionStyle::Block, start);
        let mut items = NodeItems::new();

        while let Some(line) = self.content_after_blanks()? {
            if line.indent() != indent {
                break;
            }
            let content = line.content(&source);
            if content.starts_with('\t') {
                return Err(tab_indentation_error(&line));
            }
            let Some((rest_start, rest)) = sequence_rest(content) else {
                break;
            };
            self.pos += 1;
            let rest_trim = rest.trim();
            let rest_ws = rest.len() - rest.trim_start().len();
            let value_start = rest_start + rest_ws;

            let item = if let Some((key_rest_start, key_rest)) = explicit_key_rest(rest_trim) {
                self.parse_compact_explicit_mapping_item(
                    &line,
                    value_start,
                    key_rest_start,
                    key_rest,
                    indent + value_start,
                    depth + 1,
                )?
            } else if rest_trim.is_empty() {
                self.parse_nested_or_null(
                    indent,
                    line.local_span(content.len(), content.len()),
                    depth + 1,
                )?
            } else if let Some(header) = parse_block_scalar_header(rest_trim, &line, value_start)? {
                self.parse_block_scalar(
                    header,
                    indent,
                    line.local_span(value_start, content.len()),
                    depth + 1,
                    false,
                )?
            } else if sequence_rest(rest_trim).is_some() {
                self.parse_inline_sequence_item(&line, value_start, rest_trim, depth + 1)?
            } else if find_mapping_col(rest_trim).is_some() {
                self.parse_inline_mapping_item(&line, value_start, rest_trim, depth + 1)?
            } else if is_plain_scalar_text(rest_trim) {
                self.parse_plain_scalar(rest_trim, &line, value_start, indent, depth + 1, false)?
            } else {
                self.parse_inline_value(rest_trim, &line, value_start, indent, depth + 1)?
            };
            items.push(item);
            self.check_collection_items(items.len(), line.span())?;
        }

        let end = items.last().map(|item| item.span.end).unwrap_or(start.end);
        let span = Span::new(start.start, end, start.line, start.column);
        self.emit_sequence_end(span);
        Ok(sequence_node(items.into_vec(), span))
    }

    fn parse_mapping(&mut self, indent: usize, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        let start = self.current_content()?.span();
        self.emit_mapping_start(CollectionStyle::Block, start);
        let mut entries = NodeEntries::new();
        let mut seen = DuplicateKeyTracker::new();
        let mut pending_key: Option<(Node, Span)> = None;

        while let Some(line) = self.content_after_blanks()? {
            let content = line.content(&source);
            if line.indent() != indent || sequence_rest(content).is_some() {
                break;
            }
            if content.starts_with('\t') {
                return Err(tab_indentation_error(&line));
            }
            if !starts_mapping_entry(content) {
                break;
            }
            let content = line.content(&source);
            self.pos += 1;

            let (key, value) = if let Some((rest_start, rest)) = explicit_key_rest(content) {
                if let Some((key, span)) = pending_key.take() {
                    self.emit_null_scalar(span);
                    let value = Node::null(span);
                    self.check_duplicate_key(&mut seen, &key)?;
                    entries.push((key, value));
                    self.check_collection_items(entries.len(), line.span())?;
                }
                let key =
                    self.parse_explicit_block_key(&line, rest_start, rest, indent, depth + 1)?;
                pending_key = Some((key, line.local_span(0, 1)));
                continue;
            } else if let Some((rest_start, rest)) = explicit_value_rest(content) {
                let key = pending_key.take().map(|(key, _)| key).unwrap_or_else(|| {
                    let span = line.local_span(0, 1);
                    self.emit_null_scalar(span);
                    Node::null(span)
                });
                let value =
                    self.parse_explicit_block_value(&line, rest_start, rest, indent, depth + 1)?;
                (key, value)
            } else {
                if let Some((key, span)) = pending_key.take() {
                    self.emit_null_scalar(span);
                    let value = Node::null(span);
                    self.check_duplicate_key(&mut seen, &key)?;
                    entries.push((key, value));
                    self.check_collection_items(entries.len(), line.span())?;
                }
                self.parse_mapping_pair(&line, 0, content, indent, depth + 1)?
            };
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
            self.check_collection_items(entries.len(), line.span())?;
        }
        if let Some((key, span)) = pending_key.take() {
            self.emit_null_scalar(span);
            let value = Node::null(span);
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
            self.check_collection_items(entries.len(), span)?;
        }

        let end = entries
            .last()
            .map(|(_, value)| value.span.end)
            .unwrap_or(start.end);
        let span = Span::new(start.start, end, start.line, start.column);
        self.emit_mapping_end(span);
        Ok(mapping_node(entries.into_vec(), span))
    }

    fn parse_inline_mapping_item(
        &mut self,
        line: &Line,
        rest_start: usize,
        rest: &str,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        let item_indent = line.indent() + rest_start;
        self.emit_mapping_start(
            CollectionStyle::Block,
            line.local_span(rest_start, rest_start + rest.len()),
        );
        let mut entries = NodeEntries::new();
        let mut seen = DuplicateKeyTracker::new();
        let (key, value) =
            self.parse_mapping_pair(line, rest_start, rest, item_indent, depth + 1)?;
        self.check_duplicate_key(&mut seen, &key)?;
        entries.push((key, value));
        self.check_collection_items(
            entries.len(),
            line.local_span(rest_start, rest_start + rest.len()),
        )?;

        while let Some(next) = self.content_after_blanks()? {
            if next.indent() != item_indent {
                break;
            }
            let content = next.content(&source);
            if sequence_rest(content).is_some() || find_mapping_col(content).is_none() {
                break;
            }
            let content = next.content(&source);
            self.pos += 1;
            let (key, value) =
                self.parse_mapping_pair(&next, 0, content, item_indent, depth + 1)?;
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
            self.check_collection_items(entries.len(), next.span())?;
        }

        let span = Span::new(
            line.start() + line.indent() + rest_start,
            entries
                .last()
                .map(|(_, value)| value.span.end)
                .unwrap_or(line.start() + line.indent() + rest_start + rest.len()),
            line.no(),
            line.indent() + rest_start + 1,
        );
        self.emit_mapping_end(span);
        Ok(mapping_node(entries.into_vec(), span))
    }

    fn parse_explicit_block_key(
        &mut self,
        line: &Line,
        rest_start: usize,
        rest: &str,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let rest_trim = rest.trim_start();
        let rest_ws = rest.len() - rest_trim.len();
        let value_start = rest_start + rest_ws;
        let rest_trim = rest_trim.trim_end();
        if rest_trim.is_empty() {
            self.parse_mapping_key_or_null(
                parent_indent,
                line.local_span(rest_start.saturating_sub(1), rest_start),
                depth + 1,
            )
        } else if let Some(alias) = parse_metadata_token(rest_trim, line, value_start, '*')? {
            if !alias.rest.trim().is_empty() {
                return Err(Error::new(
                    "unexpected content after alias",
                    line.local_span(alias.rest_start, alias.rest_start + alias.rest.len()),
                ));
            }
            self.emit_alias(alias.name.clone(), alias.span);
            if self.recording_events() {
                self.anchors.validate_alias(&alias.name, alias.span)?;
                return Ok(Node::null(alias.span));
            }
            self.anchors.resolve(&alias.name, alias.span, depth)
        } else if let Some(header) = parse_block_scalar_header(rest_trim, line, value_start)? {
            self.parse_block_scalar(
                header,
                parent_indent,
                line.local_span(value_start, line.content(&self.source).len()),
                depth + 1,
                false,
            )
        } else if sequence_rest(rest_trim).is_some() {
            self.parse_inline_sequence_item(line, value_start, rest_trim, depth + 1)
        } else if find_mapping_col(rest_trim).is_some() {
            self.parse_compact_mapping_node(line, value_start, rest_trim, parent_indent, depth + 1)
        } else {
            self.parse_plain_or_inline_scalar(
                rest_trim,
                line,
                value_start,
                parent_indent,
                depth + 1,
            )
        }
    }

    fn parse_explicit_block_value(
        &mut self,
        line: &Line,
        rest_start: usize,
        rest: &str,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let rest_trim = rest.trim_start();
        let rest_ws = rest.len() - rest_trim.len();
        let value_start = rest_start + rest_ws;
        let rest_trim = rest_trim.trim_end();
        if rest_trim.is_empty() {
            self.parse_mapping_value_or_null(
                parent_indent,
                line.local_span(rest_start.saturating_sub(1), rest_start),
                depth + 1,
            )
        } else if let Some(value) = self.parse_mapping_value_properties(
            rest_trim,
            line,
            value_start,
            parent_indent,
            depth + 1,
        )? {
            Ok(value)
        } else if let Some(header) = parse_block_scalar_header(rest_trim, line, value_start)? {
            self.parse_block_scalar(
                header,
                parent_indent,
                line.local_span(value_start, line.content(&self.source).len()),
                depth + 1,
                false,
            )
        } else if sequence_rest(rest_trim).is_some() {
            self.parse_inline_sequence_item(line, value_start, rest_trim, depth + 1)
        } else if find_mapping_col(rest_trim).is_some() {
            self.parse_compact_mapping_node(line, value_start, rest_trim, parent_indent, depth + 1)
        } else {
            self.parse_plain_or_inline_scalar(
                rest_trim,
                line,
                value_start,
                parent_indent,
                depth + 1,
            )
        }
    }

    fn parse_inline_sequence_item(
        &mut self,
        line: &Line,
        sequence_start: usize,
        sequence_text: &str,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        let sequence_indent = line.indent() + sequence_start;
        self.emit_sequence_start(
            CollectionStyle::Block,
            line.local_span(sequence_start, sequence_start + sequence_text.len()),
        );
        let mut items = NodeItems::new();
        let first = self.parse_sequence_value_from_line(
            line,
            sequence_start,
            sequence_text,
            sequence_indent,
            depth + 1,
        )?;
        items.push(first);
        self.check_collection_items(
            items.len(),
            line.local_span(sequence_start, sequence_start + sequence_text.len()),
        )?;

        while let Some(next) = self.content_after_blanks()? {
            if next.indent() != sequence_indent {
                break;
            }
            let content = next.content(&source);
            let Some((_, _)) = sequence_rest(content) else {
                break;
            };
            let content = next.content(&source);
            self.pos += 1;
            let item =
                self.parse_sequence_value_from_line(&next, 0, content, sequence_indent, depth + 1)?;
            items.push(item);
            self.check_collection_items(items.len(), next.span())?;
        }

        let end = items
            .last()
            .map(|item| item.span.end)
            .unwrap_or_else(|| line.start() + line.indent() + sequence_start + sequence_text.len());
        let span = Span::new(
            line.start() + line.indent() + sequence_start,
            end,
            line.no(),
            line.indent() + sequence_start + 1,
        );
        self.emit_sequence_end(span);
        Ok(sequence_node(items.into_vec(), span))
    }

    fn parse_sequence_value_from_line(
        &mut self,
        line: &Line,
        sequence_start: usize,
        sequence_text: &str,
        sequence_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        let Some((rest_start, rest)) = sequence_rest(sequence_text) else {
            return Err(Error::new(
                "expected sequence entry",
                line.local_span(sequence_start, sequence_start + sequence_text.len()),
            ));
        };
        let rest_trim = rest.trim();
        let rest_ws = rest.len() - rest.trim_start().len();
        let value_start = sequence_start + rest_start + rest_ws;
        if let Some((key_rest_start, key_rest)) = explicit_key_rest(rest_trim) {
            self.parse_compact_explicit_mapping_item(
                line,
                value_start,
                key_rest_start,
                key_rest,
                sequence_indent,
                depth + 1,
            )
        } else if rest_trim.is_empty() {
            self.parse_nested_or_null(
                sequence_indent,
                line.local_span(
                    sequence_start,
                    sequence_start + rest_start.min(sequence_text.len()),
                ),
                depth + 1,
            )
        } else if let Some(header) = parse_block_scalar_header(rest_trim, line, value_start)? {
            self.parse_block_scalar(
                header,
                sequence_indent,
                line.local_span(value_start, sequence_start + sequence_text.len()),
                depth + 1,
                false,
            )
        } else if sequence_rest(rest_trim).is_some() {
            self.parse_inline_sequence_item(line, value_start, rest_trim, depth + 1)
        } else if find_mapping_col(rest_trim).is_some() {
            self.parse_inline_mapping_item(line, value_start, rest_trim, depth + 1)
        } else if is_plain_scalar_text(rest_trim) {
            self.parse_plain_scalar(
                rest_trim,
                line,
                value_start,
                sequence_indent,
                depth + 1,
                false,
            )
        } else {
            self.parse_inline_value(rest_trim, line, value_start, sequence_indent, depth + 1)
        }
    }

    fn parse_compact_explicit_mapping_item(
        &mut self,
        line: &Line,
        marker_start: usize,
        key_rest_start: usize,
        key_rest: &str,
        item_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        self.emit_mapping_start(
            CollectionStyle::Block,
            line.local_span(marker_start, marker_start + key_rest.len()),
        );
        let key = self.parse_explicit_block_key(
            line,
            marker_start + key_rest_start,
            key_rest,
            item_indent,
            depth + 1,
        )?;
        let mut value = Node::null(line.local_span(marker_start, marker_start + 1));
        let mut emitted_value = false;
        let source = Rc::clone(&self.source);
        let explicit_value = if let Some(next) = self.content_after_blanks()? {
            let content = next.content(&source);
            if next.indent() == item_indent {
                explicit_value_rest(content).map(|(value_rest_start, value_rest)| {
                    (next, value_rest_start, value_rest.to_string())
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some((next, value_rest_start, value_rest)) = explicit_value {
            self.pos += 1;
            value = self.parse_explicit_block_value(
                &next,
                value_rest_start,
                &value_rest,
                item_indent,
                depth + 1,
            )?;
            emitted_value = true;
        }
        if !emitted_value {
            self.emit_null_scalar(value.span);
        }
        let span = Span::new(
            line.start() + line.indent() + marker_start,
            value.span.end,
            line.no(),
            line.indent() + marker_start + 1,
        );
        self.emit_mapping_end(span);
        self.record_merge_key(&key);
        Ok(mapping_node(vec![(key, value)], span))
    }

    fn parse_compact_mapping_node(
        &mut self,
        line: &Line,
        local_start: usize,
        text: &str,
        pair_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        self.emit_mapping_start(
            CollectionStyle::Block,
            line.local_span(local_start, local_start + text.len()),
        );
        let mut seen = DuplicateKeyTracker::new();
        let (key, value) =
            self.parse_mapping_pair(line, local_start, text, pair_indent, depth + 1)?;
        self.check_duplicate_key(&mut seen, &key)?;
        let span = Span::new(
            key.span.start,
            value.span.end,
            key.span.line,
            key.span.column,
        );
        self.emit_mapping_end(span);
        Ok(mapping_node(vec![(key, value)], span))
    }

    fn parse_plain_or_inline_scalar(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        if is_plain_scalar_text(text) {
            self.parse_plain_scalar(text, line, local_start, parent_indent, depth, false)
        } else {
            self.parse_inline_value(text, line, local_start, parent_indent, depth)
        }
    }

    fn parse_mapping_pair(
        &mut self,
        line: &Line,
        local_start: usize,
        text: &str,
        pair_indent: usize,
        depth: usize,
    ) -> Result<(Node, Node)> {
        self.check_depth(depth)?;
        let colon = find_mapping_col(text).ok_or_else(|| {
            Error::new(
                "expected ':' in mapping entry",
                line.local_span(local_start, local_start + text.len()),
            )
        })?;
        let raw_key = &text[..colon];
        let key_trim_start = raw_key.len() - raw_key.trim_start().len();
        let key_trimmed = raw_key.trim();
        let key = if key_trimmed.is_empty() {
            let span = line.local_span(local_start + colon, local_start + colon);
            self.emit_null_scalar(span);
            Node::empty_scalar(span)
        } else {
            self.parse_mapping_key(key_trimmed, line, local_start + key_trim_start, depth)?
        };
        let after_colon = &text[colon + 1..];
        let value_ws = after_colon.len() - after_colon.trim_start().len();
        let value_text = after_colon.trim_start();
        let value_start = local_start + colon + 1 + value_ws;
        if sequence_rest(value_text).is_some() {
            return Err(Error::new(
                "block sequence entries are not allowed in this context",
                line.local_span(value_start, value_start + 1),
            ));
        }
        let value = if value_text.is_empty() {
            self.parse_mapping_value_or_null(
                pair_indent,
                line.local_span(local_start + colon, local_start + colon + 1),
                depth + 1,
            )?
        } else if let Some(value) = self.parse_mapping_value_properties(
            value_text,
            line,
            value_start,
            pair_indent,
            depth + 1,
        )? {
            value
        } else if let Some(header) = parse_block_scalar_header(value_text, line, value_start)? {
            self.parse_block_scalar(
                header,
                pair_indent,
                line.local_span(value_start, local_start + text.len()),
                depth + 1,
                false,
            )?
        } else if is_plain_scalar_text(value_text) {
            if let Some(colon) = plain_scalar_mapping_value_colon(value_text) {
                return Err(Error::new(
                    "mapping values are not allowed in this context",
                    line.local_span(value_start + colon, value_start + colon + 1),
                ));
            }
            self.parse_plain_scalar(value_text, line, value_start, pair_indent, depth + 1, false)?
        } else {
            self.parse_inline_value(value_text, line, value_start, pair_indent, depth + 1)?
        };
        Ok((key, value))
    }

    fn parse_mapping_value_properties(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        pair_indent: usize,
        depth: usize,
    ) -> Result<Option<Node>> {
        let mut properties = NodePropertyState::default();
        self.parse_mapping_value_properties_with(
            text,
            line,
            local_start,
            pair_indent,
            depth,
            &mut properties,
        )
    }

    fn parse_mapping_value_properties_with(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        pair_indent: usize,
        depth: usize,
        properties: &mut NodePropertyState,
    ) -> Result<Option<Node>> {
        self.check_depth(depth)?;
        let leading_ws = text.len() - text.trim_start().len();
        let text = text.trim();
        let local_start = local_start + leading_ws;

        if let Some(anchor) = parse_metadata_token(text, line, local_start, '&')? {
            let anchor_after_tag = properties.tag.is_some();
            properties.record_anchor(anchor.span)?;
            reject_alias_with_node_properties(anchor.rest, line, anchor.rest_start)?;
            reject_same_line_block_sequence_after_property(anchor.rest, line, anchor.rest_start)?;
            self.push_anchor_meta(anchor.name.clone(), anchor.span);
            let generation = self.anchors.begin(anchor.name.clone(), anchor.span);
            let node = self.parse_mapping_value_property_rest(
                anchor.rest,
                line,
                anchor.rest_start,
                pair_indent,
                depth + 1,
                properties,
            )?;
            if anchor_after_tag {
                properties.defer_anchor_finish(anchor.name, generation);
                return Ok(Some(node));
            }
            self.anchors.finish(&anchor.name, generation, node.clone());
            return Ok(Some(node));
        }

        if let Some(tag) = parse_tag_token(text, line, local_start)? {
            let tag_value = self.resolve_tag(tag.tag, tag.span)?;
            properties.record_tag(tag.span)?;
            reject_alias_with_node_properties(tag.rest, line, tag.rest_start)?;
            reject_same_line_block_sequence_after_property(tag.rest, line, tag.rest_start)?;
            self.push_tag_meta(tag_value.clone(), tag.span);
            let node = self.parse_mapping_value_property_rest(
                tag.rest,
                line,
                tag.rest_start,
                pair_indent,
                depth + 1,
                properties,
            )?;
            let node = tagged_node(tag_value, tag.span, node);
            self.finish_deferred_anchor(properties, &node);
            return Ok(Some(node));
        }

        Ok(None)
    }

    fn parse_mapping_value_property_rest(
        &mut self,
        rest: &str,
        line: &Line,
        rest_start: usize,
        pair_indent: usize,
        depth: usize,
        properties: &mut NodePropertyState,
    ) -> Result<Node> {
        if rest.trim().is_empty() {
            let property_span = properties.latest.expect("node property was just parsed");
            return self.parse_mapping_value_or_null_after_properties(
                pair_indent,
                property_span,
                depth + 1,
                properties,
            );
        }
        if let Some(node) = self.parse_mapping_value_properties_with(
            rest,
            line,
            rest_start,
            pair_indent,
            depth + 1,
            properties,
        )? {
            return Ok(node);
        }
        self.parse_inline_value(rest, line, rest_start, pair_indent, depth + 1)
    }

    fn parse_mapping_key(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        depth: usize,
    ) -> Result<Node> {
        if let Some(alias) = parse_metadata_token(text, line, local_start, '*')? {
            if !alias.rest.trim().is_empty() {
                return Err(Error::new(
                    "unexpected content after alias",
                    line.local_span(alias.rest_start, alias.rest_start + alias.rest.len()),
                ));
            }
            self.emit_alias(alias.name.clone(), alias.span);
            if self.recording_events() {
                self.anchors.validate_alias(&alias.name, alias.span)?;
                return Ok(Node::null(alias.span));
            }
            return self.anchors.resolve(&alias.name, alias.span, depth);
        }

        if let Some(anchor) = parse_metadata_token(text, line, local_start, '&')? {
            if anchor.rest.trim().is_empty() {
                self.push_anchor_meta(anchor.name.clone(), anchor.span);
                let generation = self.anchors.begin(anchor.name.clone(), anchor.span);
                self.emit_null_scalar(anchor.span);
                let key = Node::null(anchor.span);
                self.anchors.finish(&anchor.name, generation, key.clone());
                return Ok(key);
            }
            reject_alias_with_node_properties(anchor.rest, line, anchor.rest_start)?;
            reject_same_line_block_sequence_after_property(anchor.rest, line, anchor.rest_start)?;
            self.push_anchor_meta(anchor.name.clone(), anchor.span);
            let generation = self.anchors.begin(anchor.name.clone(), anchor.span);
            let key = self.parse_key(anchor.rest, line, anchor.rest_start, depth)?;
            self.anchors.finish(&anchor.name, generation, key.clone());
            return Ok(key);
        }

        self.parse_key(text, line, local_start, depth)
    }

    fn parse_key(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        depth: usize,
    ) -> Result<Node> {
        self.parse_inline_value(text, line, local_start, line.indent(), depth)
    }

    fn parse_mapping_value_or_null(
        &mut self,
        parent_indent: usize,
        span: Span,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        match self.content_after_blanks()? {
            Some(next)
                if next.indent() == parent_indent
                    && sequence_rest(next.content(&source)).is_some() =>
            {
                self.parse_sequence(parent_indent, depth)
            }
            Some(next)
                if next.indent() > parent_indent
                    && self.empty_node_property_before_indentless_sequence(parent_indent)? =>
            {
                let line = next;
                let content = line.content(&source);
                self.pos += 1;
                self.parse_mapping_value_properties(content, &line, 0, parent_indent, depth + 1)?
                    .ok_or_else(|| {
                        Error::new("expected mapping value after node property", line.span())
                    })
            }
            _ => self.parse_nested_or_null(parent_indent, span, depth),
        }
    }

    fn parse_mapping_value_or_null_after_properties(
        &mut self,
        parent_indent: usize,
        span: Span,
        depth: usize,
        properties: &mut NodePropertyState,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        match self.content_after_blanks()? {
            Some(next)
                if next.indent() == parent_indent
                    && sequence_rest(next.content(&source)).is_some() =>
            {
                self.parse_sequence(parent_indent, depth)
            }
            Some(next)
                if next.indent() > parent_indent
                    && self.empty_node_property_before_indentless_sequence(parent_indent)? =>
            {
                let line = next;
                let content = line.content(&source);
                self.pos += 1;
                self.parse_mapping_value_properties_with(
                    content,
                    &line,
                    0,
                    parent_indent,
                    depth + 1,
                    properties,
                )?
                .ok_or_else(|| {
                    Error::new("expected mapping value after node property", line.span())
                })
            }
            Some(next)
                if next.indent() > parent_indent
                    && sequence_rest(next.content(&source)).is_none()
                    && !starts_mapping_entry(next.content(&source)) =>
            {
                let content = next.content(&source);
                self.pos += 1;
                if is_plain_scalar_text(content) {
                    self.parse_plain_scalar(content, &next, 0, parent_indent, depth, false)
                } else if let Some(node) = self.parse_mapping_value_properties_with(
                    content,
                    &next,
                    0,
                    parent_indent,
                    depth + 1,
                    properties,
                )? {
                    Ok(node)
                } else {
                    self.parse_inline_value(content, &next, 0, parent_indent, depth)
                }
            }
            Some(next) if next.indent() > parent_indent => self.parse_node(depth),
            _ => {
                self.emit_null_scalar(span);
                Ok(Node::empty_scalar(span))
            }
        }
    }

    fn parse_mapping_key_or_null(
        &mut self,
        parent_indent: usize,
        span: Span,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        match self.content_after_blanks()? {
            Some(next)
                if next.indent() == parent_indent
                    && sequence_rest(next.content(&source)).is_some() =>
            {
                self.parse_sequence(parent_indent, depth)
            }
            _ => self.parse_nested_or_null(parent_indent, span, depth),
        }
    }

    fn parse_nested_or_null(
        &mut self,
        parent_indent: usize,
        span: Span,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let source = Rc::clone(&self.source);
        match self.content_after_blanks()? {
            Some(next)
                if next.indent() > parent_indent
                    && sequence_rest(next.content(&source)).is_none()
                    && !starts_mapping_entry(next.content(&source)) =>
            {
                let content = next.content(&source);
                self.pos += 1;
                if is_plain_scalar_text(content) {
                    self.parse_plain_scalar(content, &next, 0, parent_indent, depth, false)
                } else {
                    self.parse_inline_value(content, &next, 0, parent_indent, depth)
                }
            }
            Some(next) if next.indent() > parent_indent => self.parse_node(depth),
            _ => {
                self.emit_null_scalar(span);
                Ok(Node::empty_scalar(span))
            }
        }
    }

    fn parse_plain_scalar(
        &mut self,
        first_text: &str,
        first_line: &Line,
        first_start: usize,
        parent_indent: usize,
        depth: usize,
        allow_same_indent_continuation: bool,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let first_trimmed = first_text.trim();
        let first_ws = first_text.len() - first_text.trim_start().len();
        let first_start = first_start + first_ws;
        let mut out = None::<String>;
        let mut end = first_line.start() + first_line.indent() + first_start + first_trimmed.len();
        let mut continued = false;
        let mut comment_terminated = first_line.had_comment;
        let source = Rc::clone(&self.source);

        loop {
            if comment_terminated {
                break;
            }
            let mut lookahead = self.pos;
            let mut blank_breaks = 0usize;
            loop {
                match self.lines.get(&self.source, lookahead)? {
                    Some(line) if line.kind == LineKind::Blank => {
                        if line.had_comment {
                            comment_terminated = true;
                            break;
                        }
                        lookahead += 1;
                        blank_breaks += 1;
                    }
                    _ => break,
                }
            }
            if comment_terminated {
                break;
            }
            let Some(next) = self.line_at(lookahead)? else {
                break;
            };
            if !matches!(next.kind, LineKind::Content | LineKind::Directive)
                || !is_plain_scalar_continuation_text(next.content(&source))
            {
                break;
            }
            let next_content = next.content(&source);
            if next.indent() < parent_indent
                || (next.indent() == parent_indent && !allow_same_indent_continuation)
                || (next.indent() <= parent_indent && sequence_rest(next_content).is_some())
                || starts_mapping_entry(next_content)
            {
                break;
            }
            self.pos = lookahead + 1;
            continued = true;
            comment_terminated = next.had_comment;
            let trimmed = next.content(&source).trim();
            if !trimmed.is_empty() {
                let out = out.get_or_insert_with(|| first_trimmed.to_string());
                if blank_breaks > 0 {
                    for _ in 0..blank_breaks {
                        out.push('\n');
                    }
                } else {
                    out.push(' ');
                }
                out.push_str(trimmed);
            }
            end = next.start() + next.raw_len();
        }

        if !continued {
            return self.parse_inline_value(
                first_trimmed,
                first_line,
                first_start,
                parent_indent,
                depth,
            );
        }

        let out = out.unwrap_or_else(|| first_trimmed.to_string());
        let node = Node::new(
            Value::String(out),
            Span::new(
                first_line.start() + first_line.indent() + first_start,
                end,
                first_line.no(),
                first_line.indent() + first_start + 1,
            ),
        );
        self.check_scalar_node(&node)?;
        self.emit_scalar_node(&node, ScalarStyle::Plain);
        Ok(node)
    }

    fn parse_block_scalar(
        &mut self,
        header: BlockScalarHeader,
        parent_indent: usize,
        marker_span: Span,
        depth: usize,
        allow_same_indent_content: bool,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let mut block_indent = header.indent.map(|indent| parent_indent + indent);
        let mut lines = Vec::new();
        let mut end = marker_span.end;
        let mut max_leading_blank_indent = 0usize;
        let source = Rc::clone(&self.source);

        while let Some(line) = self.line_at(self.pos)? {
            let raw = line.raw(&source);
            if matches!(line.kind, LineKind::DocumentStart | LineKind::DocumentEnd) {
                break;
            }
            if line.kind != LineKind::Blank
                && (line.indent() < parent_indent
                    || (line.indent() == parent_indent && !allow_same_indent_content))
            {
                break;
            }
            if line.kind == LineKind::Blank
                && !raw.trim().is_empty()
                && block_indent.is_some_and(|indent| line.indent() < indent)
            {
                break;
            }
            let raw = line.raw(&source);
            self.pos += 1;
            let text = if raw.trim().is_empty() {
                if let Some(tab_offset) = raw.bytes().position(|byte| byte == b'\t') {
                    if tab_offset == 0 {
                        return Err(Error::new(
                            "block scalar content cannot start with a tab",
                            Span::point(line.start() + tab_offset, line.no(), tab_offset + 1),
                        ));
                    }
                    let indent = *block_indent.get_or_insert(tab_offset);
                    if tab_offset < indent {
                        return Err(Error::new(
                            "block scalar content cannot start with a tab",
                            Span::point(line.start() + tab_offset, line.no(), tab_offset + 1),
                        ));
                    }
                    if max_leading_blank_indent > indent {
                        return Err(Error::new(
                            "block scalar content is less indented than a preceding blank line",
                            line.local_span(0, 0),
                        ));
                    }
                    line.raw_from(&source, indent).to_string()
                } else {
                    if let Some(indent) = block_indent {
                        line.raw_from(&source, indent).to_string()
                    } else {
                        max_leading_blank_indent = max_leading_blank_indent.max(line.raw_len());
                        String::new()
                    }
                }
            } else {
                let indent = *block_indent.get_or_insert(line.indent());
                if max_leading_blank_indent > indent {
                    return Err(Error::new(
                        "block scalar content is less indented than a preceding blank line",
                        line.local_span(0, 0),
                    ));
                }
                if line.indent() < indent {
                    break;
                }
                line.raw_from(&source, indent).to_string()
            };
            end = line.start() + line.raw_len();
            lines.push(text);
        }

        let mut out = String::new();
        if header.style == BlockScalarStyle::Literal {
            for line in &lines {
                out.push_str(line);
                out.push('\n');
            }
        } else {
            let mut idx = 0usize;
            while idx < lines.len() {
                if lines[idx].is_empty() {
                    out.push('\n');
                    idx += 1;
                    continue;
                }

                while idx < lines.len() && !lines[idx].is_empty() {
                    out.push_str(&lines[idx]);
                    if let Some(next) = lines.get(idx + 1)
                        && !next.is_empty()
                    {
                        if block_scalar_line_is_more_indented(&lines[idx])
                            || block_scalar_line_is_more_indented(next)
                        {
                            out.push('\n');
                        } else {
                            out.push(' ');
                        }
                    }
                    idx += 1;
                }

                let blank_start = idx;
                while idx < lines.len() && lines[idx].is_empty() {
                    idx += 1;
                }
                let blank_count = idx - blank_start;
                if blank_count == 0 {
                    continue;
                }

                let previous_more_indented = lines
                    .get(blank_start.wrapping_sub(1))
                    .is_some_and(|line| block_scalar_line_is_more_indented(line));
                let next_more_indented = lines
                    .get(idx)
                    .is_some_and(|line| block_scalar_line_is_more_indented(line));
                let line_breaks = if previous_more_indented || next_more_indented {
                    blank_count + 1
                } else {
                    blank_count
                };
                for _ in 0..line_breaks {
                    out.push('\n');
                }
            }
            if lines.last().is_some_and(|line| !line.is_empty()) {
                out.push('\n');
            }
        }
        let has_content_line = lines.iter().any(|line| !line.is_empty());
        match header.chomping {
            BlockScalarChomping::Strip => {
                while out.ends_with('\n') {
                    out.pop();
                }
            }
            BlockScalarChomping::Clip => {
                if has_content_line {
                    while out.ends_with("\n\n") {
                        out.pop();
                    }
                } else {
                    out.clear();
                }
            }
            BlockScalarChomping::Keep => {}
        }
        let style = match header.style {
            BlockScalarStyle::Literal => ScalarStyle::Literal,
            BlockScalarStyle::Folded => ScalarStyle::Folded,
        };
        let node = Node::new(
            Value::String(out),
            Span::new(marker_span.start, end, marker_span.line, marker_span.column),
        );
        self.check_scalar_node(&node)?;
        self.emit_scalar_node(&node, style);
        Ok(node)
    }

    fn parse_inline_value(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        let mut properties = NodePropertyState::default();
        self.parse_inline_value_with_properties(
            text,
            line,
            local_start,
            parent_indent,
            depth,
            &mut properties,
        )
    }

    fn parse_document_root_inline_value(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        parent_indent: usize,
        depth: usize,
    ) -> Result<Node> {
        let mut properties = NodePropertyState {
            allow_document_root_continuation: true,
            ..NodePropertyState::default()
        };
        self.parse_inline_value_with_properties(
            text,
            line,
            local_start,
            parent_indent,
            depth,
            &mut properties,
        )
    }

    fn parse_inline_value_with_properties(
        &mut self,
        text: &str,
        line: &Line,
        local_start: usize,
        parent_indent: usize,
        depth: usize,
        properties: &mut NodePropertyState,
    ) -> Result<Node> {
        self.options.check_nesting_depth(
            depth,
            line.local_span(local_start, local_start + text.len()),
        )?;
        let leading_ws = text.len() - text.trim_start().len();
        let text = text.trim();
        let local_start = local_start + leading_ws;
        if let Some(alias) = parse_metadata_token(text, line, local_start, '*')? {
            if !alias.rest.trim().is_empty() {
                return Err(Error::new(
                    "unexpected content after alias",
                    line.local_span(alias.rest_start, alias.rest_start + alias.rest.len()),
                ));
            }
            self.emit_alias(alias.name.clone(), alias.span);
            if self.recording_events() {
                self.anchors.validate_alias(&alias.name, alias.span)?;
                return Ok(Node::null(alias.span));
            }
            return self.anchors.resolve(&alias.name, alias.span, depth);
        }
        if let Some(anchor) = parse_metadata_token(text, line, local_start, '&')? {
            let anchor_after_tag = properties.tag.is_some();
            properties.record_anchor(anchor.span)?;
            reject_alias_with_node_properties(anchor.rest, line, anchor.rest_start)?;
            reject_same_line_block_sequence_after_property(anchor.rest, line, anchor.rest_start)?;
            self.push_anchor_meta(anchor.name.clone(), anchor.span);
            let generation = self.anchors.begin(anchor.name.clone(), anchor.span);
            let node = if anchor.rest.trim().is_empty() {
                if properties.allow_document_root_continuation && line.indent() == 0 {
                    self.parse_document_root_or_null(anchor.span, depth + 1)?
                } else {
                    self.parse_nested_or_null(parent_indent, anchor.span, depth + 1)?
                }
            } else {
                self.parse_inline_value_with_properties(
                    anchor.rest,
                    line,
                    anchor.rest_start,
                    parent_indent,
                    depth + 1,
                    properties,
                )?
            };
            if anchor_after_tag {
                properties.defer_anchor_finish(anchor.name, generation);
                return Ok(node);
            }
            self.anchors.finish(&anchor.name, generation, node.clone());
            return Ok(node);
        }
        if let Some(tag) = parse_tag_token(text, line, local_start)? {
            let tag_value = self.resolve_tag(tag.tag, tag.span)?;
            properties.record_tag(tag.span)?;
            reject_alias_with_node_properties(tag.rest, line, tag.rest_start)?;
            reject_same_line_block_sequence_after_property(tag.rest, line, tag.rest_start)?;
            self.push_tag_meta(tag_value.clone(), tag.span);
            let node = if tag.rest.trim().is_empty() {
                if properties.allow_document_root_continuation && line.indent() == 0 {
                    self.parse_document_root_or_null(tag.span, depth + 1)?
                } else {
                    self.parse_nested_or_null(parent_indent, tag.span, depth + 1)?
                }
            } else {
                self.parse_inline_value_with_properties(
                    tag.rest,
                    line,
                    tag.rest_start,
                    parent_indent,
                    depth + 1,
                    properties,
                )?
            };
            let node = tagged_node(tag_value, tag.span, node);
            self.finish_deferred_anchor(properties, &node);
            return Ok(node);
        }
        if let Some(header) = parse_block_scalar_header(text, line, local_start)? {
            return self.parse_block_scalar(
                header,
                parent_indent,
                line.local_span(local_start, local_start + text.len()),
                depth + 1,
                false,
            );
        }
        if let Some(quote @ ('"' | '\'')) = text.chars().next() {
            if let Some(trailing_start) = quoted_scalar_trailing_start(text, quote) {
                return Err(Error::new(
                    "unexpected trailing characters after quoted scalar",
                    line.local_span(local_start + trailing_start, local_start + text.len()),
                ));
            }
            if !quoted_scalar_is_closed(text, quote) {
                return self.parse_multiline_quoted_scalar(
                    text,
                    line,
                    local_start,
                    parent_indent,
                    quote,
                    false,
                );
            }
        }
        if text.starts_with('[') || text.starts_with('{') {
            let buffer = self.collect_flow_buffer(text, line, local_start, parent_indent)?;
            let (node, has_merge_key) = FlowParser::new(
                buffer,
                depth,
                &mut self.anchors,
                self.events.as_mut(),
                &self.active_tag_handles,
                self.active_schema,
                self.options,
            )
            .parse()?;
            self.current_document_has_merge_key |= has_merge_key;
            return Ok(node);
        }
        let node = parse_scalar(
            text,
            line,
            local_start,
            &self.source,
            self.scalar_storage,
            self.active_schema,
        )?;
        self.check_scalar_node(&node)?;
        self.emit_scalar_node(&node, scalar_style_for_text(text));
        Ok(node)
    }

    fn parse_multiline_quoted_scalar(
        &mut self,
        first_text: &str,
        first_line: &Line,
        first_start: usize,
        parent_indent: usize,
        quote: char,
        allow_parent_indent_continuation: bool,
    ) -> Result<Node> {
        let source = Rc::clone(&self.source);
        let mut text = quoted_line_text(&source, first_line, first_start, first_text, quote);
        let mut end = first_line.start() + first_line.indent() + first_start + first_text.len();
        let require_indented_continuation =
            !allow_parent_indent_continuation && first_line.indent() + first_start > parent_indent;

        while quoted_scalar_accepted_end(&text, quote).is_none() {
            let Some(line) = self.line_at(self.pos)? else {
                break;
            };
            match line.kind {
                LineKind::Blank => {
                    self.pos += 1;
                    text.push('\n');
                    end = line.start() + line.raw_len();
                }
                LineKind::Content if line.indent() >= parent_indent => {
                    if require_indented_continuation && line.indent() <= parent_indent {
                        if line.content(&source).starts_with('\t') {
                            return Err(tab_indentation_error(&line));
                        }
                        return Err(Error::new(
                            "multiline quoted scalar continuation is not sufficiently indented",
                            line.span(),
                        ));
                    }
                    self.pos += 1;
                    let line_text_start = text.len() + 1;
                    text.push('\n');
                    let content = line.content(&source);
                    let line_text = quoted_line_text(&source, &line, 0, content, quote);
                    text.push_str(&line_text);
                    end = quoted_scalar_accepted_end(&text, quote)
                        .filter(|close_end| *close_end >= line_text_start)
                        .map(|close_end| {
                            line.start() + line.content_start() + close_end - line_text_start
                        })
                        .unwrap_or_else(|| line.start() + line.content_start() + line_text.len());
                }
                LineKind::Content
                | LineKind::Directive
                | LineKind::DocumentStart
                | LineKind::DocumentEnd => break,
            }
        }

        let span = Span::new(
            first_line.start() + first_line.indent() + first_start,
            end,
            first_line.no(),
            first_line.indent() + first_start + 1,
        );
        let text_end = quoted_scalar_accepted_end(&text, quote).unwrap_or(text.len());
        let text = fold_flow_quoted_scalar(&text[..text_end], quote);
        let node = parse_scalar_with_span(&text, span)?;
        let style = if quote == '"' {
            ScalarStyle::DoubleQuoted
        } else {
            ScalarStyle::SingleQuoted
        };
        self.check_scalar_node(&node)?;
        self.emit_scalar_node(&node, style);
        Ok(node)
    }

    fn collect_flow_buffer(
        &mut self,
        first_text: &str,
        first_line: &Line,
        first_start: usize,
        parent_indent: usize,
    ) -> Result<FlowBuffer> {
        let mut buffer = FlowBuffer::single(first_text, first_line, first_start);
        let source = Rc::clone(&self.source);
        let require_continuation_indent =
            (first_line.indent() + first_start > parent_indent).then_some(parent_indent);
        while !flow_collection_is_closed(&buffer.text) {
            let Some(line) = self.line_at(self.pos)? else {
                break;
            };
            match line.kind {
                LineKind::Blank => {
                    let mark = SourceMark::new(
                        line.start() + line.raw_len(),
                        line.no(),
                        line.raw_len() + 1,
                    );
                    buffer.push_virtual_separator(mark);
                    self.pos += 1;
                }
                LineKind::Content | LineKind::Directive => {
                    if require_continuation_indent.is_some_and(|indent| {
                        line.indent() <= indent
                            && !flow_continuation_may_start_at_parent_indent_after_ws(
                                line.content(&source),
                            )
                    }) {
                        return Err(Error::new(
                            "flow collection continuation is not sufficiently indented",
                            line.span(),
                        ));
                    }
                    buffer.push_virtual_separator(SourceMark::for_line_content(&line, 0));
                    let content = line.content(&source);
                    buffer.push_source_text(content, &line, 0);
                    self.pos += 1;
                }
                LineKind::DocumentStart | LineKind::DocumentEnd => break,
            }
        }
        Ok(buffer)
    }

    fn check_depth(&mut self, depth: usize) -> Result<()> {
        if self
            .options
            .selected_max_nesting_depth()
            .is_none_or(|max| depth <= max)
        {
            return Ok(());
        }
        let span = self
            .peek_content()?
            .map(|line| line.span())
            .unwrap_or_else(|| Span::point(self.input_len, 1, self.input_len + 1));
        Err(Error::limit("maximum YAML nesting depth exceeded", span))
    }

    fn check_scalar_bytes(&self, len: usize, span: Span) -> Result<()> {
        self.options.check_scalar_bytes(len, span)
    }

    fn check_scalar_node(&self, node: &Node) -> Result<()> {
        self.check_scalar_bytes(event_scalar_value_len(node), node.span)
    }

    fn check_collection_items(&self, len: usize, span: Span) -> Result<()> {
        self.options.check_collection_items(len, span)
    }

    #[inline]
    fn current_content(&mut self) -> Result<Line> {
        self.content_after_blanks()?.ok_or_else(|| {
            Error::new(
                "expected YAML content",
                Span::point(self.input_len, 1, self.input_len + 1),
            )
        })
    }

    #[inline]
    fn peek_content(&mut self) -> Result<Option<Line>> {
        Ok(match self.line_at(self.pos)? {
            Some(line) if line.kind == LineKind::Content => Some(line),
            _ => None,
        })
    }

    fn peek_content_from(&mut self, mut pos: usize) -> Result<Option<Line>> {
        loop {
            match self.line_at(pos)? {
                Some(line) if line.kind == LineKind::Blank => pos += 1,
                Some(line) if line.kind == LineKind::Content => return Ok(Some(line)),
                _ => return Ok(None),
            }
        }
    }

    #[inline]
    fn line_after_blanks(&mut self) -> Result<Option<Line>> {
        loop {
            match self.lines.get(&self.source, self.pos)? {
                Some(line) if line.kind == LineKind::Blank => self.pos += 1,
                Some(line) => return Ok(Some(*line)),
                None => return Ok(None),
            }
        }
    }

    #[inline]
    fn content_after_blanks(&mut self) -> Result<Option<Line>> {
        Ok(match self.line_after_blanks()? {
            Some(line) if line.kind == LineKind::Content => Some(line),
            _ => None,
        })
    }

    fn empty_node_property_before_indentless_sequence(
        &mut self,
        parent_indent: usize,
    ) -> Result<bool> {
        let Some(line) = self.peek_content()? else {
            return Ok(false);
        };
        if !empty_node_property_line(&line, &self.source)? {
            return Ok(false);
        }
        Ok(matches!(
            self.peek_content_from(self.pos + 1)?,
            Some(next)
                if next.indent() == parent_indent
                    && sequence_rest(next.content(&self.source)).is_some()
        ))
    }

    #[inline]
    fn skip_blanks(&mut self) -> Result<()> {
        while matches!(self.line_kind_at(self.pos)?, Some(LineKind::Blank)) {
            self.pos += 1;
        }
        Ok(())
    }
}

fn block_scalar_line_is_more_indented(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

fn preprocess_line(source: &str, no: usize, start: usize, raw_len: usize) -> Result<Line> {
    let no = compact_offset(no)?;
    let raw = &source[start..start + raw_len];
    let bom_len = if start == 0 && raw.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let raw_body = &raw[bom_len..];
    let scan = scan_line(raw_body);
    if scan.blank {
        return Ok(Line {
            raw: LineText::new(start, start + raw.len())?,
            no,
            indent: compact_offset(scan.indent)?,
            content_start: compact_offset(bom_len + scan.indent)?,
            content_end: compact_offset(bom_len + scan.indent)?,
            kind: LineKind::Blank,
            had_comment: scan.had_comment,
        });
    }
    let content_start = bom_len + scan.indent;
    let content_end = bom_len + scan.content_end;
    let content = &raw[content_start..content_end];
    if let Some(tab_offset) =
        block_indicator_tab_separation_offset(&raw_body.as_bytes()[scan.indent..scan.content_end])
    {
        return Err(Error::new(
            "tabs are not allowed as separation after block indicators",
            Span::point(
                start + content_start + tab_offset,
                no as usize,
                content_start + tab_offset + 1,
            ),
        ));
    }
    let kind = match content {
        "---" => LineKind::DocumentStart,
        "..." => LineKind::DocumentEnd,
        _ if document_start_rest(content).is_some() => LineKind::DocumentStart,
        _ if content.starts_with("... ") => {
            return Err(Error::new(
                "document end markers cannot have trailing content",
                Span::new(
                    start + content_start,
                    start + content_start + content.len(),
                    no as usize,
                    content_start + 1,
                ),
            ));
        }
        _ if content.starts_with('%') => LineKind::Directive,
        _ => LineKind::Content,
    };
    Ok(Line {
        raw: LineText::new(start, start + raw.len())?,
        no,
        indent: compact_offset(scan.indent)?,
        content_start: compact_offset(content_start)?,
        content_end: compact_offset(content_end)?,
        kind,
        had_comment: scan.had_comment,
    })
}

struct LineScan {
    indent: usize,
    content_end: usize,
    blank: bool,
    had_comment: bool,
}

fn scan_line(raw_body: &str) -> LineScan {
    let mut indent = 0usize;
    let mut in_indent = true;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut previous_whitespace = true;
    let mut content_end = None;

    for (idx, ch) in raw_body.char_indices() {
        if in_indent {
            if ch == ' ' {
                indent += 1;
            } else {
                in_indent = false;
            }
        }

        if double && escaped {
            escaped = false;
            if !ch.is_whitespace() {
                content_end = Some(idx + ch.len_utf8());
            }
            previous_whitespace = ch.is_whitespace();
            continue;
        }

        match ch {
            '\\' if double => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            '#' if !single && !double && previous_whitespace => {
                return LineScan {
                    indent,
                    content_end: content_end.unwrap_or(indent),
                    blank: content_end.is_none(),
                    had_comment: true,
                };
            }
            _ => {}
        }

        if !ch.is_whitespace() {
            content_end = Some(idx + ch.len_utf8());
        }
        previous_whitespace = ch.is_whitespace();
    }

    LineScan {
        indent,
        content_end: content_end.unwrap_or(indent),
        blank: content_end.is_none(),
        had_comment: false,
    }
}

pub(crate) fn comment_start(raw: &str) -> Option<usize> {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut previous_whitespace = true;
    for (idx, ch) in raw.char_indices() {
        if double && escaped {
            escaped = false;
            previous_whitespace = ch.is_whitespace();
            continue;
        }
        match ch {
            '\\' if double => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            '#' if !single && !double && previous_whitespace => {
                return Some(idx);
            }
            _ => {}
        }
        previous_whitespace = ch.is_whitespace();
    }
    None
}

fn document_start_rest(content: &str) -> Option<(usize, &str)> {
    let rest = content.strip_prefix("---")?;
    if rest.is_empty() {
        return None;
    }
    match rest.as_bytes().first() {
        Some(b' ' | b'\t') => Some((3, rest)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct DirectiveField<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn directive_fields<'a>(line: &Line, source: &'a str) -> Vec<DirectiveField<'a>> {
    let mut fields = Vec::new();
    let mut start = None;
    let content = line.content(source);
    for (idx, ch) in content.char_indices() {
        if ch == '#' && start.is_none() {
            break;
        } else if ch.is_whitespace() {
            if let Some(field_start) = start.take() {
                fields.push(DirectiveField {
                    text: &content[field_start..idx],
                    start: field_start,
                    end: idx,
                });
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(field_start) = start {
        fields.push(DirectiveField {
            text: &content[field_start..],
            start: field_start,
            end: content.len(),
        });
    }
    fields
}

fn parse_yaml_version(text: &str) -> Option<(u8, u8)> {
    let (major, minor) = text.split_once('.')?;
    if major.is_empty() || minor.is_empty() || minor.contains('.') {
        return None;
    }
    let major = major.parse().ok()?;
    if major == 0 {
        return None;
    }
    Some((major, minor.parse().ok()?))
}

fn default_construction_schema(schema: Schema) -> Schema {
    match schema {
        Schema::YamlVersionDirective => Schema::Yaml12,
        schema => schema,
    }
}

fn schema_for_directives(schema: Schema, directives: &EventDocumentDirectives) -> Schema {
    match schema {
        Schema::YamlVersionDirective
            if directives
                .yaml_version
                .as_ref()
                .is_some_and(|version| version.major == 1 && version.minor == 1) =>
        {
            Schema::LegacySerdeYaml
        }
        Schema::YamlVersionDirective => Schema::Yaml12,
        schema => schema,
    }
}

fn merge_policy_for_schema(schema: Schema) -> MergePolicy {
    if schema.is_legacy_compatible() {
        MergePolicy::Yaml11Compatible
    } else {
        MergePolicy::Strict
    }
}

fn check_duplicate_for_schema(
    recording_events: bool,
    schema: Schema,
    options: LoadOptions,
    seen: &mut DuplicateKeyTracker,
    key: &Node,
) -> Result<()> {
    if recording_events || (schema.is_legacy_compatible() && node_is_merge_key(key)) {
        return Ok(());
    }
    check_duplicate_with_tracker_at_depth_limit(seen, key, 1, options.selected_max_nesting_depth())
}

fn node_is_merge_key(key: &Node) -> bool {
    match &key.value {
        Value::String(_) => key.as_str() == Some("<<"),
        Value::Tagged(tagged) if tagged.tag.is_yaml_core("merge") => {
            tagged.value.as_str() == Some("<<")
        }
        _ => false,
    }
}

fn valid_tag_handle(handle: &str) -> bool {
    handle == "!" || handle == "!!" || (handle.starts_with('!') && handle.ends_with('!'))
}

fn is_named_tag_handle(handle: &str) -> bool {
    handle.starts_with('!') && handle.ends_with('!') && handle != "!" && handle != "!!"
}

fn resolve_tag(active_tag_handles: &HashMap<String, String>, tag: Tag, span: Span) -> Result<Tag> {
    let suffix = decode_tag_uri_escapes(&tag.suffix);
    if suffix.is_empty() || suffix.starts_with("tag:") {
        return Ok(Tag { suffix, ..tag });
    }
    let Some(prefix) = active_tag_handles.get(&tag.handle) else {
        if is_named_tag_handle(&tag.handle) {
            return Err(Error::new("undeclared TAG directive handle", span));
        }
        return Ok(Tag { suffix, ..tag });
    };
    Ok(Tag {
        handle: "!".to_string(),
        suffix: format!("{prefix}{suffix}"),
    })
}

fn decode_tag_uri_escapes(suffix: &str) -> String {
    if !suffix.as_bytes().contains(&b'%') {
        return suffix.to_string();
    }

    let mut decoded = Vec::with_capacity(suffix.len());
    let bytes = suffix.as_bytes();
    let mut idx = 0;
    let mut changed = false;
    while idx < bytes.len() {
        if bytes[idx] == b'%'
            && idx + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_value(bytes[idx + 1]), hex_value(bytes[idx + 2]))
        {
            decoded.push((high << 4) | low);
            idx += 3;
            changed = true;
            continue;
        }
        decoded.push(bytes[idx]);
        idx += 1;
    }

    if changed {
        String::from_utf8(decoded).unwrap_or_else(|_| suffix.to_string())
    } else {
        suffix.to_string()
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn sequence_rest(content: &str) -> Option<(usize, &str)> {
    if content == "-" {
        Some((1, ""))
    } else if let Some(rest) = content.strip_prefix("- ") {
        Some((2, rest))
    } else if let Some(rest) = content.strip_prefix("-\t") {
        Some((2, rest))
    } else {
        None
    }
}

fn explicit_key_rest(content: &str) -> Option<(usize, &str)> {
    if content == "?" {
        Some((1, ""))
    } else if let Some(rest) = content.strip_prefix("? ") {
        Some((2, rest))
    } else {
        None
    }
}

fn explicit_value_rest(content: &str) -> Option<(usize, &str)> {
    if content == ":" {
        Some((1, ""))
    } else if let Some(rest) = content.strip_prefix(": ") {
        Some((2, rest))
    } else {
        None
    }
}

fn block_indicator_tab_separation_offset(content: &[u8]) -> Option<usize> {
    match content.first() {
        Some(b'-') => sequence_indicator_nested_tab_offset(content),
        Some(b'?' | b':') => indicator_tab_offset(content),
        _ => None,
    }
}

fn sequence_indicator_nested_tab_offset(content: &[u8]) -> Option<usize> {
    let first_tab = indicator_tab_offset(content)?;
    let nested_offset = content[1..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|idx| 1 + idx)?;
    nested_indicator_is_token(content, nested_offset).then_some(first_tab)
}

fn nested_indicator_is_token(content: &[u8], offset: usize) -> bool {
    match content[offset] {
        b'-' => content
            .get(offset + 1)
            .is_none_or(|byte| byte.is_ascii_whitespace()),
        b'?' | b':' => true,
        _ => false,
    }
}

fn indicator_tab_offset(content: &[u8]) -> Option<usize> {
    match content.get(1) {
        Some(b'\t') => Some(1),
        Some(b' ') => content[1..]
            .iter()
            .position(|byte| *byte != b' ')
            .and_then(|idx| (content[1 + idx] == b'\t').then_some(1 + idx)),
        _ => None,
    }
}

fn root_tab_separated_flow_collection(content: &str) -> bool {
    matches!(
        content.trim_start_matches('\t').chars().next(),
        Some('[' | '{')
    )
}

fn starts_mapping_entry(content: &str) -> bool {
    find_mapping_col(content).is_some()
        || explicit_key_rest(content).is_some()
        || explicit_value_rest(content).is_some()
}

fn find_mapping_col(text: &str) -> Option<usize> {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut flow_depth = 0usize;
    let mut pos = 0usize;
    while let Some(ch) = text[pos..].chars().next() {
        let idx = pos;
        if double && escaped {
            escaped = false;
            pos += ch.len_utf8();
            continue;
        }
        if !single
            && !double
            && flow_depth == 0
            && matches!(ch, '&' | '*')
            && block_metadata_token_starts_at(text, idx)
        {
            pos = skip_block_metadata_token(text, idx, ch);
            continue;
        }
        match ch {
            '\\' if double => escaped = true,
            '"' if !single && (double || can_start_quoted_context(text, idx)) => double = !double,
            '\'' if !double && (single || can_start_quoted_context(text, idx)) => single = !single,
            '[' | '{' if !single && !double => flow_depth += 1,
            ']' | '}' if !single && !double => flow_depth = flow_depth.saturating_sub(1),
            ':' if !single && !double && flow_depth == 0 => {
                let after = text[idx + ch.len_utf8()..].chars().next();
                if after.is_none_or(char::is_whitespace) {
                    return Some(idx);
                }
            }
            _ => {}
        }
        pos += ch.len_utf8();
    }
    None
}

fn block_metadata_token_starts_at(text: &str, idx: usize) -> bool {
    text[..idx].trim().is_empty()
}

fn skip_block_metadata_token(text: &str, idx: usize, sigil: char) -> usize {
    let mut pos = idx + sigil.len_utf8();
    while let Some(ch) = text[pos..].chars().next() {
        if ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}') {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn can_start_quoted_context(text: &str, idx: usize) -> bool {
    let prefix = text[..idx].trim_end();
    if prefix.is_empty() {
        return true;
    }

    if prefix
        .chars()
        .next_back()
        .is_some_and(|ch| matches!(ch, '[' | '{' | ',' | ':' | '?' | '-'))
    {
        return true;
    }

    if prefix.ends_with('>')
        && let Some(tag_start) = prefix.rfind("!<")
    {
        let valid_start = prefix[..tag_start]
            .chars()
            .next_back()
            .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '[' | '{' | ',' | ':' | '?' | '-'));
        if valid_start {
            return true;
        }
    }

    let token_start = prefix
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| {
            (ch.is_whitespace() || matches!(ch, '[' | '{')).then_some(idx + ch.len_utf8())
        })
        .unwrap_or(0);
    matches!(prefix[token_start..].chars().next(), Some('!' | '&'))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockScalarHeader {
    style: BlockScalarStyle,
    indent: Option<usize>,
    chomping: BlockScalarChomping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockScalarStyle {
    Literal,
    Folded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockScalarChomping {
    Clip,
    Strip,
    Keep,
}

fn parse_block_scalar_header(
    text: &str,
    line: &Line,
    local_start: usize,
) -> Result<Option<BlockScalarHeader>> {
    let mut chars = text.char_indices();
    let style = match chars.next().map(|(_, ch)| ch) {
        None => return Ok(None),
        Some('|') => BlockScalarStyle::Literal,
        Some('>') => BlockScalarStyle::Folded,
        _ => return Ok(None),
    };
    let mut indent = None;
    let mut chomping = BlockScalarChomping::Clip;

    for (offset, ch) in chars {
        match ch {
            '1'..='9' if indent.is_none() => {
                indent = Some(ch.to_digit(10).expect("matched digit") as usize);
            }
            '-' if chomping == BlockScalarChomping::Clip => {
                chomping = BlockScalarChomping::Strip;
            }
            '+' if chomping == BlockScalarChomping::Clip => {
                chomping = BlockScalarChomping::Keep;
            }
            _ => {
                return Err(Error::new(
                    "invalid block scalar header",
                    line.local_span(local_start + offset, local_start + offset + ch.len_utf8()),
                ));
            }
        }
    }

    Ok(Some(BlockScalarHeader {
        style,
        indent,
        chomping,
    }))
}

fn is_plain_scalar_text(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && !text.starts_with('"')
        && !text.starts_with('\'')
        && !text.starts_with('[')
        && !text.starts_with('{')
        && !text.starts_with('&')
        && !text.starts_with('*')
        && !text.starts_with('!')
        && !text.starts_with('|')
        && !text.starts_with('>')
}

fn plain_scalar_mapping_value_colon(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(idx, ch)| {
        if ch != ':' {
            return None;
        }
        let after = text[idx + ch.len_utf8()..].chars().next();
        after.is_none_or(char::is_whitespace).then_some(idx)
    })
}

fn is_plain_scalar_continuation_text(text: &str) -> bool {
    !text.trim().is_empty()
}

fn parse_scalar(
    text: &str,
    line: &Line,
    local_start: usize,
    source: &Rc<str>,
    scalar_storage: ScalarStorage,
    schema: Schema,
) -> Result<Node> {
    let span = line.local_span(local_start, local_start + text.len());
    if scalar_storage == ScalarStorage::SourceBacked {
        parse_source_scalar_with_schema(text, span, schema, source)
    } else {
        parse_scalar_with_schema(text, span, schema)
    }
}

fn scalar_style_for_text(text: &str) -> ScalarStyle {
    match text.trim_start().chars().next() {
        Some('"') => ScalarStyle::DoubleQuoted,
        Some('\'') => ScalarStyle::SingleQuoted,
        _ => ScalarStyle::Plain,
    }
}

fn parse_metadata_token<'a>(
    text: &'a str,
    line: &Line,
    local_start: usize,
    sigil: char,
) -> Result<Option<MetadataToken<'a>>> {
    let Some(rest) = text.strip_prefix(sigil) else {
        return Ok(None);
    };
    let name_len = metadata_name_len(rest);
    if name_len == 0 {
        let kind = if sigil == '&' { "anchor" } else { "alias" };
        return Err(Error::new(
            format!("{kind} name cannot be empty"),
            line.local_span(local_start, local_start + sigil.len_utf8()),
        ));
    }
    let after_name = &rest[name_len..];
    if !after_name.is_empty() && !after_name.starts_with(char::is_whitespace) {
        let kind = if sigil == '&' { "anchor" } else { "alias" };
        return Err(Error::new(
            format!("{kind} name must be separated from the node value by whitespace"),
            line.local_span(local_start, local_start + sigil.len_utf8() + name_len),
        ));
    }
    let rest_ws = after_name.len() - after_name.trim_start().len();
    Ok(Some(MetadataToken {
        name: rest[..name_len].to_string(),
        span: line.local_span(local_start, local_start + sigil.len_utf8() + name_len),
        rest: after_name.trim_start(),
        rest_start: local_start + sigil.len_utf8() + name_len + rest_ws,
    }))
}

fn parse_tag_token<'a>(
    text: &'a str,
    line: &Line,
    local_start: usize,
) -> Result<Option<TagToken<'a>>> {
    let Some(rest) = text.strip_prefix('!') else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(Some(TagToken {
            tag: Tag::new("!"),
            span: line.local_span(local_start, local_start + 1),
            rest: "",
            rest_start: local_start + 1,
        }));
    }

    let tag_len = if let Some(rest) = rest.strip_prefix('<') {
        let Some(end) = verbatim_tag_end(rest) else {
            return Err(Error::new(
                "verbatim tag is missing closing `>`",
                line.local_span(local_start, local_start + text.len()),
            ));
        };
        3 + end
    } else {
        1 + rest
            .char_indices()
            .find_map(|(idx, ch)| {
                (ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}')).then_some(idx)
            })
            .unwrap_or(rest.len())
    };

    let raw = &text[..tag_len];
    if raw == "!!" {
        return Err(Error::new(
            "tag name cannot be empty",
            line.local_span(local_start, local_start + tag_len),
        ));
    }
    let after_tag = &text[tag_len..];
    if !after_tag.is_empty() && !after_tag.starts_with(char::is_whitespace) {
        return Err(Error::new(
            "tag must be separated from the node value by whitespace",
            line.local_span(local_start, local_start + tag_len),
        ));
    }
    let rest_ws = after_tag.len() - after_tag.trim_start().len();
    Ok(Some(TagToken {
        tag: Tag::new(raw),
        span: line.local_span(local_start, local_start + tag_len),
        rest: after_tag.trim_start(),
        rest_start: local_start + tag_len + rest_ws,
    }))
}

fn reject_alias_with_node_properties(text: &str, line: &Line, local_start: usize) -> Result<()> {
    let trimmed = text.trim_start();
    let trimmed_start = local_start + text.len() - trimmed.len();
    if let Some(alias) = parse_metadata_token(trimmed, line, trimmed_start, '*')? {
        return Err(Error::new(
            "alias nodes cannot have anchor or tag properties",
            alias.span,
        ));
    }
    Ok(())
}

fn reject_same_line_block_sequence_after_property(
    text: &str,
    line: &Line,
    local_start: usize,
) -> Result<()> {
    let trimmed = text.trim_start();
    let trimmed_start = local_start + text.len() - trimmed.len();
    if sequence_rest(trimmed).is_some() {
        return Err(Error::new(
            "block sequence entries are not allowed in this context",
            line.local_span(trimmed_start, trimmed_start + 1),
        ));
    }
    Ok(())
}

fn empty_node_property_line(line: &Line, source: &str) -> Result<bool> {
    let content = line.content(source);
    let text = content.trim();
    let local_start = content.len() - content.trim_start().len();
    if let Some(anchor) = parse_metadata_token(text, line, local_start, '&')? {
        return Ok(anchor.rest.trim().is_empty());
    }
    if let Some(tag) = parse_tag_token(text, line, local_start)? {
        return Ok(tag.rest.trim().is_empty());
    }
    Ok(false)
}

fn verbatim_tag_end(rest: &str) -> Option<usize> {
    rest.char_indices().find_map(|(idx, ch)| {
        if ch != '>' {
            return None;
        }
        let after = rest[idx + ch.len_utf8()..].chars().next();
        after.is_none_or(tag_token_can_end_before).then_some(idx)
    })
}

fn tag_token_can_end_before(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}' | ':')
}

fn metadata_name_len(text: &str) -> usize {
    text.char_indices()
        .find_map(|(idx, ch)| {
            (ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}')).then_some(idx)
        })
        .unwrap_or(text.len())
}

fn tagged_node(tag: Tag, tag_span: Span, mut node: Node) -> Node {
    let span = Span::new(
        tag_span.start,
        node.span.end.max(tag_span.end),
        tag_span.line,
        tag_span.column,
    );
    if tag.is_non_specific() {
        return non_specific_tagged_node(span, node);
    }
    if core_scalar_tag_preserves_source(&tag)
        && let Some(source) = node
            .scalar_source()
            .map(|source| source.raw().to_string())
            .or_else(|| matches!(&node.value, Value::Null).then(String::new))
    {
        node = Node::new(Value::String(source.clone()), node.span).with_scalar_source(source);
    }
    Node::new(
        Value::Tagged(Box::new(TaggedNode {
            tag,
            tag_span,
            value: node,
        })),
        span,
    )
}

fn non_specific_tagged_node(span: Span, mut node: Node) -> Node {
    node.span = span;
    match &node.value {
        Value::Sequence(_) | Value::Mapping(_) | Value::String(_) | Value::Tagged(_) => node,
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            let source = node
                .scalar_source()
                .map(|source| source.raw().to_string())
                .unwrap_or_default();
            Node::new(Value::String(source.clone()), span).with_scalar_source(source)
        }
    }
}

fn core_scalar_tag_preserves_source(tag: &Tag) -> bool {
    ["binary", "bool", "float", "int", "null", "str", "timestamp"]
        .iter()
        .any(|suffix| tag.is_yaml_core(suffix))
}

fn count_nodes(node: &Node) -> usize {
    match &node.value {
        Value::Sequence(items) => 1 + items.iter().map(count_nodes).sum::<usize>(),
        Value::Mapping(entries) => {
            1 + entries
                .iter()
                .map(|(key, value)| count_nodes(key) + count_nodes(value))
                .sum::<usize>()
        }
        Value::Tagged(tagged) => 1 + count_nodes(&tagged.value),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 1,
    }
}

fn node_depth(node: &Node) -> usize {
    match &node.value {
        Value::Sequence(items) => 1 + items.iter().map(node_depth).max().unwrap_or(0),
        Value::Mapping(entries) => {
            1 + entries
                .iter()
                .map(|(key, value)| node_depth(key).max(node_depth(value)))
                .max()
                .unwrap_or(0)
        }
        Value::Tagged(tagged) => 1 + node_depth(&tagged.value),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 1,
    }
}

fn event_scalar_value(node: &Node) -> String {
    match &node.value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(Number::Integer(value)) => node
            .scalar_source()
            .map(|source| source.raw().to_string())
            .unwrap_or_else(|| value.to_string()),
        Value::Number(Number::Unsigned(value)) => node
            .scalar_source()
            .map(|source| source.raw().to_string())
            .unwrap_or_else(|| value.to_string()),
        Value::Number(Number::Float(value)) => node
            .scalar_source()
            .map(|source| source.raw().to_string())
            .unwrap_or_else(|| value.to_string()),
        Value::String(_) => node
            .as_str()
            .expect("string node has semantic string value")
            .to_string(),
        Value::Tagged(tagged) => event_scalar_value(&tagged.value),
        Value::Sequence(_) | Value::Mapping(_) => String::new(),
    }
}

fn event_scalar_value_len(node: &Node) -> usize {
    match &node.value {
        Value::Null => "null".len(),
        Value::Bool(value) => value.to_string().len(),
        Value::Number(Number::Integer(value)) => node
            .scalar_source()
            .map(|source| source.raw().len())
            .unwrap_or_else(|| value.to_string().len()),
        Value::Number(Number::Unsigned(value)) => node
            .scalar_source()
            .map(|source| source.raw().len())
            .unwrap_or_else(|| value.to_string().len()),
        Value::Number(Number::Float(value)) => node
            .scalar_source()
            .map(|source| source.raw().len())
            .unwrap_or_else(|| value.to_string().len()),
        Value::String(_) => node
            .as_str()
            .expect("string node has semantic string value")
            .len(),
        Value::Tagged(tagged) => event_scalar_value_len(&tagged.value),
        Value::Sequence(_) | Value::Mapping(_) => 0,
    }
}

fn parse_scalar_with_span(text: &str, span: Span) -> Result<Node> {
    parse_scalar_with_schema(text, span, Schema::Yaml12)
}

fn parse_scalar_with_schema(text: &str, span: Span, schema: Schema) -> Result<Node> {
    parse_scalar_with_schema_and_source(text, span, schema, None)
}

fn parse_source_scalar_with_schema(
    text: &str,
    span: Span,
    schema: Schema,
    source: &Rc<str>,
) -> Result<Node> {
    parse_scalar_with_schema_and_source(text, span, schema, Some(source))
}

fn parse_scalar_with_schema_and_source(
    text: &str,
    span: Span,
    schema: Schema,
    source: Option<&Rc<str>>,
) -> Result<Node> {
    if schema == Schema::Failsafe {
        return parse_failsafe_scalar(text, span, source);
    }
    if schema == Schema::Json {
        return parse_json_scalar(text, span, source);
    }
    if text.is_empty() || text == "~" || text.eq_ignore_ascii_case("null") {
        return Ok(node_with_scalar_source(Value::Null, span, text, source));
    }
    if schema.is_legacy_compatible()
        && let Some(value) = parse_yaml11_bool(text)
    {
        return Ok(node_with_scalar_source(
            Value::Bool(value),
            span,
            text,
            source,
        ));
    }
    if text == "true" || text == "True" || text == "TRUE" {
        return Ok(node_with_scalar_source(
            Value::Bool(true),
            span,
            text,
            source,
        ));
    }
    if text == "false" || text == "False" || text == "FALSE" {
        return Ok(node_with_scalar_source(
            Value::Bool(false),
            span,
            text,
            source,
        ));
    }
    if text.starts_with('"') {
        return parse_double_quoted(text, span, source);
    }
    if text.starts_with('\'') {
        return parse_single_quoted(text, span, source);
    }
    if schema.is_legacy_compatible() && is_yaml11_timestamp(text) {
        return Ok(yaml11_timestamp_node(text, span, source));
    }
    if schema.is_legacy_compatible()
        && let Some(number) = parse_yaml11_number(text)?
    {
        return Ok(node_with_scalar_source(
            Value::Number(number),
            span,
            text,
            source,
        ));
    }
    if schema.is_legacy_compatible() && is_yaml11_invalid_octal(text) {
        return Ok(string_node(text, span, source, true));
    }
    match parse_number(text)? {
        NumberParse::Number(number) => {
            return Ok(node_with_scalar_source(
                Value::Number(number),
                span,
                text,
                source,
            ));
        }
        NumberParse::InvalidInteger => {
            return Ok(string_node(text, span, source, true));
        }
        NumberParse::PlainScalar => {}
    }
    Ok(string_node(text, span, source, false))
}

fn parse_failsafe_scalar(text: &str, span: Span, source: Option<&Rc<str>>) -> Result<Node> {
    if text.starts_with('"') {
        return parse_double_quoted(text, span, source);
    }
    if text.starts_with('\'') {
        return parse_single_quoted(text, span, source);
    }
    Ok(string_node(text, span, source, true))
}

fn parse_json_scalar(text: &str, span: Span, source: Option<&Rc<str>>) -> Result<Node> {
    if text == "null" {
        return Ok(node_with_scalar_source(Value::Null, span, text, source));
    }
    if text == "true" {
        return Ok(node_with_scalar_source(
            Value::Bool(true),
            span,
            text,
            source,
        ));
    }
    if text == "false" {
        return Ok(node_with_scalar_source(
            Value::Bool(false),
            span,
            text,
            source,
        ));
    }
    if text.starts_with('"') {
        return parse_double_quoted(text, span, source);
    }
    if text.starts_with('\'') {
        return parse_single_quoted(text, span, source);
    }
    if is_json_number(text)
        && let NumberParse::Number(number) = parse_number(text)?
    {
        return Ok(node_with_scalar_source(
            Value::Number(number),
            span,
            text,
            source,
        ));
    }
    Ok(string_node(text, span, source, true))
}

fn parse_yaml11_bool(text: &str) -> Option<bool> {
    yaml11::parse_bool(text)
}

fn yaml11_timestamp_node(text: &str, span: Span, source: Option<&Rc<str>>) -> Node {
    let value = string_node(text, span, source, true);
    Node::new(
        Value::Tagged(Box::new(TaggedNode {
            tag: Tag::new("!!timestamp"),
            tag_span: span,
            value,
        })),
        span,
    )
}

fn is_yaml11_timestamp(text: &str) -> bool {
    Timestamp::parse_yaml_1_1(text).is_some()
}

fn parse_single_quoted(text: &str, span: Span, source: Option<&Rc<str>>) -> Result<Node> {
    if !text.ends_with('\'') || text.len() < 2 {
        return Err(Error::new("unterminated single-quoted scalar", span));
    }
    let inner = &text[1..text.len() - 1];
    if inner.contains("''") {
        return Ok(Node::new(Value::String(inner.replace("''", "'")), span));
    }
    Ok(quoted_string_node(inner, span, source))
}

fn parse_double_quoted(text: &str, span: Span, source: Option<&Rc<str>>) -> Result<Node> {
    if !text.ends_with('"') || text.len() < 2 {
        return Err(Error::new("unterminated double-quoted scalar", span));
    }
    let inner = &text[1..text.len() - 1];
    if !inner.as_bytes().contains(&b'\\') {
        return Ok(quoted_string_node(inner, span, source));
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(Error::new("unterminated escape sequence", span));
        };
        match escaped {
            ' ' => out.push(' '),
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            '0' => out.push('\0'),
            'a' => out.push('\u{0007}'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000C}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\t' => out.push('\t'),
            'v' => out.push('\u{000B}'),
            'e' => out.push('\u{001B}'),
            '_' => out.push('\u{00A0}'),
            'N' => out.push('\u{0085}'),
            'L' => out.push('\u{2028}'),
            'P' => out.push('\u{2029}'),
            'x' => {
                let code = take_hex_escape(&mut chars, 2, span)?;
                out.push(char_from_escape(code, span)?);
            }
            'u' => {
                let code = take_hex_escape(&mut chars, 4, span)?;
                out.push(char_from_escape(code, span)?);
            }
            'U' => {
                let code = take_hex_escape(&mut chars, 8, span)?;
                out.push(char_from_escape(code, span)?);
            }
            _ => return Err(Error::new("unsupported escape sequence", span)),
        }
    }
    Ok(Node::new(Value::String(out), span))
}

fn node_with_scalar_source(
    value: Value,
    span: Span,
    text: &str,
    _source: Option<&Rc<str>>,
) -> Node {
    let node = Node::new(value, span);
    node.with_scalar_source(text)
}

fn string_node(text: &str, span: Span, source: Option<&Rc<str>>, retain_source: bool) -> Node {
    if let Some(source) = source {
        return Node::source_backed_string(Rc::clone(source), span, span.start, span.end);
    }
    let node = Node::new(Value::String(text.to_string()), span);
    if retain_source {
        node.with_scalar_source(text)
    } else {
        node
    }
}

fn quoted_string_node(inner: &str, span: Span, source: Option<&Rc<str>>) -> Node {
    if let Some(source) = source {
        return Node::source_backed_string(
            Rc::clone(source),
            span,
            span.start + 1,
            span.end.saturating_sub(1),
        );
    }
    Node::new(Value::String(inner.to_string()), span)
}

fn quoted_scalar_is_closed(text: &str, quote: char) -> bool {
    quoted_scalar_close_end(text, quote)
        .is_some_and(|end| text[end..].chars().all(char::is_whitespace))
}

fn quoted_scalar_accepted_end(text: &str, quote: char) -> Option<usize> {
    let end = quoted_scalar_close_end(text, quote)?;
    let trailing = &text[end..];
    if trailing.chars().all(char::is_whitespace) {
        return Some(end);
    }
    let separated_comment = trailing.chars().next().is_some_and(char::is_whitespace)
        && trailing.trim_start().starts_with('#');
    separated_comment.then_some(end)
}

fn quoted_scalar_trailing_start(text: &str, quote: char) -> Option<usize> {
    let end = quoted_scalar_close_end(text, quote)?;
    let trailing = &text[end..];
    (!trailing.trim().is_empty()).then(|| end + trailing.len() - trailing.trim_start().len())
}

fn quoted_scalar_close_end(text: &str, quote: char) -> Option<usize> {
    if !text.starts_with(quote) {
        return None;
    }
    let mut chars = text[quote.len_utf8()..].chars().peekable();
    let mut escaped = false;
    let mut offset = quote.len_utf8();
    while let Some(ch) = chars.next() {
        if quote == '"' && escaped {
            escaped = false;
            offset += ch.len_utf8();
            continue;
        }
        if quote == '"' && ch == '\\' {
            escaped = true;
            offset += ch.len_utf8();
            continue;
        }
        if quote == '\'' && ch == '\'' && chars.peek() == Some(&'\'') {
            chars.next();
            offset += ch.len_utf8() * 2;
            continue;
        }
        if ch == quote {
            return Some(offset + ch.len_utf8());
        }
        offset += ch.len_utf8();
    }
    None
}

fn take_hex_escape(chars: &mut std::str::Chars<'_>, digits: usize, span: Span) -> Result<u32> {
    let mut value = 0u32;
    for _ in 0..digits {
        let Some(ch) = chars.next() else {
            return Err(Error::new("unterminated escape sequence", span));
        };
        let Some(digit) = ch.to_digit(16) else {
            return Err(Error::new("invalid hex escape sequence", span));
        };
        value = (value << 4) | digit;
    }
    Ok(value)
}

fn char_from_escape(code: u32, span: Span) -> Result<char> {
    char::from_u32(code).ok_or_else(|| Error::new("invalid Unicode escape scalar", span))
}

enum NumberParse {
    Number(Number),
    InvalidInteger,
    PlainScalar,
}

fn parse_number(text: &str) -> Result<NumberParse> {
    if !could_be_number(text) {
        return Ok(NumberParse::PlainScalar);
    }
    let compact = compact_number_text(text);
    let compact = compact.as_ref();
    if let Some(number) = parse_special_float(compact) {
        return Ok(NumberParse::Number(number));
    }
    if is_int_like(text) {
        if compact.starts_with('-') {
            return match compact.parse::<i128>() {
                Ok(value) => Ok(NumberParse::Number(Number::Integer(value))),
                Err(_) => Ok(NumberParse::InvalidInteger),
            };
        }
        if let Some(positive) = compact.strip_prefix('+') {
            return parse_positive_integer_number(positive);
        }
        return parse_positive_integer_number(compact);
    }
    if is_float_like(text) {
        match compact.parse::<f64>() {
            Ok(value) => return Ok(NumberParse::Number(Number::Float(value))),
            Err(_) => return Ok(NumberParse::PlainScalar),
        }
    }
    Ok(NumberParse::PlainScalar)
}

fn is_json_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut idx = usize::from(matches!(bytes.first(), Some(b'-')));
    if idx >= bytes.len() {
        return false;
    }

    match bytes[idx] {
        b'0' => {
            idx += 1;
            if matches!(bytes.get(idx), Some(b'0'..=b'9')) {
                return false;
            }
        }
        b'1'..=b'9' => {
            idx += 1;
            while matches!(bytes.get(idx), Some(b'0'..=b'9')) {
                idx += 1;
            }
        }
        _ => return false,
    }

    if matches!(bytes.get(idx), Some(b'.')) {
        idx += 1;
        let fraction_start = idx;
        while matches!(bytes.get(idx), Some(b'0'..=b'9')) {
            idx += 1;
        }
        if idx == fraction_start {
            return false;
        }
    }

    if matches!(bytes.get(idx), Some(b'e' | b'E')) {
        idx += 1;
        if matches!(bytes.get(idx), Some(b'+' | b'-')) {
            idx += 1;
        }
        let exponent_start = idx;
        while matches!(bytes.get(idx), Some(b'0'..=b'9')) {
            idx += 1;
        }
        if idx == exponent_start {
            return false;
        }
    }

    idx == bytes.len()
}

fn could_be_number(text: &str) -> bool {
    matches!(
        text.as_bytes().first(),
        Some(b'+' | b'-' | b'.' | b'0'..=b'9')
    )
}

fn compact_number_text(text: &str) -> Cow<'_, str> {
    if text.as_bytes().contains(&b'_') {
        Cow::Owned(text.replace('_', ""))
    } else {
        Cow::Borrowed(text)
    }
}

fn parse_yaml11_number(text: &str) -> Result<Option<Number>> {
    Ok(yaml11::parse_implicit_numeric_extension(text))
}

fn is_yaml11_invalid_octal(text: &str) -> bool {
    let compact = text.replace('_', "");
    let positive = compact
        .strip_prefix('-')
        .or_else(|| compact.strip_prefix('+'))
        .unwrap_or(&compact);
    positive.len() > 1
        && positive.starts_with('0')
        && positive.chars().all(|ch| ch.is_ascii_digit())
        && positive.chars().any(|ch| matches!(ch, '8' | '9'))
}

fn parse_special_float(compact: &str) -> Option<Number> {
    if compact.eq_ignore_ascii_case(".nan") {
        Some(Number::from(f64::NAN))
    } else if compact.eq_ignore_ascii_case(".inf") || compact.eq_ignore_ascii_case("+.inf") {
        Some(Number::from(f64::INFINITY))
    } else if compact.eq_ignore_ascii_case("-.inf") {
        Some(Number::from(f64::NEG_INFINITY))
    } else {
        None
    }
}

fn parse_positive_integer_number(compact: &str) -> Result<NumberParse> {
    match compact.parse::<i64>() {
        Ok(value) => Ok(NumberParse::Number(Number::Integer(i128::from(value)))),
        Err(_) => match compact.parse::<u64>() {
            Ok(value) => Ok(NumberParse::Number(Number::Unsigned(u128::from(value)))),
            Err(_) => match compact.parse::<i128>() {
                Ok(value) => Ok(NumberParse::Number(Number::Integer(value))),
                Err(_) => compact
                    .parse::<u128>()
                    .map(Number::Unsigned)
                    .map(NumberParse::Number)
                    .or(Ok(NumberParse::InvalidInteger)),
            },
        },
    }
}

fn is_int_like(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut idx = usize::from(matches!(bytes[0], b'+' | b'-'));
    let mut digits = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'0'..=b'9' => digits += 1,
            b'_' => {}
            _ => return false,
        }
        idx += 1;
    }
    digits > 0
}

fn is_float_like(text: &str) -> bool {
    if !(text.contains('.') || text.contains('e') || text.contains('E')) {
        return false;
    }
    let bytes = text.as_bytes();
    let mut digits = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'+' | b'-' if idx == 0 => {}
            b'+' | b'-' if matches!(bytes.get(idx.wrapping_sub(1)), Some(b'e' | b'E')) => {}
            b'.' | b'e' | b'E' | b'_' => {}
            _ => return false,
        }
    }
    digits > 0
}

fn flow_collection_is_closed(text: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut started = false;

    for ch in text.chars() {
        if double && escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if double => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            '[' | '{' if !single && !double => {
                started = true;
                depth += 1;
            }
            ']' | '}' if !single && !double && depth > 0 => {
                depth -= 1;
                if started && depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
    }

    false
}

fn flow_continuation_may_start_at_parent_indent(ch: char) -> bool {
    matches!(ch, ',' | ']' | '}')
}

fn flow_continuation_may_start_at_parent_indent_after_ws(content: &str) -> bool {
    content
        .chars()
        .find(|ch| !ch.is_whitespace())
        .is_some_and(flow_continuation_may_start_at_parent_indent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceMark {
    offset: usize,
    line: usize,
    column: usize,
}

impl SourceMark {
    fn new(offset: usize, line: usize, column: usize) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }

    fn for_line_content(line: &Line, local_start: usize) -> Self {
        Self::new(
            line.start() + line.indent() + local_start,
            line.no(),
            line.indent() + local_start + 1,
        )
    }
}

struct FlowBuffer {
    text: String,
    marks: Vec<SourceMark>,
}

impl FlowBuffer {
    fn single(text: &str, line: &Line, local_start: usize) -> Self {
        let mut buffer = Self {
            text: String::new(),
            marks: Vec::with_capacity(text.len() + 1),
        };
        buffer.push_source_text(text, line, local_start);
        buffer
    }

    fn push_source_text(&mut self, text: &str, line: &Line, local_start: usize) {
        if text.is_empty() {
            return;
        }
        let start = SourceMark::for_line_content(line, local_start);
        if self.marks.is_empty() {
            self.marks.push(start);
        }
        self.text.push_str(text);
        for offset in 1..=text.len() {
            self.marks.push(SourceMark::new(
                start.offset + offset,
                line.no(),
                line.indent() + local_start + offset + 1,
            ));
        }
    }

    fn push_virtual_separator(&mut self, next: SourceMark) {
        if self.text.is_empty() {
            self.marks.push(next);
            return;
        }
        self.text.push('\n');
        self.marks.push(next);
    }

    fn span(&self, start: usize, end: usize) -> Span {
        let start = self.marks[start];
        let end = self.marks[end];
        Span::new(start.offset, end.offset, start.line, start.column)
    }
}

struct FlowParser<'a> {
    buffer: FlowBuffer,
    pos: usize,
    depth: usize,
    schema: Schema,
    anchors: Option<&'a mut AnchorRegistry>,
    events: Option<&'a mut EventRecorder>,
    active_tag_handles: &'a HashMap<String, String>,
    options: LoadOptions,
    pending_anchor: Option<PendingAnchor>,
    has_merge_key: bool,
}

impl<'a> FlowParser<'a> {
    fn new(
        buffer: FlowBuffer,
        depth: usize,
        anchors: &'a mut AnchorRegistry,
        events: Option<&'a mut EventRecorder>,
        active_tag_handles: &'a HashMap<String, String>,
        schema: Schema,
        options: LoadOptions,
    ) -> Self {
        Self {
            buffer,
            pos: 0,
            depth,
            schema,
            anchors: Some(anchors),
            events,
            active_tag_handles,
            options,
            pending_anchor: None,
            has_merge_key: false,
        }
    }

    fn parse(mut self) -> Result<(Node, bool)> {
        let node = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.buffer.text.len() {
            return Err(Error::new(
                "unexpected trailing characters in flow value",
                self.span(self.pos, self.buffer.text.len()),
            ));
        }
        Ok((node, self.has_merge_key))
    }

    fn emit_sequence_start(&mut self, style: CollectionStyle, span: Span) {
        if let Some(events) = &mut self.events {
            let meta = events.take_meta();
            events
                .events
                .push(Event::SequenceStart { meta, style, span });
        }
    }

    fn emit_sequence_end(&mut self, span: Span) {
        if let Some(events) = &mut self.events {
            events.events.push(Event::SequenceEnd { span });
        }
    }

    fn emit_mapping_start(&mut self, style: CollectionStyle, span: Span) {
        if let Some(events) = &mut self.events {
            let meta = events.take_meta();
            events
                .events
                .push(Event::MappingStart { meta, style, span });
        }
    }

    fn emit_mapping_end(&mut self, span: Span) {
        if let Some(events) = &mut self.events {
            events.events.push(Event::MappingEnd { span });
        }
    }

    fn emit_alias(&mut self, name: String, span: Span) {
        if let Some(events) = &mut self.events {
            events.events.push(Event::Alias {
                anchor: EventAnchor { name, span },
            });
        }
    }

    fn emit_scalar_node(&mut self, node: &Node, style: ScalarStyle) {
        if let Some(events) = &mut self.events {
            let meta = events.take_meta();
            events.events.push(Event::Scalar {
                value: event_scalar_value(node),
                style,
                meta,
                span: node.span,
            });
        }
    }

    fn emit_null_scalar(&mut self, span: Span) {
        if let Some(events) = &mut self.events {
            let meta = events.take_meta();
            events.events.push(Event::Scalar {
                value: "null".to_string(),
                style: ScalarStyle::Plain,
                meta,
                span,
            });
        }
    }

    fn push_anchor_meta(&mut self, name: String, span: Span) {
        if let Some(events) = &mut self.events {
            events.pending_meta.anchor = Some(EventAnchor { name, span });
        }
    }

    fn push_tag_meta(&mut self, tag: Tag, span: Span) {
        if let Some(events) = &mut self.events {
            events.pending_meta.tag = Some(EventTag { tag, span });
        }
    }

    fn finish_deferred_anchor(&mut self, node: &Node) -> Result<()> {
        if let Some(anchor) = self.pending_anchor.take() {
            self.with_anchors(node.span, |anchors| {
                anchors.finish(&anchor.name, anchor.generation, node.clone());
                Ok(())
            })?;
        }
        Ok(())
    }

    fn recording_events(&self) -> bool {
        self.events.is_some()
    }

    fn record_merge_key(&mut self, key: &Node) {
        if node_is_merge_key(key) {
            self.has_merge_key = true;
        }
    }

    fn check_duplicate_key(&mut self, seen: &mut DuplicateKeyTracker, key: &Node) -> Result<()> {
        self.record_merge_key(key);
        check_duplicate_for_schema(
            self.recording_events(),
            self.schema,
            self.options,
            seen,
            key,
        )
    }

    fn resolve_tag(&self, tag: Tag, span: Span) -> Result<Tag> {
        resolve_tag(self.active_tag_handles, tag, span)
    }

    fn parse_value(&mut self) -> Result<Node> {
        self.options
            .check_nesting_depth(self.depth, self.span(self.pos, self.pos))?;
        self.skip_ws();
        match self.peek() {
            Some('[') => self.parse_sequence(),
            Some('{') => self.parse_mapping(),
            Some('"') => {
                let (text, start, end) = self.take_quoted('"')?;
                let node = parse_double_quoted(&text, self.span(start, end), None)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::DoubleQuoted);
                Ok(node)
            }
            Some('\'') => {
                let (text, start, end) = self.take_quoted('\'')?;
                let node = parse_single_quoted(&text, self.span(start, end), None)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::SingleQuoted);
                Ok(node)
            }
            Some('&') => self.parse_anchor_value(false),
            Some('*') => self.parse_alias_value(),
            Some('!') => self.parse_tag_value(),
            Some(_) => {
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if matches!(ch, ',' | ']' | '}') {
                        break;
                    }
                    self.bump(ch);
                }
                let end = self.pos;
                let node = self.parse_plain_flow_scalar(start, end)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::Plain);
                Ok(node)
            }
            None => Err(Error::new(
                "expected flow value",
                self.span(self.pos, self.pos),
            )),
        }
    }

    fn parse_sequence(&mut self) -> Result<Node> {
        let start = self.pos;
        self.expect('[')?;
        self.emit_sequence_start(CollectionStyle::Flow, self.span(start, self.pos));
        let mut items = NodeItems::new();
        loop {
            self.skip_ws();
            if self.consume(']') {
                let span = self.span(start, self.pos);
                self.emit_sequence_end(span);
                return Ok(sequence_node(items.into_vec(), span));
            }
            if self.peek() == Some(',') {
                return Err(Error::new(
                    "unexpected comma in flow sequence",
                    self.span(self.pos, self.pos + 1),
                ));
            }
            let item_start = self.pos;
            let item = self.with_depth(|parser| parser.parse_sequence_item())?;
            let item_end = self.pos;
            self.skip_ws();
            if self.consume(',') {
                items.push(item);
                self.check_collection_items(items.len(), self.span(start, self.pos))?;
                self.skip_ws();
                if self.consume(']') {
                    let span = self.span(start, self.pos);
                    self.emit_sequence_end(span);
                    return Ok(sequence_node(items.into_vec(), span));
                }
                if self.peek() == Some(',') {
                    return Err(Error::new(
                        "unexpected comma in flow sequence",
                        self.span(self.pos, self.pos + 1),
                    ));
                }
                continue;
            }
            self.reject_missing_flow_sequence_comma(item_start, item_end, &item)?;
            items.push(item);
            self.check_collection_items(items.len(), self.span(start, self.pos))?;
            self.expect(']')?;
            let span = self.span(start, self.pos);
            self.emit_sequence_end(span);
            return Ok(sequence_node(items.into_vec(), span));
        }
    }

    fn parse_anchor_value(&mut self, defer_finish: bool) -> Result<Node> {
        let Some(anchor) = self.take_metadata_token('&')? else {
            unreachable!("parse_anchor_value is only called at an anchor token");
        };
        self.skip_ws();
        self.reject_alias_with_node_properties_at_current_position()?;
        self.push_anchor_meta(anchor.name.clone(), anchor.span);
        let generation = self.with_anchors(anchor.span, |anchors| {
            Ok(anchors.begin(anchor.name.clone(), anchor.span))
        })?;
        let node = if matches!(self.peek(), None | Some(',' | ']' | '}')) {
            self.emit_null_scalar(anchor.span);
            Node::null(anchor.span)
        } else {
            self.with_depth(|parser| parser.parse_value())?
        };
        if defer_finish {
            debug_assert!(self.pending_anchor.is_none());
            self.pending_anchor = Some(PendingAnchor {
                name: anchor.name,
                generation,
            });
        } else {
            self.with_anchors(anchor.span, |anchors| {
                anchors.finish(&anchor.name, generation, node.clone());
                Ok(())
            })?;
        }
        Ok(node)
    }

    fn parse_alias_value(&mut self) -> Result<Node> {
        let Some(alias) = self.take_metadata_token('*')? else {
            unreachable!("parse_alias_value is only called at an alias token");
        };
        self.emit_alias(alias.name.clone(), alias.span);
        if self.recording_events() {
            self.with_anchors(alias.span, |anchors| {
                anchors.validate_alias(&alias.name, alias.span)
            })?;
            return Ok(Node::null(alias.span));
        }
        let depth = self.depth;
        self.with_anchors(alias.span, |anchors| {
            anchors.resolve(&alias.name, alias.span, depth)
        })
    }

    fn parse_tag_value(&mut self) -> Result<Node> {
        let Some(tag) = self.take_tag_token()? else {
            unreachable!("parse_tag_value is only called at a tag token");
        };
        let tag_value = self.resolve_tag(tag.tag, tag.span)?;
        self.push_tag_meta(tag_value.clone(), tag.span);
        self.skip_ws();
        self.reject_alias_with_node_properties_at_current_position()?;
        let node = if matches!(self.peek(), None | Some(',' | ']' | '}')) {
            self.emit_null_scalar(tag.span);
            Node::null(tag.span)
        } else if self.peek() == Some('&') {
            self.parse_anchor_value(true)?
        } else {
            self.with_depth(|parser| parser.parse_value())?
        };
        let node = tagged_node(tag_value, tag.span, node);
        self.finish_deferred_anchor(&node)?;
        Ok(node)
    }

    fn parse_mapping(&mut self) -> Result<Node> {
        let start = self.pos;
        self.expect('{')?;
        self.emit_mapping_start(CollectionStyle::Flow, self.span(start, self.pos));
        let mut entries = NodeEntries::new();
        let mut seen = DuplicateKeyTracker::new();
        loop {
            self.skip_ws();
            if self.consume('}') {
                let span = self.span(start, self.pos);
                self.emit_mapping_end(span);
                return Ok(mapping_node(entries.into_vec(), span));
            }
            if self.peek() == Some(',') {
                return Err(Error::new(
                    "unexpected comma in flow mapping",
                    self.span(self.pos, self.pos + 1),
                ));
            }
            let (key, value) = self.with_depth(|parser| parser.parse_flow_mapping_entry())?;
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
            self.check_collection_items(entries.len(), self.span(start, self.pos))?;
            self.skip_ws();
            if self.consume(',') {
                self.skip_ws();
                if self.consume('}') {
                    let span = self.span(start, self.pos);
                    self.emit_mapping_end(span);
                    return Ok(mapping_node(entries.into_vec(), span));
                }
                if self.peek() == Some(',') {
                    return Err(Error::new(
                        "unexpected comma in flow mapping",
                        self.span(self.pos, self.pos + 1),
                    ));
                }
                continue;
            }
            self.expect('}')?;
            let span = self.span(start, self.pos);
            self.emit_mapping_end(span);
            return Ok(mapping_node(entries.into_vec(), span));
        }
    }

    fn parse_sequence_item(&mut self) -> Result<Node> {
        self.skip_ws();
        let Some(colon) = self.flow_sequence_mapping_col() else {
            return self.parse_value();
        };
        self.parse_sequence_mapping_item(colon)
    }

    fn parse_sequence_mapping_item(&mut self, colon: usize) -> Result<Node> {
        let start = self.pos;
        self.emit_mapping_start(CollectionStyle::Flow, self.span(start, start));
        if self.consume_explicit_flow_key_indicator() {
            self.skip_ws();
        }
        let key = if self.pos == colon {
            let span = self.span(start, colon);
            self.emit_null_scalar(span);
            Node::empty_scalar(span)
        } else {
            self.parse_flow_key()?
        };
        self.skip_ws();
        if self.pos != colon {
            return Err(Error::new(
                "expected `:` in flow mapping entry",
                self.span(self.pos, self.pos),
            ));
        }
        self.pos = colon + 1;
        let value = self.with_depth(|parser| parser.parse_value_or_null())?;
        let span = Span::new(
            key.span.start,
            value.span.end,
            key.span.line,
            key.span.column,
        );
        self.emit_mapping_end(span);
        self.record_merge_key(&key);
        Ok(mapping_node(vec![(key, value)], span))
    }

    fn parse_flow_mapping_entry(&mut self) -> Result<(Node, Node)> {
        if self.consume_explicit_flow_key_indicator() {
            self.skip_ws();
        }
        let key = self.parse_flow_key()?;
        self.skip_ws();
        let value = if self.consume(':') {
            self.with_depth(|parser| parser.parse_value_or_null())?
        } else if matches!(self.peek(), Some(',') | Some('}')) {
            let span = self.span(self.pos, self.pos);
            self.emit_null_scalar(span);
            Node::empty_scalar(span)
        } else {
            return Err(Error::new(
                "expected `:` in flow mapping entry",
                self.span(self.pos, self.pos),
            ));
        };
        Ok((key, value))
    }

    fn parse_flow_key(&mut self) -> Result<Node> {
        self.skip_ws();
        match self.peek() {
            Some('[' | '{') => self.parse_value(),
            Some('"') => {
                let (text, start, end) = self.take_quoted('"')?;
                let node = parse_double_quoted(&text, self.span(start, end), None)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::DoubleQuoted);
                Ok(node)
            }
            Some('\'') => {
                let (text, start, end) = self.take_quoted('\'')?;
                let node = parse_single_quoted(&text, self.span(start, end), None)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::SingleQuoted);
                Ok(node)
            }
            Some('&') => self.parse_anchor_key(false),
            Some('*') => self.parse_alias_key(),
            Some('!') => self.parse_tag_key(),
            Some(_) => {
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if matches!(ch, ',' | '}') || self.is_mapping_value_colon() {
                        break;
                    }
                    self.bump(ch);
                }
                let end = self.pos;
                let node = self.parse_plain_flow_scalar(start, end)?;
                self.check_scalar_node(&node)?;
                self.emit_scalar_node(&node, ScalarStyle::Plain);
                Ok(node)
            }
            None => Err(Error::new(
                "expected flow mapping key",
                self.span(self.pos, self.pos),
            )),
        }
    }

    fn parse_anchor_key(&mut self, defer_finish: bool) -> Result<Node> {
        let Some(anchor) = self.take_metadata_token('&')? else {
            unreachable!("parse_anchor_key is only called at an anchor token");
        };
        self.skip_ws();
        self.push_anchor_meta(anchor.name.clone(), anchor.span);
        self.reject_alias_with_node_properties_at_current_position()?;
        let generation = self.with_anchors(anchor.span, |anchors| {
            Ok(anchors.begin(anchor.name.clone(), anchor.span))
        })?;
        let key = if self.at_flow_key_terminator() {
            self.emit_null_scalar(anchor.span);
            Node::null(anchor.span)
        } else {
            self.with_depth(|parser| parser.parse_flow_key())?
        };
        if defer_finish {
            debug_assert!(self.pending_anchor.is_none());
            self.pending_anchor = Some(PendingAnchor {
                name: anchor.name,
                generation,
            });
        } else {
            self.with_anchors(anchor.span, |anchors| {
                anchors.finish(&anchor.name, generation, key.clone());
                Ok(())
            })?;
        }
        Ok(key)
    }

    fn parse_alias_key(&mut self) -> Result<Node> {
        let Some(alias) = self.take_metadata_token('*')? else {
            unreachable!("parse_alias_key is only called at an alias token");
        };
        self.emit_alias(alias.name.clone(), alias.span);
        if self.recording_events() {
            self.with_anchors(alias.span, |anchors| {
                anchors.validate_alias(&alias.name, alias.span)
            })?;
            return Ok(Node::null(alias.span));
        }
        let depth = self.depth;
        self.with_anchors(alias.span, |anchors| {
            anchors.resolve(&alias.name, alias.span, depth)
        })
    }

    fn parse_tag_key(&mut self) -> Result<Node> {
        let Some(tag) = self.take_tag_token()? else {
            unreachable!("parse_tag_key is only called at a tag token");
        };
        let tag_value = self.resolve_tag(tag.tag, tag.span)?;
        self.push_tag_meta(tag_value.clone(), tag.span);
        self.skip_ws();
        self.reject_alias_with_node_properties_at_current_position()?;
        let key = if self.at_flow_key_terminator() {
            self.emit_null_scalar(tag.span);
            Node::null(tag.span)
        } else if self.peek() == Some('&') {
            self.parse_anchor_key(true)?
        } else {
            self.with_depth(|parser| parser.parse_flow_key())?
        };
        let key = tagged_node(tag_value, tag.span, key);
        self.finish_deferred_anchor(&key)?;
        Ok(key)
    }

    fn consume_explicit_flow_key_indicator(&mut self) -> bool {
        if self.peek() != Some('?') {
            return false;
        }
        let next = self.buffer.text[self.pos + '?'.len_utf8()..].chars().next();
        if !next.is_some_and(char::is_whitespace) {
            return false;
        }
        self.bump('?');
        true
    }

    fn take_metadata_token(&mut self, sigil: char) -> Result<Option<MetadataToken<'static>>> {
        if self.peek() != Some(sigil) {
            return Ok(None);
        }
        let start = self.pos;
        self.bump(sigil);
        let name_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}') {
                break;
            }
            self.bump(ch);
        }
        if self.pos == name_start {
            let kind = if sigil == '&' { "anchor" } else { "alias" };
            return Err(Error::new(
                format!("{kind} name cannot be empty"),
                self.span(start, self.pos),
            ));
        }
        Ok(Some(MetadataToken {
            name: self.buffer.text[name_start..self.pos].to_string(),
            span: self.span(start, self.pos),
            rest: "",
            rest_start: self.pos,
        }))
    }

    fn take_tag_token(&mut self) -> Result<Option<TagToken<'static>>> {
        if self.peek() != Some('!') {
            return Ok(None);
        }
        let start = self.pos;
        self.bump('!');
        if self.peek().is_none() {
            return Ok(Some(TagToken {
                tag: Tag::new("!"),
                span: self.span(start, self.pos),
                rest: "",
                rest_start: self.pos,
            }));
        }
        if self
            .peek()
            .is_some_and(|ch| ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}'))
        {
            return Ok(Some(TagToken {
                tag: Tag::new("!"),
                span: self.span(start, self.pos),
                rest: "",
                rest_start: self.pos,
            }));
        }
        if self.consume('<') {
            let suffix_start = self.pos;
            while let Some(ch) = self.peek() {
                if ch == '>' && self.verbatim_tag_closes_here() {
                    let suffix = self.buffer.text[suffix_start..self.pos].to_string();
                    self.bump('>');
                    if suffix.is_empty() {
                        return Err(Error::new(
                            "tag name cannot be empty",
                            self.span(start, self.pos),
                        ));
                    }
                    return Ok(Some(TagToken {
                        tag: Tag {
                            handle: "!".to_string(),
                            suffix,
                        },
                        span: self.span(start, self.pos),
                        rest: "",
                        rest_start: self.pos,
                    }));
                }
                self.bump(ch);
            }
            return Err(Error::new(
                "verbatim tag is missing closing `>`",
                self.span(start, self.pos),
            ));
        }

        let name_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}') {
                break;
            }
            self.bump(ch);
        }
        if self.pos == name_start {
            return Ok(Some(TagToken {
                tag: Tag::new("!"),
                span: self.span(start, self.pos),
                rest: "",
                rest_start: self.pos,
            }));
        }
        let raw = self.buffer.text[start..self.pos].to_string();
        if raw == "!!" {
            return Err(Error::new(
                "tag name cannot be empty",
                self.span(start, self.pos),
            ));
        }
        Ok(Some(TagToken {
            tag: Tag::new(raw),
            span: self.span(start, self.pos),
            rest: "",
            rest_start: self.pos,
        }))
    }

    fn verbatim_tag_closes_here(&self) -> bool {
        let close_end = self.pos + '>'.len_utf8();
        self.buffer.text[close_end..]
            .chars()
            .next()
            .is_none_or(tag_token_can_end_before)
    }

    fn with_anchors<T>(
        &mut self,
        span: Span,
        f: impl FnOnce(&mut AnchorRegistry) -> Result<T>,
    ) -> Result<T> {
        let Some(anchors) = self.anchors.as_deref_mut() else {
            return Err(Error::new(
                "anchors are not supported in this context",
                span,
            ));
        };
        f(anchors)
    }

    fn parse_value_or_null(&mut self) -> Result<Node> {
        self.skip_ws();
        if matches!(self.peek(), Some(',') | Some('}' | ']')) {
            let span = self.span(self.pos, self.pos);
            self.emit_null_scalar(span);
            return Ok(Node::empty_scalar(span));
        }
        self.parse_value()
    }

    fn flow_sequence_mapping_col(&self) -> Option<usize> {
        let mut single = false;
        let mut double = false;
        let mut escaped = false;
        let mut flow_depth = 0usize;
        let mut pos = self.pos;
        while let Some(ch) = self.buffer.text[pos..].chars().next() {
            let idx = pos;
            if double && escaped {
                escaped = false;
                pos += ch.len_utf8();
                continue;
            }
            if !single
                && !double
                && flow_depth == 0
                && matches!(ch, '&' | '*')
                && self.flow_metadata_token_starts_at(idx)
            {
                pos = self.skip_flow_metadata_token(idx, ch);
                continue;
            }
            match ch {
                '\\' if double => escaped = true,
                '"' if !single => double = !double,
                '\'' if !double => single = !single,
                '[' | '{' if !single && !double => flow_depth += 1,
                ']' | '}' if !single && !double && flow_depth > 0 => flow_depth -= 1,
                ',' | ']' if !single && !double && flow_depth == 0 => break,
                ':' if !single
                    && !double
                    && flow_depth == 0
                    && self.colon_is_mapping_value_indicator(idx) =>
                {
                    let key = &self.buffer.text[self.pos..idx];
                    if key.contains('\n') && !key.trim_start().starts_with('?') {
                        return None;
                    }
                    return Some(idx);
                }
                _ => {}
            }
            pos += ch.len_utf8();
        }
        None
    }

    fn flow_metadata_token_starts_at(&self, idx: usize) -> bool {
        self.buffer.text[..idx]
            .chars()
            .next_back()
            .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '[' | '{' | ',' | '?'))
    }

    fn skip_flow_metadata_token(&self, idx: usize, sigil: char) -> usize {
        let mut pos = idx + sigil.len_utf8();
        while let Some(ch) = self.buffer.text[pos..].chars().next() {
            if ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}') {
                break;
            }
            pos += ch.len_utf8();
        }
        pos
    }

    fn is_mapping_value_colon(&self) -> bool {
        self.peek() == Some(':') && self.colon_is_mapping_value_indicator(self.pos)
    }

    fn at_flow_key_terminator(&self) -> bool {
        matches!(self.peek(), None | Some(',' | '}')) || self.is_mapping_value_colon()
    }

    fn reject_missing_flow_sequence_comma(
        &self,
        start: usize,
        end: usize,
        item: &Node,
    ) -> Result<()> {
        let raw = &self.buffer.text[start..end];
        let Some(first) = raw.trim_start().chars().next() else {
            return Ok(());
        };
        if matches!(item.value, Value::Mapping(_)) {
            return Ok(());
        }
        if raw_has_content_after_line_break(raw)
            && !matches!(first, '"' | '\'' | '[' | '{' | '&' | '*' | '!')
        {
            return Err(Error::syntax(
                "expected `,` between flow sequence entries",
                self.span(end, end),
            ));
        }
        Ok(())
    }

    fn colon_is_mapping_value_indicator(&self, idx: usize) -> bool {
        let after = self.buffer.text[idx + ':'.len_utf8()..].chars().next();
        if after.is_none_or(|ch| ch.is_whitespace() || matches!(ch, ',' | ']' | '}')) {
            return true;
        }
        self.buffer.text[..idx]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace())
            .is_some_and(|ch| matches!(ch, '"' | '\'' | ']' | '}'))
    }

    fn with_depth<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn check_scalar_node(&self, node: &Node) -> Result<()> {
        self.options
            .check_scalar_bytes(event_scalar_value_len(node), node.span)
    }

    fn check_collection_items(&self, len: usize, span: Span) -> Result<()> {
        self.options.check_collection_items(len, span)
    }

    fn take_quoted(&mut self, quote: char) -> Result<(String, usize, usize)> {
        let start = self.pos;
        self.expect(quote)?;
        let mut escaped = false;
        while let Some(ch) = self.peek() {
            self.bump(ch);
            if quote == '"' && escaped {
                escaped = false;
                continue;
            }
            if quote == '"' && ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                let text = self.buffer.text[start..self.pos].to_string();
                return Ok((fold_flow_quoted_scalar(&text, quote), start, self.pos));
            }
        }
        Err(Error::new(
            "unterminated quoted flow scalar",
            self.span(start, self.pos),
        ))
    }

    fn parse_plain_flow_scalar(&self, start: usize, end: usize) -> Result<Node> {
        let raw = &self.buffer.text[start..end];
        if let Some(colon) = self.plain_flow_scalar_mapping_value_colon(start, end) {
            return Err(Error::syntax(
                "expected `,` between flow mapping entries",
                self.span(colon, colon + ':'.len_utf8()),
            ));
        }
        let leading_trim = raw.len() - raw.trim_start().len();
        let scalar_text = raw.trim();
        if scalar_text.starts_with('#') {
            return Err(Error::syntax(
                "comments must be separated from other tokens by whitespace",
                self.span(start + leading_trim, start + leading_trim + '#'.len_utf8()),
            ));
        }
        if scalar_text == "-" {
            return Err(Error::syntax(
                "plain scalar cannot start with '-' followed by flow punctuation",
                self.span(start + leading_trim, start + leading_trim + '-'.len_utf8()),
            ));
        }
        let span_start = start + leading_trim;
        let span_end = span_start + scalar_text.len();
        let scalar_text = fold_flow_plain_scalar(scalar_text);
        parse_scalar_with_schema(&scalar_text, self.span(span_start, span_end), self.schema)
    }

    fn plain_flow_scalar_mapping_value_colon(&self, start: usize, end: usize) -> Option<usize> {
        self.buffer.text[start..end]
            .char_indices()
            .filter_map(|(offset, ch)| (ch == ':').then_some(start + offset))
            .find(|idx| self.colon_is_mapping_value_indicator(*idx))
    }

    fn reject_alias_with_node_properties_at_current_position(&mut self) -> Result<()> {
        if self.peek() != Some('*') {
            return Ok(());
        }
        let Some(alias) = self.take_metadata_token('*')? else {
            return Ok(());
        };
        Err(Error::new(
            "alias nodes cannot have anchor or tag properties",
            alias.span,
        ))
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            let ch = self.peek().expect("checked");
            self.bump(ch);
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump(expected);
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) -> Result<()> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(Error::new(
                format!("expected `{expected}` in flow value"),
                self.span(self.pos, self.pos),
            ))
        }
    }

    fn peek(&self) -> Option<char> {
        self.buffer.text[self.pos..].chars().next()
    }

    fn bump(&mut self, ch: char) {
        self.pos += ch.len_utf8();
    }

    fn span(&self, start: usize, end: usize) -> Span {
        self.buffer.span(start, end)
    }
}

fn raw_has_content_after_line_break(raw: &str) -> bool {
    raw.split('\n').skip(1).any(|line| !line.trim().is_empty())
}

fn fold_flow_quoted_scalar(text: &str, quote: char) -> String {
    if !text.contains('\n') {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\n' {
            out.push(ch);
            continue;
        }

        if quote == '"' && trailing_backslash_count(&out) % 2 == 1 {
            out.pop();
            skip_flow_line_indent(&mut chars);
            continue;
        }

        fold_flow_line_break(&mut out, &mut chars);
    }
    out
}

fn trailing_backslash_count(text: &str) -> usize {
    text.as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
}

fn fold_flow_plain_scalar(text: &str) -> String {
    if !text.contains('\n') {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\n' {
            out.push(ch);
            continue;
        }

        fold_flow_line_break(&mut out, &mut chars);
    }
    out
}

fn fold_flow_line_break(out: &mut String, chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    skip_flow_line_indent(chars);
    let mut blank_breaks = 0usize;
    while chars.peek() == Some(&'\n') {
        chars.next();
        blank_breaks += 1;
        skip_flow_line_indent(chars);
    }
    if blank_breaks == 0 {
        out.push(' ');
    } else {
        for _ in 0..blank_breaks {
            out.push('\n');
        }
    }
}

fn skip_flow_line_indent(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_line_table_entries_stay_compact() {
        assert!(
            std::mem::size_of::<Line>() <= 40,
            "Line is {} bytes",
            std::mem::size_of::<Line>()
        );
        assert!(
            std::mem::size_of::<LineText>() <= 8,
            "LineText is {} bytes",
            std::mem::size_of::<LineText>()
        );
    }

    #[test]
    fn streaming_parser_tracks_current_schema_without_accumulating_schema_history() {
        let mut input = String::new();
        for _ in 0..128 {
            input.push_str("%YAML 1.1\n---\nlegacy: yes\n...\n---\nmodern: yes\n...\n");
        }

        let mut parser =
            StreamingParser::new(&input, LoadOptions::yaml_version_directive()).unwrap();
        let mut documents = 0usize;
        while let Some(document) = parser.next_raw_document() {
            document.unwrap();
            documents += 1;
            assert!(
                parser.parser.document_schemas.is_empty(),
                "streaming parser must not retain per-document schema history"
            );
            assert!(
                parser.parser.document_has_merge_keys.is_empty(),
                "streaming parser must not retain per-document merge-key history"
            );
            let expected_schema = if documents % 2 == 1 {
                Schema::LegacySerdeYaml
            } else {
                Schema::Yaml12
            };
            assert_eq!(parser.last_document_schema(), expected_schema);
        }

        assert_eq!(documents, 256);
    }

    #[test]
    fn streaming_parser_line_buffer_tracks_largest_document_not_whole_stream() {
        let doc_count = 512usize;
        let mut input = String::new();
        for idx in 0..doc_count {
            input.push_str("---\nservice:\n  name: app-");
            input.push_str(&idx.to_string());
            input.push_str("\n  image: nginx\n  ports:\n    - 80\n");
        }

        let mut parser = StreamingParser::new(&input, LoadOptions::new()).unwrap();
        let mut documents = 0usize;
        while let Some(document) = parser.next_raw_document() {
            document.unwrap();
            documents += 1;
            assert!(
                parser.parser.lines.retained_len() <= 1,
                "completed document should leave only the next boundary line buffered, retained={}",
                parser.parser.lines.retained_len()
            );
        }

        assert_eq!(documents, doc_count);
        assert!(
            parser.parser.lines.max_retained_len() <= 8,
            "line buffer should stay near one document, max_retained={}",
            parser.parser.lines.max_retained_len()
        );
    }
}
