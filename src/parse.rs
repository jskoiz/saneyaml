//! YAML parser entrypoints and raw event types.

use crate::{
    Error, Node, NodeValue as Value, Number, Result, Span, Tag, TaggedNode, Timestamp,
    ast::MergePolicy,
    de::read_to_end_with_options,
    error::utf8_error_span,
    key_identity::{DuplicateKey, check_duplicate},
    schema::{LoadOptions, Schema},
    yaml11,
};
use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    mem,
};

pub(crate) const MAX_DEPTH: usize = 128;

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

#[derive(Clone, Debug, PartialEq, Eq)]
enum LineKind {
    Blank,
    Content,
    Directive,
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Debug)]
struct Line {
    no: usize,
    start: usize,
    raw: String,
    indent: usize,
    content_start: usize,
    content: String,
    kind: LineKind,
    had_comment: bool,
}

impl Line {
    fn span(&self) -> Span {
        Span::new(
            self.start + self.content_start,
            self.start + self.content_start + self.content.len(),
            self.no,
            self.content_start + 1,
        )
    }

    fn local_span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.start + self.content_start + start,
            self.start + self.content_start + end,
            self.no,
            self.content_start + start + 1,
        )
    }

    fn raw_from(&self, indent: usize) -> &str {
        if indent >= self.raw.len() {
            ""
        } else {
            &self.raw[indent..]
        }
    }

    fn raw_content_from(&self, local_start: usize) -> &str {
        let start = self.content_start + local_start;
        if start >= self.raw.len() {
            ""
        } else {
            &self.raw[start..]
        }
    }
}

fn quoted_line_text(line: &Line, local_start: usize, trimmed_text: &str, quote: char) -> String {
    let mut text = trimmed_text.to_string();
    if quote != '"' || trailing_backslash_count(&text) % 2 == 0 {
        return text;
    }

    let raw_text = line.raw_content_from(local_start);
    let Some(stripped) = raw_text.strip_prefix(trimmed_text) else {
        return text;
    };
    if let Some(ch @ (' ' | '\t')) = stripped.chars().next() {
        text.push(ch);
    }
    text
}

fn tab_indentation_error(line: &Line) -> Error {
    Error::new(
        "tabs are not allowed for indentation",
        Span::point(line.start + line.indent, line.no, line.indent + 1),
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
        Err(err) => Err(Error::new(
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

pub(crate) fn parse_document_results_with_options(
    input: &str,
    options: LoadOptions,
) -> Vec<Result<Node>> {
    match Parser::new_with_options(input, options) {
        Ok(mut parser) => {
            let results = parser.parse_document_results();
            let schemas = mem::take(&mut parser.document_schemas);
            apply_merge_keys_to_document_results(results, schemas)
        }
        Err(error) => vec![Err(error)],
    }
}

fn apply_merge_keys_to_document_results(
    results: Vec<Result<Node>>,
    schemas: Vec<Schema>,
) -> Vec<Result<Node>> {
    let mut schemas = schemas.into_iter();
    results
        .into_iter()
        .map(|result| {
            result.and_then(|mut node| {
                let schema = schemas.next().unwrap_or(Schema::Yaml12);
                node.apply_merge_keys_with_policy(merge_policy_for_schema(schema))?;
                Ok(node)
            })
        })
        .collect()
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
/// for semantic document loading.
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
        let input = std::str::from_utf8(input)
            .map_err(|err| Error::new("input is not valid UTF-8", utf8_error_span(input, err)))?;
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
/// retaining a `Vec<Node>`.
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
        let input = std::str::from_utf8(input)
            .map_err(|err| Error::new("input is not valid UTF-8", utf8_error_span(input, err)))?;
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
                Some(
                    node.apply_merge_keys_with_policy(merge_policy_for_schema(schema))
                        .map(|()| node),
                )
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
}

impl StreamingParser {
    fn new(input: &str, options: LoadOptions) -> Result<Self> {
        Ok(Self {
            parser: Parser::new_with_options(input, options)?,
            state: DocumentParseState::default(),
        })
    }

    fn enable_events(&mut self) {
        self.parser.enable_events();
    }

    fn next_raw_document(&mut self) -> Option<Result<Node>> {
        self.parser.parse_next_document_result(&mut self.state)
    }

    fn take_events(&mut self) -> VecDeque<Event> {
        self.parser
            .events
            .as_mut()
            .map(|recorder| recorder.events.drain(..).collect())
            .unwrap_or_default()
    }

    fn last_document_schema(&self) -> Schema {
        self.parser
            .document_schemas
            .last()
            .copied()
            .unwrap_or(Schema::Yaml12)
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
    lines: Vec<Line>,
    pos: usize,
    input_len: usize,
    schema: Schema,
    active_schema: Schema,
    anchors: AnchorRegistry,
    events: Option<EventRecorder>,
    active_tag_handles: HashMap<String, String>,
    pending_tag_handles: HashMap<String, String>,
    pending_document_directives: EventDocumentDirectives,
    pending_directives: bool,
    document_schemas: Vec<Schema>,
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
}

impl AnchorRegistry {
    fn new(expansion_budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            expanded_nodes: 0,
            expansion_budget,
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
                return Err(Error::with_related(
                    format!("recursive alias `{name}` is not supported"),
                    span,
                    "anchor is still being parsed here",
                    *anchor_span,
                ));
            }
            None => {
                return Err(Error::new(format!("unknown anchor `{name}`"), span));
            }
        };

        let node_count = count_nodes(target);
        self.expanded_nodes = self.expanded_nodes.saturating_add(node_count);
        if self.expanded_nodes > self.expansion_budget {
            return Err(Error::new("alias expansion limit exceeded", span));
        }
        if depth.saturating_add(node_depth(target)) > MAX_DEPTH {
            return Err(Error::new(
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
        Err(Error::new(format!("unknown anchor `{name}`"), span))
    }
}

impl Parser {
    fn new_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        options.check_input_len(input.len())?;
        let schema = options.selected_schema();
        let alias_expansion_budget = options.alias_expansion_budget(input.len());
        Ok(Self {
            lines: preprocess(input)?,
            pos: 0,
            input_len: input.len(),
            schema,
            active_schema: default_construction_schema(schema),
            anchors: AnchorRegistry::new(alias_expansion_budget),
            events: None,
            active_tag_handles: HashMap::new(),
            pending_tag_handles: HashMap::new(),
            pending_document_directives: EventDocumentDirectives::default(),
            pending_directives: false,
            document_schemas: Vec::new(),
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
        self.emit_scalar("null".to_string(), ScalarStyle::Plain, span);
    }

    fn emit_scalar_node(&mut self, node: &Node, style: ScalarStyle) {
        self.emit_scalar(event_scalar_value(node), style, node.span);
    }

    fn emit_scalar(&mut self, value: String, style: ScalarStyle, span: Span) {
        if let Some(recorder) = &mut self.events {
            let meta = recorder.take_meta();
            recorder.events.push(Event::Scalar {
                value,
                style,
                meta,
                span,
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

    fn check_duplicate_key(
        &self,
        seen: &mut HashMap<DuplicateKey, Span>,
        key: &Node,
    ) -> Result<()> {
        check_duplicate_for_schema(self.recording_events(), self.active_schema, seen, key)
    }

    fn parse_document_results(&mut self) -> Vec<Result<Node>> {
        let mut docs = Vec::new();
        let mut state = DocumentParseState::default();
        while let Some(result) = self.parse_next_document_result(&mut state) {
            docs.push(result);
        }
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
            self.skip_blanks();
            let Some(line) = self.lines.get(self.pos).cloned() else {
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
                    if let Some((rest_start, rest)) = document_start_rest(&line.content) {
                        self.activate_document_schema(&directives);
                        self.emit_document_start(true, directives, marker_span);
                        self.anchors.reset_document();
                        let doc = match self.parse_document_start_value(
                            &line,
                            rest_start,
                            rest,
                            line.indent,
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
        doc
    }

    fn parse_directive(&mut self, line: &Line) -> Result<()> {
        let fields = directive_fields(line);
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

    fn reject_trailing_content_after_document_node(&self) -> Result<()> {
        if let Some(line) = self.peek_content() {
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
        self.skip_blanks();
        if self.peek_content().is_some() {
            self.parse_node(depth)
        } else {
            self.emit_null_scalar(span);
            Ok(Node::null(span))
        }
    }

    fn parse_node(&mut self, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let line = self.current_content()?.clone();
        if line.content.starts_with('\t') {
            if line.indent == 0 && root_tab_separated_flow_collection(&line.content) {
                self.pos += 1;
                return self.parse_document_root_inline_value(
                    &line.content,
                    &line,
                    0,
                    line.indent,
                    depth,
                );
            }
            return Err(tab_indentation_error(&line));
        }
        if sequence_rest(&line.content).is_some() {
            return self.parse_sequence(line.indent, depth);
        }
        if starts_mapping_entry(&line.content) {
            return self.parse_mapping(line.indent, depth);
        }
        self.pos += 1;
        if let Some(header) = parse_block_scalar_header(&line.content, &line, 0)? {
            return self.parse_block_scalar(
                header,
                line.indent,
                line.span(),
                depth + 1,
                line.indent == 0,
            );
        }
        if is_plain_scalar_text(&line.content) {
            return self.parse_plain_scalar(&line.content, &line, 0, line.indent, depth, true);
        }
        if line.indent == 0 {
            self.parse_document_root_inline_value(&line.content, &line, 0, line.indent, depth)
        } else {
            self.parse_inline_value(&line.content, &line, 0, line.indent, depth)
        }
    }

    fn parse_sequence(&mut self, indent: usize, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let start = self.current_content()?.span();
        self.emit_sequence_start(CollectionStyle::Block, start);
        let mut items = Vec::new();

        loop {
            self.skip_blanks();
            let Some(line) = self.peek_content() else {
                break;
            };
            let line = line.clone();
            if line.indent != indent {
                break;
            }
            if line.content.starts_with('\t') {
                return Err(tab_indentation_error(&line));
            }
            let Some((rest_start, rest)) = sequence_rest(&line.content) else {
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
                    line.local_span(line.content.len(), line.content.len()),
                    depth + 1,
                )?
            } else if let Some(header) = parse_block_scalar_header(rest_trim, &line, value_start)? {
                self.parse_block_scalar(
                    header,
                    indent,
                    line.local_span(value_start, line.content.len()),
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
        }

        let end = items.last().map(|item| item.span.end).unwrap_or(start.end);
        let span = Span::new(start.start, end, start.line, start.column);
        self.emit_sequence_end(span);
        Ok(Node::new(Value::Sequence(items), span))
    }

    fn parse_mapping(&mut self, indent: usize, depth: usize) -> Result<Node> {
        self.check_depth(depth)?;
        let start = self.current_content()?.span();
        self.emit_mapping_start(CollectionStyle::Block, start);
        let mut entries = Vec::new();
        let mut seen = HashMap::<DuplicateKey, Span>::new();
        let mut pending_key: Option<(Node, Span)> = None;

        loop {
            self.skip_blanks();
            let Some(line) = self.peek_content() else {
                break;
            };
            if line.indent != indent || sequence_rest(&line.content).is_some() {
                break;
            }
            if line.content.starts_with('\t') {
                return Err(tab_indentation_error(line));
            }
            if !starts_mapping_entry(&line.content) {
                break;
            }
            let line = line.clone();
            self.pos += 1;

            let (key, value) = if let Some((rest_start, rest)) = explicit_key_rest(&line.content) {
                if let Some((key, span)) = pending_key.take() {
                    self.emit_null_scalar(span);
                    let value = Node::null(span);
                    self.check_duplicate_key(&mut seen, &key)?;
                    entries.push((key, value));
                }
                let key =
                    self.parse_explicit_block_key(&line, rest_start, rest, indent, depth + 1)?;
                pending_key = Some((key, line.local_span(0, 1)));
                continue;
            } else if let Some((rest_start, rest)) = explicit_value_rest(&line.content) {
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
                }
                self.parse_mapping_pair(&line, 0, &line.content, indent, depth + 1)?
            };
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
        }
        if let Some((key, span)) = pending_key.take() {
            self.emit_null_scalar(span);
            let value = Node::null(span);
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
        }

        let end = entries
            .last()
            .map(|(_, value)| value.span.end)
            .unwrap_or(start.end);
        let span = Span::new(start.start, end, start.line, start.column);
        self.emit_mapping_end(span);
        Ok(Node::new(Value::Mapping(entries), span))
    }

    fn parse_inline_mapping_item(
        &mut self,
        line: &Line,
        rest_start: usize,
        rest: &str,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        let item_indent = line.indent + rest_start;
        self.emit_mapping_start(
            CollectionStyle::Block,
            line.local_span(rest_start, rest_start + rest.len()),
        );
        let mut entries = Vec::new();
        let mut seen = HashMap::<DuplicateKey, Span>::new();
        let (key, value) =
            self.parse_mapping_pair(line, rest_start, rest, item_indent, depth + 1)?;
        self.check_duplicate_key(&mut seen, &key)?;
        entries.push((key, value));

        loop {
            self.skip_blanks();
            let Some(next) = self.peek_content() else {
                break;
            };
            if next.indent != item_indent {
                break;
            }
            if sequence_rest(&next.content).is_some() || find_mapping_col(&next.content).is_none() {
                break;
            }
            let next = next.clone();
            self.pos += 1;
            let (key, value) =
                self.parse_mapping_pair(&next, 0, &next.content, item_indent, depth + 1)?;
            self.check_duplicate_key(&mut seen, &key)?;
            entries.push((key, value));
        }

        let span = Span::new(
            line.start + line.indent + rest_start,
            entries
                .last()
                .map(|(_, value)| value.span.end)
                .unwrap_or(line.start + line.indent + rest_start + rest.len()),
            line.no,
            line.indent + rest_start + 1,
        );
        self.emit_mapping_end(span);
        Ok(Node::new(Value::Mapping(entries), span))
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
                line.local_span(value_start, line.content.len()),
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
                line.local_span(value_start, line.content.len()),
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
        let sequence_indent = line.indent + sequence_start;
        self.emit_sequence_start(
            CollectionStyle::Block,
            line.local_span(sequence_start, sequence_start + sequence_text.len()),
        );
        let mut items = Vec::new();
        let first = self.parse_sequence_value_from_line(
            line,
            sequence_start,
            sequence_text,
            sequence_indent,
            depth + 1,
        )?;
        items.push(first);

        loop {
            self.skip_blanks();
            let Some(next) = self.peek_content() else {
                break;
            };
            if next.indent != sequence_indent {
                break;
            }
            let Some((_, _)) = sequence_rest(&next.content) else {
                break;
            };
            let next = next.clone();
            self.pos += 1;
            let item = self.parse_sequence_value_from_line(
                &next,
                0,
                &next.content,
                sequence_indent,
                depth + 1,
            )?;
            items.push(item);
        }

        let end = items
            .last()
            .map(|item| item.span.end)
            .unwrap_or_else(|| line.start + line.indent + sequence_start + sequence_text.len());
        let span = Span::new(
            line.start + line.indent + sequence_start,
            end,
            line.no,
            line.indent + sequence_start + 1,
        );
        self.emit_sequence_end(span);
        Ok(Node::new(Value::Sequence(items), span))
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
        self.skip_blanks();
        if let Some((next, value_rest_start, value_rest)) = self.peek_content().and_then(|next| {
            (next.indent == item_indent)
                .then(|| {
                    explicit_value_rest(&next.content).map(|(value_rest_start, value_rest)| {
                        (next.clone(), value_rest_start, value_rest.to_string())
                    })
                })
                .flatten()
        }) {
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
            line.start + line.indent + marker_start,
            value.span.end,
            line.no,
            line.indent + marker_start + 1,
        );
        self.emit_mapping_end(span);
        Ok(Node::new(Value::Mapping(vec![(key, value)]), span))
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
        let mut seen = HashMap::<DuplicateKey, Span>::new();
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
        Ok(Node::new(Value::Mapping(vec![(key, value)]), span))
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
        self.parse_inline_value(text, line, local_start, line.indent, depth)
    }

    fn parse_mapping_value_or_null(
        &mut self,
        parent_indent: usize,
        span: Span,
        depth: usize,
    ) -> Result<Node> {
        self.check_depth(depth)?;
        self.skip_blanks();
        match self.peek_content() {
            Some(next)
                if next.indent == parent_indent && sequence_rest(&next.content).is_some() =>
            {
                self.parse_sequence(parent_indent, depth)
            }
            Some(next)
                if next.indent > parent_indent
                    && self.empty_node_property_before_indentless_sequence(parent_indent)? =>
            {
                let line = next.clone();
                self.pos += 1;
                self.parse_mapping_value_properties(
                    &line.content,
                    &line,
                    0,
                    parent_indent,
                    depth + 1,
                )?
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
        self.skip_blanks();
        match self.peek_content() {
            Some(next)
                if next.indent == parent_indent && sequence_rest(&next.content).is_some() =>
            {
                self.parse_sequence(parent_indent, depth)
            }
            Some(next)
                if next.indent > parent_indent
                    && self.empty_node_property_before_indentless_sequence(parent_indent)? =>
            {
                let line = next.clone();
                self.pos += 1;
                self.parse_mapping_value_properties_with(
                    &line.content,
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
                if next.indent > parent_indent
                    && sequence_rest(&next.content).is_none()
                    && !starts_mapping_entry(&next.content) =>
            {
                let next = next.clone();
                self.pos += 1;
                if is_plain_scalar_text(&next.content) {
                    self.parse_plain_scalar(&next.content, &next, 0, parent_indent, depth, false)
                } else if let Some(node) = self.parse_mapping_value_properties_with(
                    &next.content,
                    &next,
                    0,
                    parent_indent,
                    depth + 1,
                    properties,
                )? {
                    Ok(node)
                } else {
                    self.parse_inline_value(&next.content, &next, 0, parent_indent, depth)
                }
            }
            Some(next) if next.indent > parent_indent => self.parse_node(depth),
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
        self.skip_blanks();
        match self.peek_content() {
            Some(next)
                if next.indent == parent_indent && sequence_rest(&next.content).is_some() =>
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
        self.skip_blanks();
        match self.peek_content() {
            Some(next)
                if next.indent > parent_indent
                    && sequence_rest(&next.content).is_none()
                    && !starts_mapping_entry(&next.content) =>
            {
                let next = next.clone();
                self.pos += 1;
                if is_plain_scalar_text(&next.content) {
                    self.parse_plain_scalar(&next.content, &next, 0, parent_indent, depth, false)
                } else {
                    self.parse_inline_value(&next.content, &next, 0, parent_indent, depth)
                }
            }
            Some(next) if next.indent > parent_indent => self.parse_node(depth),
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
        let mut out = first_trimmed.to_string();
        let mut end = first_line.start + first_line.indent + first_start + first_trimmed.len();
        let mut continued = false;
        let mut comment_terminated = first_line.had_comment;

        loop {
            if comment_terminated {
                break;
            }
            let mut lookahead = self.pos;
            let mut blank_breaks = 0usize;
            while matches!(
                self.lines.get(lookahead).map(|line| &line.kind),
                Some(LineKind::Blank)
            ) {
                if self.lines[lookahead].had_comment {
                    comment_terminated = true;
                    break;
                }
                lookahead += 1;
                blank_breaks += 1;
            }
            if comment_terminated {
                break;
            }
            let Some(next) = self.lines.get(lookahead).filter(|line| {
                matches!(line.kind, LineKind::Content | LineKind::Directive)
                    && is_plain_scalar_continuation_text(&line.content)
            }) else {
                break;
            };
            if next.indent < parent_indent
                || (next.indent == parent_indent && !allow_same_indent_continuation)
                || (next.indent <= parent_indent && sequence_rest(&next.content).is_some())
                || starts_mapping_entry(&next.content)
            {
                break;
            }
            let next = next.clone();
            self.pos = lookahead + 1;
            continued = true;
            comment_terminated = next.had_comment;
            let trimmed = next.content.trim();
            if !trimmed.is_empty() {
                if blank_breaks > 0 {
                    for _ in 0..blank_breaks {
                        out.push('\n');
                    }
                } else {
                    out.push(' ');
                }
                out.push_str(trimmed);
            }
            end = next.start + next.raw.len();
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

        let node = Node::new(
            Value::String(out),
            Span::new(
                first_line.start + first_line.indent + first_start,
                end,
                first_line.no,
                first_line.indent + first_start + 1,
            ),
        );
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

        while let Some(line) = self.lines.get(self.pos) {
            if matches!(line.kind, LineKind::DocumentStart | LineKind::DocumentEnd) {
                break;
            }
            if line.kind != LineKind::Blank
                && (line.indent < parent_indent
                    || (line.indent == parent_indent && !allow_same_indent_content))
            {
                break;
            }
            if line.kind == LineKind::Blank
                && !line.raw.trim().is_empty()
                && block_indent.is_some_and(|indent| line.indent < indent)
            {
                break;
            }
            let line = line.clone();
            self.pos += 1;
            let text = if line.raw.trim().is_empty() {
                if let Some(tab_offset) = line.raw.bytes().position(|byte| byte == b'\t') {
                    if tab_offset == 0 {
                        return Err(Error::new(
                            "block scalar content cannot start with a tab",
                            Span::point(line.start + tab_offset, line.no, tab_offset + 1),
                        ));
                    }
                    let indent = *block_indent.get_or_insert(tab_offset);
                    if tab_offset < indent {
                        return Err(Error::new(
                            "block scalar content cannot start with a tab",
                            Span::point(line.start + tab_offset, line.no, tab_offset + 1),
                        ));
                    }
                    if max_leading_blank_indent > indent {
                        return Err(Error::new(
                            "block scalar content is less indented than a preceding blank line",
                            line.local_span(0, 0),
                        ));
                    }
                    line.raw_from(indent).to_string()
                } else {
                    if let Some(indent) = block_indent {
                        line.raw_from(indent).to_string()
                    } else {
                        max_leading_blank_indent = max_leading_blank_indent.max(line.raw.len());
                        String::new()
                    }
                }
            } else {
                let indent = *block_indent.get_or_insert(line.indent);
                if max_leading_blank_indent > indent {
                    return Err(Error::new(
                        "block scalar content is less indented than a preceding blank line",
                        line.local_span(0, 0),
                    ));
                }
                if line.indent < indent {
                    break;
                }
                line.raw_from(indent).to_string()
            };
            end = line.start + line.raw.len();
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
        if depth > MAX_DEPTH {
            return Err(Error::new(
                "maximum YAML nesting depth exceeded",
                line.local_span(local_start, local_start + text.len()),
            ));
        }
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
                if properties.allow_document_root_continuation && line.indent == 0 {
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
                if properties.allow_document_root_continuation && line.indent == 0 {
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
            return FlowParser::new(
                buffer,
                depth,
                &mut self.anchors,
                self.events.as_mut(),
                &self.active_tag_handles,
                self.active_schema,
            )
            .parse();
        }
        let node = parse_scalar(text, line, local_start, self.active_schema)?;
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
        let mut text = quoted_line_text(first_line, first_start, first_text, quote);
        let mut end = first_line.start + first_line.indent + first_start + first_text.len();
        let require_indented_continuation =
            !allow_parent_indent_continuation && first_line.indent + first_start > parent_indent;

        while quoted_scalar_accepted_end(&text, quote).is_none() {
            let Some(line) = self.lines.get(self.pos).cloned() else {
                break;
            };
            match line.kind {
                LineKind::Blank => {
                    self.pos += 1;
                    text.push('\n');
                    end = line.start + line.raw.len();
                }
                LineKind::Content if line.indent >= parent_indent => {
                    if require_indented_continuation && line.indent <= parent_indent {
                        if line.content.starts_with('\t') {
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
                    let line_text = quoted_line_text(&line, 0, &line.content, quote);
                    text.push_str(&line_text);
                    end = quoted_scalar_accepted_end(&text, quote)
                        .filter(|close_end| *close_end >= line_text_start)
                        .map(|close_end| {
                            line.start + line.content_start + close_end - line_text_start
                        })
                        .unwrap_or_else(|| line.start + line.content_start + line_text.len());
                }
                LineKind::Content
                | LineKind::Directive
                | LineKind::DocumentStart
                | LineKind::DocumentEnd => break,
            }
        }

        let span = Span::new(
            first_line.start + first_line.indent + first_start,
            end,
            first_line.no,
            first_line.indent + first_start + 1,
        );
        let text_end = quoted_scalar_accepted_end(&text, quote).unwrap_or(text.len());
        let text = fold_flow_quoted_scalar(&text[..text_end], quote);
        let node = parse_scalar_with_span(&text, span)?;
        let style = if quote == '"' {
            ScalarStyle::DoubleQuoted
        } else {
            ScalarStyle::SingleQuoted
        };
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
        let require_continuation_indent =
            (first_line.indent + first_start > parent_indent).then_some(parent_indent);
        while !flow_collection_is_closed(&buffer.text) {
            let Some(line) = self.lines.get(self.pos).cloned() else {
                break;
            };
            match line.kind {
                LineKind::Blank => {
                    let mark =
                        SourceMark::new(line.start + line.raw.len(), line.no, line.raw.len() + 1);
                    buffer.push_virtual_separator(mark);
                    self.pos += 1;
                }
                LineKind::Content | LineKind::Directive => {
                    if require_continuation_indent.is_some_and(|indent| {
                        line.indent <= indent
                            && !flow_continuation_may_start_at_parent_indent_after_ws(&line.content)
                    }) {
                        return Err(Error::new(
                            "flow collection continuation is not sufficiently indented",
                            line.span(),
                        ));
                    }
                    buffer.push_virtual_separator(SourceMark::for_line_content(&line, 0));
                    buffer.push_source_text(&line.content, &line, 0);
                    self.pos += 1;
                }
                LineKind::DocumentStart | LineKind::DocumentEnd => break,
            }
        }
        Ok(buffer)
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            let span = self
                .peek_content()
                .map(Line::span)
                .unwrap_or_else(|| Span::point(self.input_len, 1, self.input_len + 1));
            return Err(Error::new("maximum YAML nesting depth exceeded", span));
        }
        Ok(())
    }

    fn current_content(&mut self) -> Result<&Line> {
        self.skip_blanks();
        self.peek_content().ok_or_else(|| {
            Error::new(
                "expected YAML content",
                Span::point(self.input_len, 1, self.input_len + 1),
            )
        })
    }

    fn peek_content(&self) -> Option<&Line> {
        match self.lines.get(self.pos) {
            Some(line) if line.kind == LineKind::Content => Some(line),
            _ => None,
        }
    }

    fn peek_content_from(&self, mut pos: usize) -> Option<&Line> {
        loop {
            match self.lines.get(pos) {
                Some(line) if line.kind == LineKind::Blank => pos += 1,
                Some(line) if line.kind == LineKind::Content => return Some(line),
                _ => return None,
            }
        }
    }

    fn empty_node_property_before_indentless_sequence(&self, parent_indent: usize) -> Result<bool> {
        let Some(line) = self.peek_content() else {
            return Ok(false);
        };
        if !empty_node_property_line(line)? {
            return Ok(false);
        }
        Ok(matches!(
            self.peek_content_from(self.pos + 1),
            Some(next) if next.indent == parent_indent && sequence_rest(&next.content).is_some()
        ))
    }

    fn skip_blanks(&mut self) {
        while matches!(
            self.lines.get(self.pos).map(|line| &line.kind),
            Some(LineKind::Blank)
        ) {
            self.pos += 1;
        }
    }
}

fn block_scalar_line_is_more_indented(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

fn preprocess(input: &str) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (idx, chunk) in input.split_inclusive('\n').enumerate() {
        let raw = chunk
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string();
        push_preprocessed_line(&mut out, idx + 1, offset, raw)?;
        offset += chunk.len();
    }
    if !input.is_empty() && !input.ends_with('\n') {
        // The last line was already yielded by split_inclusive.
    } else if input.is_empty() {
        return Ok(out);
    }
    Ok(out)
}

fn push_preprocessed_line(out: &mut Vec<Line>, no: usize, start: usize, raw: String) -> Result<()> {
    let bom_len = if start == 0 && raw.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let raw_body = &raw[bom_len..];
    let raw_indent = raw_body.bytes().take_while(|byte| *byte == b' ').count();
    if raw_body.trim().is_empty() {
        out.push(Line {
            no,
            start,
            raw,
            indent: raw_indent,
            content_start: bom_len + raw_indent,
            content: String::new(),
            kind: LineKind::Blank,
            had_comment: false,
        });
        return Ok(());
    }
    let comment = comment_start(raw_body).map(|idx| bom_len + idx);
    let end = comment.unwrap_or(raw.len());
    let had_comment = comment.is_some();
    let no_comment = raw[bom_len..end].trim_end();
    let indent = no_comment.bytes().take_while(|byte| *byte == b' ').count();
    let content_start = bom_len + indent;
    let content = no_comment[indent..].to_string();
    if let Some(tab_offset) =
        block_indicator_tab_separation_offset(&no_comment.as_bytes()[indent..])
    {
        return Err(Error::new(
            "tabs are not allowed as separation after block indicators",
            Span::point(
                start + content_start + tab_offset,
                no,
                content_start + tab_offset + 1,
            ),
        ));
    }
    if content.trim().is_empty() {
        out.push(Line {
            no,
            start,
            raw,
            indent: raw_indent,
            content_start: bom_len + raw_indent,
            content: String::new(),
            kind: LineKind::Blank,
            had_comment,
        });
        return Ok(());
    }
    let kind = match content.as_str() {
        "---" => LineKind::DocumentStart,
        "..." => LineKind::DocumentEnd,
        _ if document_start_rest(&content).is_some() => LineKind::DocumentStart,
        _ if content.starts_with("... ") => {
            return Err(Error::new(
                "document end markers cannot have trailing content",
                Span::new(
                    start + content_start,
                    start + content_start + content.len(),
                    no,
                    content_start + 1,
                ),
            ));
        }
        _ if content.starts_with('%') => LineKind::Directive,
        _ => LineKind::Content,
    };
    out.push(Line {
        no,
        start,
        raw,
        indent,
        content_start,
        content,
        kind,
        had_comment,
    });
    Ok(())
}

pub(crate) fn comment_start(raw: &str) -> Option<usize> {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if double && escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if double => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            '#' if !single
                && !double
                && (idx == 0 || raw[..idx].chars().last().is_some_and(char::is_whitespace)) =>
            {
                return Some(idx);
            }
            _ => {}
        }
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

fn directive_fields(line: &Line) -> Vec<DirectiveField<'_>> {
    let mut fields = Vec::new();
    let mut start = None;
    for (idx, ch) in line.content.char_indices() {
        if ch == '#' && start.is_none() {
            break;
        } else if ch.is_whitespace() {
            if let Some(field_start) = start.take() {
                fields.push(DirectiveField {
                    text: &line.content[field_start..idx],
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
            text: &line.content[field_start..],
            start: field_start,
            end: line.content.len(),
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
        Schema::Yaml11 => Schema::Yaml11,
        Schema::Yaml12 | Schema::YamlVersionDirective => Schema::Yaml12,
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
            Schema::Yaml11
        }
        Schema::YamlVersionDirective => Schema::Yaml12,
        Schema::Yaml11 => Schema::Yaml11,
        Schema::Yaml12 => Schema::Yaml12,
    }
}

fn merge_policy_for_schema(schema: Schema) -> MergePolicy {
    match schema {
        Schema::Yaml11 => MergePolicy::Yaml11Compatible,
        Schema::Yaml12 | Schema::YamlVersionDirective => MergePolicy::Strict,
    }
}

fn check_duplicate_for_schema(
    recording_events: bool,
    schema: Schema,
    seen: &mut HashMap<DuplicateKey, Span>,
    key: &Node,
) -> Result<()> {
    if recording_events || (schema == Schema::Yaml11 && node_is_merge_key(key)) {
        return Ok(());
    }
    check_duplicate(seen, key)
}

fn node_is_merge_key(key: &Node) -> bool {
    match &key.value {
        Value::String(value) => value == "<<",
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

fn parse_scalar(text: &str, line: &Line, local_start: usize, schema: Schema) -> Result<Node> {
    let span = line.local_span(local_start, local_start + text.len());
    parse_scalar_with_schema(text, span, schema)
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

fn empty_node_property_line(line: &Line) -> Result<bool> {
    let text = line.content.trim();
    let local_start = line.content.len() - line.content.trim_start().len();
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
        Value::String(value) => value.clone(),
        Value::Tagged(tagged) => event_scalar_value(&tagged.value),
        Value::Sequence(_) | Value::Mapping(_) => String::new(),
    }
}

fn parse_scalar_with_span(text: &str, span: Span) -> Result<Node> {
    parse_scalar_with_schema(text, span, Schema::Yaml12)
}

fn parse_scalar_with_schema(text: &str, span: Span, schema: Schema) -> Result<Node> {
    if text.is_empty() || text == "~" || text.eq_ignore_ascii_case("null") {
        return Ok(Node::new(Value::Null, span).with_scalar_source(text));
    }
    if schema == Schema::Yaml11
        && let Some(value) = parse_yaml11_bool(text)
    {
        return Ok(Node::new(Value::Bool(value), span).with_scalar_source(text));
    }
    if text == "true" || text == "True" || text == "TRUE" {
        return Ok(Node::new(Value::Bool(true), span).with_scalar_source(text));
    }
    if text == "false" || text == "False" || text == "FALSE" {
        return Ok(Node::new(Value::Bool(false), span).with_scalar_source(text));
    }
    if text.starts_with('"') {
        return parse_double_quoted(text, span);
    }
    if text.starts_with('\'') {
        return parse_single_quoted(text, span);
    }
    if schema == Schema::Yaml11 && is_yaml11_timestamp(text) {
        return Ok(yaml11_timestamp_node(text, span));
    }
    if schema == Schema::Yaml11
        && let Some(number) = parse_yaml11_number(text)?
    {
        return Ok(Node::new(Value::Number(number), span).with_scalar_source(text));
    }
    if schema == Schema::Yaml11 && is_yaml11_invalid_octal(text) {
        return Ok(Node::new(Value::String(text.to_string()), span).with_scalar_source(text));
    }
    if is_int_like(text) {
        if let Some(number) = parse_number(text, span)? {
            return Ok(Node::new(Value::Number(number), span).with_scalar_source(text));
        }
        return Ok(Node::new(Value::String(text.to_string()), span).with_scalar_source(text));
    }
    if let Some(number) = parse_number(text, span)? {
        return Ok(Node::new(Value::Number(number), span).with_scalar_source(text));
    }
    Ok(Node::new(Value::String(text.to_string()), span))
}

fn parse_yaml11_bool(text: &str) -> Option<bool> {
    yaml11::parse_bool(text)
}

fn yaml11_timestamp_node(text: &str, span: Span) -> Node {
    let value = Node::new(Value::String(text.to_string()), span).with_scalar_source(text);
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

fn parse_single_quoted(text: &str, span: Span) -> Result<Node> {
    if !text.ends_with('\'') || text.len() < 2 {
        return Err(Error::new("unterminated single-quoted scalar", span));
    }
    let inner = &text[1..text.len() - 1];
    Ok(Node::new(Value::String(inner.replace("''", "'")), span))
}

fn parse_double_quoted(text: &str, span: Span) -> Result<Node> {
    if !text.ends_with('"') || text.len() < 2 {
        return Err(Error::new("unterminated double-quoted scalar", span));
    }
    let mut out = String::new();
    let mut chars = text[1..text.len() - 1].chars();
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

fn parse_number(text: &str, _span: Span) -> Result<Option<Number>> {
    let compact = text.replace('_', "");
    if let Some(number) = parse_special_float(&compact) {
        return Ok(Some(number));
    }
    if is_int_like(text) {
        if compact.starts_with('-') {
            return match compact.parse::<i128>() {
                Ok(value) => Ok(Some(Number::Integer(value))),
                Err(_) => Ok(None),
            };
        }
        if let Some(positive) = compact.strip_prefix('+') {
            return parse_positive_integer_number(positive);
        }
        return parse_positive_integer_number(&compact);
    }
    if is_float_like(text) {
        match compact.parse::<f64>() {
            Ok(value) => return Ok(Some(Number::Float(value))),
            Err(_) => return Ok(None),
        }
    }
    Ok(None)
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

fn parse_positive_integer_number(compact: &str) -> Result<Option<Number>> {
    match compact.parse::<i64>() {
        Ok(value) => Ok(Some(Number::Integer(i128::from(value)))),
        Err(_) => match compact.parse::<u64>() {
            Ok(value) => Ok(Some(Number::Unsigned(u128::from(value)))),
            Err(_) => match compact.parse::<i128>() {
                Ok(value) => Ok(Some(Number::Integer(value))),
                Err(_) => compact
                    .parse::<u128>()
                    .map(Number::Unsigned)
                    .map(Some)
                    .or(Ok(None)),
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
            line.start + line.indent + local_start,
            line.no,
            line.indent + local_start + 1,
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
                line.no,
                line.indent + local_start + offset + 1,
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
    pending_anchor: Option<PendingAnchor>,
}

impl<'a> FlowParser<'a> {
    fn new(
        buffer: FlowBuffer,
        depth: usize,
        anchors: &'a mut AnchorRegistry,
        events: Option<&'a mut EventRecorder>,
        active_tag_handles: &'a HashMap<String, String>,
        schema: Schema,
    ) -> Self {
        Self {
            buffer,
            pos: 0,
            depth,
            schema,
            anchors: Some(anchors),
            events,
            active_tag_handles,
            pending_anchor: None,
        }
    }

    fn parse(mut self) -> Result<Node> {
        let node = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.buffer.text.len() {
            return Err(Error::new(
                "unexpected trailing characters in flow value",
                self.span(self.pos, self.buffer.text.len()),
            ));
        }
        Ok(node)
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

    fn check_duplicate_key(
        &self,
        seen: &mut HashMap<DuplicateKey, Span>,
        key: &Node,
    ) -> Result<()> {
        check_duplicate_for_schema(self.recording_events(), self.schema, seen, key)
    }

    fn resolve_tag(&self, tag: Tag, span: Span) -> Result<Tag> {
        resolve_tag(self.active_tag_handles, tag, span)
    }

    fn parse_value(&mut self) -> Result<Node> {
        if self.depth > MAX_DEPTH {
            return Err(Error::new(
                "maximum YAML nesting depth exceeded",
                self.span(self.pos, self.pos),
            ));
        }
        self.skip_ws();
        match self.peek() {
            Some('[') => self.parse_sequence(),
            Some('{') => self.parse_mapping(),
            Some('"') => {
                let (text, start, end) = self.take_quoted('"')?;
                let node = parse_double_quoted(&text, self.span(start, end))?;
                self.emit_scalar_node(&node, ScalarStyle::DoubleQuoted);
                Ok(node)
            }
            Some('\'') => {
                let (text, start, end) = self.take_quoted('\'')?;
                let node = parse_single_quoted(&text, self.span(start, end))?;
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
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.consume(']') {
                let span = self.span(start, self.pos);
                self.emit_sequence_end(span);
                return Ok(Node::new(Value::Sequence(items), span));
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
                self.skip_ws();
                if self.consume(']') {
                    let span = self.span(start, self.pos);
                    self.emit_sequence_end(span);
                    return Ok(Node::new(Value::Sequence(items), span));
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
            self.expect(']')?;
            let span = self.span(start, self.pos);
            self.emit_sequence_end(span);
            return Ok(Node::new(Value::Sequence(items), span));
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
        let mut entries = Vec::new();
        let mut seen = HashMap::<DuplicateKey, Span>::new();
        loop {
            self.skip_ws();
            if self.consume('}') {
                let span = self.span(start, self.pos);
                self.emit_mapping_end(span);
                return Ok(Node::new(Value::Mapping(entries), span));
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
            self.skip_ws();
            if self.consume(',') {
                self.skip_ws();
                if self.consume('}') {
                    let span = self.span(start, self.pos);
                    self.emit_mapping_end(span);
                    return Ok(Node::new(Value::Mapping(entries), span));
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
            return Ok(Node::new(Value::Mapping(entries), span));
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
        Ok(Node::new(Value::Mapping(vec![(key, value)]), span))
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
                let node = parse_double_quoted(&text, self.span(start, end))?;
                self.emit_scalar_node(&node, ScalarStyle::DoubleQuoted);
                Ok(node)
            }
            Some('\'') => {
                let (text, start, end) = self.take_quoted('\'')?;
                let node = parse_single_quoted(&text, self.span(start, end))?;
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
            return Err(Error::new(
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
            return Err(Error::new(
                "expected `,` between flow mapping entries",
                self.span(colon, colon + ':'.len_utf8()),
            ));
        }
        let leading_trim = raw.len() - raw.trim_start().len();
        let scalar_text = raw.trim();
        if scalar_text.starts_with('#') {
            return Err(Error::new(
                "comments must be separated from other tokens by whitespace",
                self.span(start + leading_trim, start + leading_trim + '#'.len_utf8()),
            ));
        }
        if scalar_text == "-" {
            return Err(Error::new(
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
