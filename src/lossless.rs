//! Source-backed lossless YAML stream and graph identity API.
//!
//! This module is intentionally separate from the semantic [`crate::Node`] and
//! [`crate::Value`] loaders. The semantic loaders expand aliases and discard
//! comments because that is the useful shape for config reads. The lossless
//! stream keeps the original source text, exposes comments and blank lines as
//! trivia, and builds a graph view where aliases reference stable anchor ids.

use crate::{
    CollectionStyle, Error, Event, EventAnchor, EventDocumentDirectives, EventMeta, EventTag,
    Result, ScalarStyle, Span,
    error::utf8_error_span,
    parse::{comment_start, parse_events},
};
use std::collections::HashMap;
use std::fmt;

/// Parses a YAML stream into a source-backed lossless graph view.
pub fn parse_lossless(input: &str) -> Result<LosslessStream> {
    LosslessStream::parse(input)
}

/// Parses UTF-8 YAML bytes into a source-backed lossless graph view.
pub fn parse_lossless_bytes(input: &[u8]) -> Result<LosslessStream> {
    match std::str::from_utf8(input) {
        Ok(input) => parse_lossless(input),
        Err(err) => Err(Error::new(
            "input is not valid UTF-8",
            utf8_error_span(input, err),
        )),
    }
}

/// Stable node identifier inside a [`LosslessStream`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

impl NodeId {
    /// Returns the zero-based node index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// Stable anchor identifier inside a [`LosslessStream`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnchorId(usize);

impl AnchorId {
    /// Returns the zero-based anchor index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// Stable alias identifier inside a [`LosslessStream`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AliasId(usize);

impl AliasId {
    /// Returns the zero-based alias index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// YAML stream that keeps the original source and a graph-shaped node view.
#[derive(Clone, Debug, PartialEq)]
pub struct LosslessStream {
    source: String,
    documents: Vec<LosslessDocument>,
    nodes: Vec<LosslessNode>,
    anchors: Vec<LosslessAnchor>,
    aliases: Vec<LosslessAlias>,
    trivia: Vec<LosslessTrivia>,
}

impl LosslessStream {
    /// Parses a YAML stream into a source-backed lossless graph view.
    pub fn parse(input: &str) -> Result<Self> {
        let events = parse_events(input)?;
        let trivia = scan_trivia(input);
        Builder::new(input, events, trivia).build()
    }

    /// Returns the original YAML source.
    pub fn as_source(&self) -> &str {
        &self.source
    }

    /// Consumes the stream and returns the original YAML source.
    pub fn into_source(self) -> String {
        self.source
    }

    /// Returns a source fragment for a span if the span still points into the
    /// retained source.
    pub fn source_fragment(&self, span: Span) -> Option<&str> {
        if span.start <= span.end && span.end <= self.source.len() {
            self.source.get(span.start..span.end)
        } else {
            None
        }
    }

    /// Builds a source span from byte bounds in the retained YAML source.
    ///
    /// This is useful with [`LosslessEdit::replace_source_span`] and
    /// [`LosslessEdit::delete_source_span`] when a tool needs to edit raw YAML
    /// punctuation, mapping entries, sequence items, or surrounding whitespace
    /// that is not represented as a single graph node.
    pub fn source_span(&self, start: usize, end: usize) -> Result<Span> {
        span_for_source_range(&self.source, start, end)
    }

    /// Returns parsed documents in source order.
    pub fn documents(&self) -> &[LosslessDocument] {
        &self.documents
    }

    /// Returns graph nodes in allocation order.
    pub fn nodes(&self) -> &[LosslessNode] {
        &self.nodes
    }

    /// Looks up a graph node by id.
    pub fn node(&self, id: NodeId) -> Option<&LosslessNode> {
        self.nodes.get(id.0)
    }

    /// Returns anchor definitions in source order.
    pub fn anchors(&self) -> &[LosslessAnchor] {
        &self.anchors
    }

    /// Looks up an anchor definition by id.
    pub fn anchor(&self, id: AnchorId) -> Option<&LosslessAnchor> {
        self.anchors.get(id.0)
    }

    /// Returns alias references in source order.
    pub fn aliases(&self) -> &[LosslessAlias] {
        &self.aliases
    }

    /// Looks up an alias reference by id.
    pub fn alias(&self, id: AliasId) -> Option<&LosslessAlias> {
        self.aliases.get(id.0)
    }

    /// Returns comments and blank-line trivia found in the original source.
    pub fn trivia(&self) -> &[LosslessTrivia] {
        &self.trivia
    }

    /// Returns only comment trivia found in the original source.
    pub fn comments(&self) -> impl Iterator<Item = &LosslessTrivia> {
        self.trivia
            .iter()
            .filter(|trivia| trivia.kind == LosslessTriviaKind::Comment)
    }

    /// Starts a source-preserving edit session for this stream.
    ///
    /// Edits replace explicit source spans and keep every untouched byte
    /// unchanged. [`LosslessEdit::finish`] validates the final YAML before it
    /// returns the edited source.
    pub fn edit(&self) -> LosslessEdit<'_> {
        LosslessEdit {
            stream: self,
            replacements: Vec::new(),
        }
    }

    /// Replaces one graph node's source span and returns validated edited YAML.
    ///
    /// This is a convenience wrapper around [`LosslessEdit`]. The replacement
    /// is raw YAML source for the selected node range.
    pub fn replace_node_source(
        &self,
        node: NodeId,
        replacement: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.replace_node_source(node, replacement)?;
        edit.finish()
    }

    /// Replaces one raw source span and returns validated edited YAML.
    ///
    /// This is a convenience wrapper around [`LosslessEdit`]. Use
    /// [`Self::source_span`] to build spans from byte ranges in the retained
    /// source.
    pub fn replace_source_span(
        &self,
        span: Span,
        replacement: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.replace_source_span(span, replacement)?;
        edit.finish()
    }

    /// Inserts raw YAML source at a byte offset and returns validated edited YAML.
    pub fn insert_source(&self, offset: usize, insertion: impl Into<String>) -> Result<String> {
        let mut edit = self.edit();
        edit.insert_source(offset, insertion)?;
        edit.finish()
    }

    /// Deletes one raw source span and returns validated edited YAML.
    pub fn delete_source_span(&self, span: Span) -> Result<String> {
        let mut edit = self.edit();
        edit.delete_source_span(span)?;
        edit.finish()
    }
}

impl fmt::Display for LosslessStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

/// Source-preserving edit builder for a [`LosslessStream`].
///
/// The builder is intentionally low-level: replacement text is YAML source, not
/// a semantic [`crate::Value`]. This keeps it useful for format-sensitive tools
/// while avoiding a second, incompatible value model.
#[derive(Debug)]
pub struct LosslessEdit<'a> {
    stream: &'a LosslessStream,
    replacements: Vec<LosslessReplacement>,
}

impl LosslessEdit<'_> {
    /// Replaces one node's retained source range with raw YAML source.
    ///
    /// The edited full document is parsed again by [`Self::finish`]. Overlapping
    /// replacements are rejected.
    pub fn replace_node_source(
        &mut self,
        node: NodeId,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        let span = self.checked_node_span(node)?;
        self.push_replacement(span, replacement)
    }

    /// Replaces a scalar node with source that parses as one scalar node.
    ///
    /// Use [`Self::replace_node_source`] when intentionally replacing a scalar
    /// with another YAML node kind.
    pub fn replace_scalar_source(
        &mut self,
        node: NodeId,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        let current = self
            .stream
            .node(node)
            .ok_or_else(|| Error::new("lossless node id is out of bounds", None))?;
        if !matches!(current.kind(), LosslessNodeKind::Scalar { .. }) {
            return Err(Error::new(
                "lossless replacement target is not a scalar",
                Some(current.span()),
            ));
        }
        let replacement = replacement.into();
        ensure_scalar_fragment(&replacement, current.span())?;
        self.replace_node_source(node, replacement)
    }

    /// Replaces a raw source span with raw YAML source.
    ///
    /// The span must point into the retained source. Use
    /// [`LosslessStream::source_span`] to build a span from byte bounds when
    /// editing mapping entries, sequence items, separators, comments, or
    /// whitespace outside a single graph node.
    pub fn replace_source_span(
        &mut self,
        span: Span,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        let span = self.checked_source_span(span)?;
        self.push_replacement(span, replacement)
    }

    /// Inserts raw YAML source at a byte offset in the retained source.
    ///
    /// The final edited document is still validated by [`Self::finish`].
    pub fn insert_source(
        &mut self,
        offset: usize,
        insertion: impl Into<String>,
    ) -> Result<&mut Self> {
        let span = self.stream.source_span(offset, offset)?;
        self.push_replacement(span, insertion)
    }

    /// Deletes a raw source span from the retained source.
    ///
    /// This is equivalent to replacing the span with an empty string, followed
    /// by full YAML validation in [`Self::finish`].
    pub fn delete_source_span(&mut self, span: Span) -> Result<&mut Self> {
        self.replace_source_span(span, "")
    }

    /// Returns validated edited YAML with untouched source bytes preserved.
    pub fn finish(mut self) -> Result<String> {
        self.replacements
            .sort_by_key(|replacement| (replacement.start, replacement.end, replacement.order));
        self.validate_replacements()?;

        let mut output = String::with_capacity(self.edited_capacity());
        let mut cursor = 0usize;
        for replacement in &self.replacements {
            let Some(prefix) = self.stream.source.get(cursor..replacement.start) else {
                return Err(Error::new(
                    "lossless replacement span is not on a UTF-8 boundary",
                    Some(replacement.span),
                ));
            };
            output.push_str(prefix);
            output.push_str(&replacement.replacement);
            cursor = replacement.end;
        }
        let Some(suffix) = self.stream.source.get(cursor..) else {
            return Err(Error::new(
                "lossless replacement span is not on a UTF-8 boundary",
                None,
            ));
        };
        output.push_str(suffix);

        parse_lossless(&output)?;
        Ok(output)
    }

    fn push_replacement(
        &mut self,
        span: Span,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        let order = self.replacements.len();
        self.replacements.push(LosslessReplacement {
            order,
            start: span.start,
            end: span.end,
            span,
            replacement: replacement.into(),
        });
        Ok(self)
    }

    fn checked_node_span(&self, node: NodeId) -> Result<Span> {
        let node = self
            .stream
            .node(node)
            .ok_or_else(|| Error::new("lossless node id is out of bounds", None))?;
        let span = node.span();
        if span.start > span.end || span.end > self.stream.source.len() {
            return Err(Error::new(
                "lossless node span is outside the retained source",
                Some(span),
            ));
        }
        if self.stream.source.get(span.start..span.end).is_none() {
            return Err(Error::new(
                "lossless node span is not on a UTF-8 boundary",
                Some(span),
            ));
        }
        Ok(span)
    }

    fn checked_source_span(&self, span: Span) -> Result<Span> {
        if span.start > span.end || span.end > self.stream.source.len() {
            return Err(Error::new(
                "lossless source span is outside the retained source",
                Some(span),
            ));
        }
        if self.stream.source.get(span.start..span.end).is_none() {
            return Err(Error::new(
                "lossless source span is not on a UTF-8 boundary",
                Some(span),
            ));
        }
        Ok(span)
    }

    fn validate_replacements(&self) -> Result<()> {
        let mut cursor = 0usize;
        for replacement in &self.replacements {
            if replacement.start < cursor {
                return Err(Error::new(
                    "lossless replacements overlap",
                    Some(replacement.span),
                ));
            }
            if self.stream.source.get(cursor..replacement.start).is_none() {
                return Err(Error::new(
                    "lossless replacement span is not on a UTF-8 boundary",
                    Some(replacement.span),
                ));
            }
            cursor = replacement.end;
        }
        if self.stream.source.get(cursor..).is_none() {
            return Err(Error::new(
                "lossless replacement span is not on a UTF-8 boundary",
                None,
            ));
        }
        Ok(())
    }

    fn edited_capacity(&self) -> usize {
        let removed = self
            .replacements
            .iter()
            .map(|replacement| replacement.end - replacement.start)
            .sum::<usize>();
        let added = self
            .replacements
            .iter()
            .map(|replacement| replacement.replacement.len())
            .sum::<usize>();
        self.stream.source.len() - removed + added
    }
}

#[derive(Clone, Debug)]
struct LosslessReplacement {
    order: usize,
    start: usize,
    end: usize,
    span: Span,
    replacement: String,
}

/// One YAML document in a [`LosslessStream`].
#[derive(Clone, Debug, PartialEq)]
pub struct LosslessDocument {
    index: usize,
    explicit_start: bool,
    explicit_end: bool,
    directives: EventDocumentDirectives,
    start_span: Span,
    end_span: Span,
    root: Option<NodeId>,
}

impl LosslessDocument {
    /// Returns the zero-based document index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns whether the document had an explicit `---` start marker.
    pub fn explicit_start(&self) -> bool {
        self.explicit_start
    }

    /// Returns whether the document had an explicit `...` end marker.
    pub fn explicit_end(&self) -> bool {
        self.explicit_end
    }

    /// Returns directive metadata active for this document.
    pub fn directives(&self) -> &EventDocumentDirectives {
        &self.directives
    }

    /// Returns the start-event span for this document.
    pub fn start_span(&self) -> Span {
        self.start_span
    }

    /// Returns the end-event span for this document.
    pub fn end_span(&self) -> Span {
        self.end_span
    }

    /// Returns the root node id, if the document contains a node.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }
}

/// One graph node in a [`LosslessStream`].
#[derive(Clone, Debug, PartialEq)]
pub struct LosslessNode {
    id: NodeId,
    span: Span,
    anchor: Option<AnchorId>,
    tag: Option<EventTag>,
    kind: LosslessNodeKind,
}

impl LosslessNode {
    /// Returns this node's stable id.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the node source span.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the anchor definition attached to this node, if any.
    pub fn anchor(&self) -> Option<AnchorId> {
        self.anchor
    }

    /// Returns the tag attached to this node, if any.
    pub fn tag(&self) -> Option<&EventTag> {
        self.tag.as_ref()
    }

    /// Returns this node's graph payload.
    pub fn kind(&self) -> &LosslessNodeKind {
        &self.kind
    }
}

/// Graph node payload for the lossless source-backed API.
#[derive(Clone, Debug, PartialEq)]
pub enum LosslessNodeKind {
    /// Scalar node with resolved text plus original scalar style.
    Scalar {
        /// Resolved scalar text from the parser event stream.
        value: String,
        /// Parser-observed scalar style.
        style: ScalarStyle,
    },
    /// Sequence node with source child node ids.
    Sequence {
        /// Block or flow sequence style.
        style: CollectionStyle,
        /// Child node ids in source order.
        children: Vec<NodeId>,
    },
    /// Mapping node with source key/value node ids.
    Mapping {
        /// Block or flow mapping style.
        style: CollectionStyle,
        /// Key/value node ids in source order.
        entries: Vec<(NodeId, NodeId)>,
    },
    /// Alias reference node.
    Alias {
        /// Anchor name used by the alias.
        name: String,
        /// Alias reference id.
        alias: AliasId,
        /// Anchor definition targeted by this alias at this source position.
        target: AnchorId,
    },
}

/// Anchor definition attached to a lossless graph node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LosslessAnchor {
    id: AnchorId,
    name: String,
    span: Span,
    node: NodeId,
}

impl LosslessAnchor {
    /// Returns this anchor's stable id.
    pub fn id(&self) -> AnchorId {
        self.id
    }

    /// Returns the anchor name without the leading `&`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the source span for the anchor token name.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the node this anchor defines.
    pub fn node(&self) -> NodeId {
        self.node
    }
}

/// Alias reference in the lossless graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LosslessAlias {
    id: AliasId,
    name: String,
    span: Span,
    node: NodeId,
    target: AnchorId,
}

impl LosslessAlias {
    /// Returns this alias's stable id.
    pub fn id(&self) -> AliasId {
        self.id
    }

    /// Returns the alias name without the leading `*`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the source span for the alias token name.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the graph node representing this alias occurrence.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Returns the anchor definition targeted at this source position.
    pub fn target(&self) -> AnchorId {
        self.target
    }
}

/// Trivia kind retained outside the semantic YAML tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LosslessTriviaKind {
    /// A YAML comment beginning with `#`.
    Comment,
    /// A blank or whitespace-only source line.
    BlankLine,
}

/// Comment or blank-line trivia in the retained source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LosslessTrivia {
    kind: LosslessTriviaKind,
    span: Span,
    text: String,
}

impl LosslessTrivia {
    /// Returns the trivia kind.
    pub fn kind(&self) -> LosslessTriviaKind {
        self.kind
    }

    /// Returns the source span for this trivia.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the trivia source text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

struct Builder {
    source: String,
    events: Vec<Event>,
    documents: Vec<LosslessDocument>,
    nodes: Vec<LosslessNode>,
    anchors: Vec<LosslessAnchor>,
    aliases: Vec<LosslessAlias>,
    trivia: Vec<LosslessTrivia>,
    active_anchors: HashMap<String, AnchorId>,
    current_document: Option<DocumentBuilder>,
    stack: Vec<Frame>,
}

impl Builder {
    fn new(input: &str, events: Vec<Event>, trivia: Vec<LosslessTrivia>) -> Self {
        Self {
            source: input.to_string(),
            events,
            documents: Vec::new(),
            nodes: Vec::new(),
            anchors: Vec::new(),
            aliases: Vec::new(),
            trivia,
            active_anchors: HashMap::new(),
            current_document: None,
            stack: Vec::new(),
        }
    }

    fn build(mut self) -> Result<LosslessStream> {
        let events = std::mem::take(&mut self.events);
        for event in events {
            match event {
                Event::StreamStart | Event::StreamEnd => {}
                Event::DocumentStart {
                    explicit,
                    directives,
                    span,
                } => self.start_document(explicit, directives, span),
                Event::DocumentEnd { explicit, span } => self.end_document(explicit, span)?,
                Event::SequenceStart { meta, style, span } => {
                    let id =
                        self.push_collection_node(meta, span, |id, anchor, tag| LosslessNode {
                            id,
                            span,
                            anchor,
                            tag,
                            kind: LosslessNodeKind::Sequence {
                                style,
                                children: Vec::new(),
                            },
                        })?;
                    self.stack.push(Frame::Sequence { node: id, span });
                }
                Event::SequenceEnd { span } => self.end_collection(span, "sequence")?,
                Event::MappingStart { meta, style, span } => {
                    let id =
                        self.push_collection_node(meta, span, |id, anchor, tag| LosslessNode {
                            id,
                            span,
                            anchor,
                            tag,
                            kind: LosslessNodeKind::Mapping {
                                style,
                                entries: Vec::new(),
                            },
                        })?;
                    self.stack.push(Frame::Mapping {
                        node: id,
                        span,
                        pending_key: None,
                    });
                }
                Event::MappingEnd { span } => self.end_collection(span, "mapping")?,
                Event::Alias { anchor } => self.push_alias_node(anchor)?,
                Event::Scalar {
                    value,
                    style,
                    meta,
                    span,
                } => self.push_scalar_node(value, style, meta, span)?,
            }
        }
        if !self.stack.is_empty() {
            return Err(Error::new(
                "unclosed collection in lossless event stream",
                self.stack.last().map(|frame| frame.span()),
            ));
        }
        if let Some(document) = self.current_document.take() {
            let end_span = document.start_span;
            self.documents.push(document.finish(false, end_span));
        }
        Ok(LosslessStream {
            source: self.source,
            documents: self.documents,
            nodes: self.nodes,
            anchors: self.anchors,
            aliases: self.aliases,
            trivia: self.trivia,
        })
    }

    fn start_document(&mut self, explicit: bool, directives: EventDocumentDirectives, span: Span) {
        self.active_anchors.clear();
        self.current_document = Some(DocumentBuilder {
            index: self.documents.len(),
            explicit_start: explicit,
            directives,
            start_span: span,
            root: None,
        });
    }

    fn end_document(&mut self, explicit: bool, span: Span) -> Result<()> {
        if !self.stack.is_empty() {
            return Err(Error::new(
                "document ended before collection closed",
                self.stack.last().map(|frame| frame.span()),
            ));
        }
        let Some(document) = self.current_document.take() else {
            return Err(Error::new(
                "document end without document start",
                Some(span),
            ));
        };
        self.documents.push(document.finish(explicit, span));
        Ok(())
    }

    fn push_collection_node(
        &mut self,
        meta: EventMeta,
        span: Span,
        make_node: impl FnOnce(NodeId, Option<AnchorId>, Option<EventTag>) -> LosslessNode,
    ) -> Result<NodeId> {
        let id = NodeId(self.nodes.len());
        let anchor = self.register_anchor(id, meta.anchor);
        self.nodes.push(make_node(id, anchor, meta.tag));
        self.attach_node(id, span)?;
        Ok(id)
    }

    fn push_scalar_node(
        &mut self,
        value: String,
        style: ScalarStyle,
        meta: EventMeta,
        span: Span,
    ) -> Result<()> {
        let id = NodeId(self.nodes.len());
        let anchor = self.register_anchor(id, meta.anchor);
        self.nodes.push(LosslessNode {
            id,
            span,
            anchor,
            tag: meta.tag,
            kind: LosslessNodeKind::Scalar { value, style },
        });
        self.attach_node(id, span)
    }

    fn push_alias_node(&mut self, anchor: EventAnchor) -> Result<()> {
        let Some(target) = self.active_anchors.get(&anchor.name).copied() else {
            return Err(Error::new(
                format!("unknown anchor `{}`", anchor.name),
                anchor.span,
            ));
        };
        let node = NodeId(self.nodes.len());
        let alias = AliasId(self.aliases.len());
        self.aliases.push(LosslessAlias {
            id: alias,
            name: anchor.name.clone(),
            span: anchor.span,
            node,
            target,
        });
        self.nodes.push(LosslessNode {
            id: node,
            span: anchor.span,
            anchor: None,
            tag: None,
            kind: LosslessNodeKind::Alias {
                name: anchor.name,
                alias,
                target,
            },
        });
        self.attach_node(node, anchor.span)
    }

    fn register_anchor(&mut self, node: NodeId, anchor: Option<EventAnchor>) -> Option<AnchorId> {
        let anchor = anchor?;
        let id = AnchorId(self.anchors.len());
        self.anchors.push(LosslessAnchor {
            id,
            name: anchor.name.clone(),
            span: anchor.span,
            node,
        });
        self.active_anchors.insert(anchor.name, id);
        Some(id)
    }

    fn attach_node(&mut self, id: NodeId, span: Span) -> Result<()> {
        if let Some(frame) = self.stack.last_mut() {
            match frame {
                Frame::Sequence { node, .. } => {
                    let Some(parent) = self.nodes.get_mut(node.0) else {
                        return Err(Error::new("missing sequence node", Some(span)));
                    };
                    let LosslessNodeKind::Sequence { children, .. } = &mut parent.kind else {
                        return Err(Error::new("expected sequence node", Some(span)));
                    };
                    children.push(id);
                }
                Frame::Mapping {
                    node, pending_key, ..
                } => {
                    if let Some(key) = pending_key.take() {
                        let Some(parent) = self.nodes.get_mut(node.0) else {
                            return Err(Error::new("missing mapping node", Some(span)));
                        };
                        let LosslessNodeKind::Mapping { entries, .. } = &mut parent.kind else {
                            return Err(Error::new("expected mapping node", Some(span)));
                        };
                        entries.push((key, id));
                    } else {
                        *pending_key = Some(id);
                    }
                }
            }
        } else if let Some(document) = &mut self.current_document {
            document.root = Some(id);
        }
        Ok(())
    }

    fn end_collection(&mut self, end_span: Span, expected: &str) -> Result<()> {
        let Some(frame) = self.stack.pop() else {
            return Err(Error::new(
                format!("{expected} end without matching start"),
                end_span,
            ));
        };
        if frame.name() != expected {
            return Err(Error::new(
                format!("{expected} end closed {}", frame.name()),
                end_span,
            ));
        }
        let node_id = frame.node();
        let start = self.nodes[node_id.0].span;
        self.nodes[node_id.0].span = Span::new(start.start, end_span.end, start.line, start.column);
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct DocumentBuilder {
    index: usize,
    explicit_start: bool,
    directives: EventDocumentDirectives,
    start_span: Span,
    root: Option<NodeId>,
}

impl DocumentBuilder {
    fn finish(self, explicit_end: bool, end_span: Span) -> LosslessDocument {
        LosslessDocument {
            index: self.index,
            explicit_start: self.explicit_start,
            explicit_end,
            directives: self.directives,
            start_span: self.start_span,
            end_span,
            root: self.root,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Frame {
    Sequence {
        node: NodeId,
        span: Span,
    },
    Mapping {
        node: NodeId,
        span: Span,
        pending_key: Option<NodeId>,
    },
}

impl Frame {
    fn node(&self) -> NodeId {
        match self {
            Self::Sequence { node, .. } | Self::Mapping { node, .. } => *node,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Sequence { .. } => "sequence",
            Self::Mapping { .. } => "mapping",
        }
    }

    fn span(&self) -> Span {
        match self {
            Self::Sequence { span, .. } | Self::Mapping { span, .. } => *span,
        }
    }
}

fn span_for_source_range(source: &str, start: usize, end: usize) -> Result<Span> {
    if start > end || end > source.len() {
        return Err(Error::new(
            "lossless source span is outside the retained source",
            None,
        ));
    }
    if source.get(start..end).is_none() {
        return Err(Error::new(
            "lossless source span is not on a UTF-8 boundary",
            None,
        ));
    }
    let Some(prefix) = source.get(..start) else {
        return Err(Error::new(
            "lossless source span is not on a UTF-8 boundary",
            None,
        ));
    };

    let mut line = 1usize;
    let mut column = 1usize;
    for byte in prefix.bytes() {
        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    Ok(Span::new(start, end, line, column))
}

fn scan_trivia(input: &str) -> Vec<LosslessTrivia> {
    let mut trivia = Vec::new();
    let mut offset = 0usize;
    for (line_idx, chunk) in input.split_inclusive('\n').enumerate() {
        let raw = chunk.trim_end_matches('\n').trim_end_matches('\r');
        let line = line_idx + 1;
        let bom_len = if offset == 0 && raw.starts_with('\u{feff}') {
            '\u{feff}'.len_utf8()
        } else {
            0
        };
        let raw_body = &raw[bom_len..];
        if raw_body.trim().is_empty() {
            trivia.push(LosslessTrivia {
                kind: LosslessTriviaKind::BlankLine,
                span: Span::new(offset + bom_len, offset + raw.len(), line, bom_len + 1),
                text: raw_body.to_string(),
            });
        } else if let Some(comment) = comment_start(raw_body) {
            let start = bom_len + comment;
            trivia.push(LosslessTrivia {
                kind: LosslessTriviaKind::Comment,
                span: Span::new(offset + start, offset + raw.len(), line, start + 1),
                text: raw[start..].to_string(),
            });
        }
        offset += chunk.len();
    }
    trivia
}

fn ensure_scalar_fragment(replacement: &str, span: Span) -> Result<()> {
    let parsed = parse_lossless(replacement).map_err(|error| {
        Error::new(
            format!("replacement is not valid YAML: {error}"),
            Some(span),
        )
    })?;
    if parsed.documents().len() != 1 {
        return Err(Error::new(
            "replacement must parse as one YAML document",
            Some(span),
        ));
    }
    let Some(root) = parsed.documents()[0].root() else {
        return Err(Error::new(
            "replacement must parse as one scalar node",
            Some(span),
        ));
    };
    let Some(node) = parsed.node(root) else {
        return Err(Error::new("replacement scalar node is missing", Some(span)));
    };
    if !matches!(node.kind(), LosslessNodeKind::Scalar { .. }) {
        return Err(Error::new(
            "replacement must parse as one scalar node",
            Some(span),
        ));
    }
    Ok(())
}
