//! Source-backed lossless YAML stream and graph identity API.
//!
//! This module is intentionally separate from the semantic [`crate::Node`] and
//! [`crate::Value`] loaders. The semantic loaders expand aliases and discard
//! comments because that is the useful shape for config reads. The lossless
//! stream keeps the original source text, exposes comments and blank lines as
//! trivia, and builds a graph view where aliases reference stable anchor ids.
//!
//! ```rust
//! let stream = saneyaml::parse_lossless("# service\nname: api\n")?;
//! assert_eq!(stream.comments().count(), 1);
//! let root = stream.documents()[0].root().expect("document root");
//! assert!(stream.node(root).is_some());
//! # Ok::<(), saneyaml::Error>(())
//! ```

use crate::{
    CollectionStyle, EmitCollectionStyle, EmitOptions, Error, Event, EventAnchor,
    EventDocumentDirectives, EventMeta, EventTag, LoadOptions, Result, ScalarStyle, Span,
    error::utf8_error_span, parse::comment_start,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Parses a YAML stream into a source-backed lossless graph view.
pub fn parse_lossless(input: &str) -> Result<LosslessStream> {
    LosslessStream::parse(input)
}

/// Parses a YAML stream into a source-backed lossless graph view with load options.
pub fn parse_lossless_with_options(input: &str, options: LoadOptions) -> Result<LosslessStream> {
    LosslessStream::parse_with_options(input, options)
}

/// Parses UTF-8 YAML bytes into a source-backed lossless graph view.
pub fn parse_lossless_bytes(input: &[u8]) -> Result<LosslessStream> {
    parse_lossless_bytes_with_options(input, LoadOptions::new())
}

/// Parses UTF-8 YAML bytes into a source-backed lossless graph view with load options.
pub fn parse_lossless_bytes_with_options(
    input: &[u8],
    options: LoadOptions,
) -> Result<LosslessStream> {
    options.check_input_len(input.len())?;
    match std::str::from_utf8(input) {
        Ok(input) => parse_lossless_with_options(input, options),
        Err(err) => Err(Error::encoding(
            "input is not valid UTF-8",
            utf8_error_span(input, err),
        )),
    }
}

/// Starts a source-preserving config edit session from YAML source.
///
/// Each high-level edit reparses the current working source before applying the
/// next operation, so chained path edits have sequential semantics instead of
/// reusing spans from an earlier snapshot.
pub fn edit(input: impl Into<String>) -> Result<ConfigEditor> {
    ConfigEditor::new(input)
}

/// Reads a YAML file and starts a source-preserving config edit session.
///
/// [`ConfigEditor::finish`] returns the edited source. Use
/// [`ConfigEditor::finish_to_file`] to write it back to the original path.
pub fn edit_file(path: impl AsRef<Path>) -> Result<ConfigEditor> {
    ConfigEditor::from_file(path)
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

/// One step in a [`LosslessStream`] node path.
///
/// Paths address a node by walking from a document root through mapping keys and
/// sequence indices. Use [`LosslessStream::resolve_path`] to turn a path into a
/// [`NodeId`] that composes with the structural edit helpers. `From<&str>` and
/// `From<usize>` are provided so a path can be written as
/// `[PathSegment::from("services"), PathSegment::from(0)]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    /// Descend into a block or flow mapping by matching a scalar key.
    Key(String),
    /// Descend into a block or flow sequence by zero-based index.
    Index(usize),
}

impl From<&str> for PathSegment {
    fn from(key: &str) -> Self {
        PathSegment::Key(key.to_owned())
    }
}

impl From<String> for PathSegment {
    fn from(key: String) -> Self {
        PathSegment::Key(key)
    }
}

impl From<usize> for PathSegment {
    fn from(index: usize) -> Self {
        PathSegment::Index(index)
    }
}

/// Public path type for source-preserving config refactors.
///
/// The canonical constructor is [`ConfigPath::new`], which takes explicit
/// [`PathSegment`] keys and indices. [`ConfigPath::json_pointer`] is provided for
/// slash- and tilde-heavy config keys using JSON Pointer escaping (`~1` for `/`,
/// `~0` for `~`). Dotted strings are intentionally not parsed because real
/// config keys often contain dots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigPath {
    segments: Vec<ConfigPathStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ConfigPathStep {
    Key(String),
    Index(usize),
    PointerToken(String),
}

impl ConfigPath {
    /// Creates a path from explicit key and index segments.
    pub fn new(segments: impl IntoIterator<Item = PathSegment>) -> Self {
        Self {
            segments: segments
                .into_iter()
                .map(|segment| match segment {
                    PathSegment::Key(key) => ConfigPathStep::Key(key),
                    PathSegment::Index(index) => ConfigPathStep::Index(index),
                })
                .collect(),
        }
    }

    /// Creates a path made only of mapping keys.
    pub fn keys<I, K>(keys: I) -> Self
    where
        I: IntoIterator<Item = K>,
        K: Into<String>,
    {
        Self {
            segments: keys
                .into_iter()
                .map(|key| ConfigPathStep::Key(key.into()))
                .collect(),
        }
    }

    /// Creates a root path.
    pub fn root() -> Self {
        Self::default()
    }

    /// Parses a JSON Pointer-style path.
    ///
    /// An empty string addresses the document root. Non-empty pointers must
    /// start with `/`. Escapes follow RFC 6901: `~1` decodes to `/`, and `~0`
    /// decodes to `~`. Tokens are interpreted by the node being traversed:
    /// mapping nodes use them as keys, and sequence nodes parse them as indices.
    pub fn json_pointer(pointer: &str) -> Result<Self> {
        if pointer.is_empty() {
            return Ok(Self::root());
        }
        let Some(rest) = pointer.strip_prefix('/') else {
            return Err(Error::new(
                "config JSON Pointer path must be empty or start with '/'",
                None,
            ));
        };
        let segments = rest
            .split('/')
            .map(decode_json_pointer_token)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(ConfigPathStep::PointerToken)
            .collect();
        Ok(Self { segments })
    }

    /// Returns true when this path addresses the document root.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns the number of path segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }
}

impl From<Vec<PathSegment>> for ConfigPath {
    fn from(segments: Vec<PathSegment>) -> Self {
        Self::new(segments)
    }
}

impl<const N: usize> From<[PathSegment; N]> for ConfigPath {
    fn from(segments: [PathSegment; N]) -> Self {
        Self::new(segments)
    }
}

impl From<&[PathSegment]> for ConfigPath {
    fn from(segments: &[PathSegment]) -> Self {
        Self::new(segments.iter().cloned())
    }
}

/// Source provenance for a lossless effective mapping entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LosslessEffectiveMappingSource {
    /// Entry was written directly in the requested mapping.
    Explicit,
    /// Entry came from a YAML merge key in the requested mapping or one of its
    /// merged sources.
    Merge {
        /// Source key node for the `<<` merge entry that introduced this value.
        merge_key: NodeId,
        /// Alias node used by that merge entry, when the entry was introduced
        /// through `<<: *anchor` or a merge-list alias.
        alias: Option<AliasId>,
        /// Anchor definition targeted by the merge alias, when available.
        target_anchor: Option<AnchorId>,
    },
}

impl LosslessEffectiveMappingSource {
    /// Returns whether this entry was written directly in the requested mapping.
    pub fn is_explicit(self) -> bool {
        matches!(self, Self::Explicit)
    }

    /// Returns the merge-key node that introduced this entry, if any.
    pub fn merge_key(self) -> Option<NodeId> {
        match self {
            Self::Explicit => None,
            Self::Merge { merge_key, .. } => Some(merge_key),
        }
    }

    /// Returns the merge alias that introduced this entry, if any.
    pub fn alias(self) -> Option<AliasId> {
        match self {
            Self::Explicit => None,
            Self::Merge { alias, .. } => alias,
        }
    }

    /// Returns the target anchor for the merge alias that introduced this entry,
    /// if any.
    pub fn target_anchor(self) -> Option<AnchorId> {
        match self {
            Self::Explicit => None,
            Self::Merge { target_anchor, .. } => target_anchor,
        }
    }
}

/// Entry in a mapping's lossless effective view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LosslessEffectiveMappingEntry {
    key: NodeId,
    value: NodeId,
    source: LosslessEffectiveMappingSource,
    overridden: bool,
}

impl LosslessEffectiveMappingEntry {
    /// Returns the source key node.
    pub fn key(&self) -> NodeId {
        self.key
    }

    /// Returns the source value node.
    pub fn value(&self) -> NodeId {
        self.value
    }

    /// Returns whether the entry was explicit or merge-derived.
    pub fn source(&self) -> LosslessEffectiveMappingSource {
        self.source
    }

    /// Returns whether another effective entry shadows this scalar key.
    pub fn is_overridden(&self) -> bool {
        self.overridden
    }
}

/// YAML stream that keeps the original source and a graph-shaped node view.
#[derive(Clone, Debug, PartialEq)]
pub struct LosslessStream {
    source: Arc<str>,
    documents: Vec<LosslessDocument>,
    nodes: Vec<LosslessNode>,
    anchors: Vec<LosslessAnchor>,
    aliases: Vec<LosslessAlias>,
    trivia: Vec<LosslessTrivia>,
}

impl LosslessStream {
    /// Parses a YAML stream into a source-backed lossless graph view.
    pub fn parse(input: &str) -> Result<Self> {
        Self::parse_with_options(input, LoadOptions::new())
    }

    /// Parses a YAML stream into a source-backed lossless graph view with load options.
    pub fn parse_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        let events = options.stream_events(input)?.collect::<Result<Vec<_>>>()?;
        let source: Arc<str> = Arc::from(input);
        let trivia = scan_trivia(&source);
        Builder::new(source, events, trivia).build()
    }

    /// Returns the original YAML source.
    pub fn as_source(&self) -> &str {
        &self.source
    }

    /// Consumes the stream and returns the original YAML source.
    pub fn into_source(self) -> String {
        self.source.to_string()
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

    /// Returns a mapping's explicit and merge-derived effective entries.
    ///
    /// The underlying lossless graph still preserves the raw `<<` merge key,
    /// alias nodes, comments, and source formatting. This view expands merge
    /// aliases for inspection only and keeps source provenance for each derived
    /// entry. Entries shadowed by an earlier effective scalar key are retained
    /// with [`LosslessEffectiveMappingEntry::is_overridden`] set.
    pub fn effective_mapping_entries(
        &self,
        mapping: NodeId,
    ) -> Result<Vec<LosslessEffectiveMappingEntry>> {
        let mut entries = self.collect_effective_mapping_entries(
            mapping,
            LosslessEffectiveMappingSource::Explicit,
            &mut Vec::new(),
        )?;
        self.mark_effective_mapping_overrides(&mut entries);
        Ok(entries)
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

    /// Replaces the value source for one scalar-keyed mapping entry.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::replace_mapping_value_source`].
    pub fn replace_mapping_value_source(
        &self,
        mapping: NodeId,
        key: &str,
        replacement: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.replace_mapping_value_source(mapping, key, replacement)?;
        edit.finish()
    }

    /// Inserts one complete entry into a block mapping and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::insert_block_mapping_entry_source`].
    pub fn insert_block_mapping_entry_source(
        &self,
        mapping: NodeId,
        entry_source: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.insert_block_mapping_entry_source(mapping, entry_source)?;
        edit.finish()
    }

    /// Deletes one scalar-keyed block mapping entry and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::delete_block_mapping_entry_source`].
    pub fn delete_block_mapping_entry_source(&self, mapping: NodeId, key: &str) -> Result<String> {
        let mut edit = self.edit();
        edit.delete_block_mapping_entry_source(mapping, key)?;
        edit.finish()
    }

    /// Inserts one complete entry into a flow mapping and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::insert_flow_mapping_entry_source`].
    pub fn insert_flow_mapping_entry_source(
        &self,
        mapping: NodeId,
        entry_source: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.insert_flow_mapping_entry_source(mapping, entry_source)?;
        edit.finish()
    }

    /// Deletes one scalar-keyed flow mapping entry and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::delete_flow_mapping_entry_source`].
    pub fn delete_flow_mapping_entry_source(&self, mapping: NodeId, key: &str) -> Result<String> {
        let mut edit = self.edit();
        edit.delete_flow_mapping_entry_source(mapping, key)?;
        edit.finish()
    }

    /// Replaces one sequence item's value source and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::replace_sequence_item_source`].
    pub fn replace_sequence_item_source(
        &self,
        sequence: NodeId,
        index: usize,
        replacement: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.replace_sequence_item_source(sequence, index, replacement)?;
        edit.finish()
    }

    /// Inserts one item into a block sequence and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::insert_block_sequence_item_source`].
    pub fn insert_block_sequence_item_source(
        &self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.insert_block_sequence_item_source(sequence, index, item_source)?;
        edit.finish()
    }

    /// Deletes one item from a block sequence and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::delete_block_sequence_item_source`].
    pub fn delete_block_sequence_item_source(
        &self,
        sequence: NodeId,
        index: usize,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.delete_block_sequence_item_source(sequence, index)?;
        edit.finish()
    }

    /// Inserts one item into a flow sequence and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::insert_flow_sequence_item_source`].
    pub fn insert_flow_sequence_item_source(
        &self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.insert_flow_sequence_item_source(sequence, index, item_source)?;
        edit.finish()
    }

    /// Deletes one item from a flow sequence and returns edited YAML.
    ///
    /// This is a convenience wrapper around
    /// [`LosslessEdit::delete_flow_sequence_item_source`].
    pub fn delete_flow_sequence_item_source(
        &self,
        sequence: NodeId,
        index: usize,
    ) -> Result<String> {
        let mut edit = self.edit();
        edit.delete_flow_sequence_item_source(sequence, index)?;
        edit.finish()
    }

    /// Resolves a node path against one parsed document and returns the
    /// addressed node id.
    ///
    /// The path walks from the document root: [`PathSegment::Key`] descends into
    /// a block or flow mapping by matching a scalar key, and
    /// [`PathSegment::Index`] descends into a block or flow sequence by
    /// zero-based position. An empty path returns the document root. The returned
    /// [`NodeId`] composes with the structural edit helpers, so callers can
    /// address a container by path and then insert, delete, or replace within it.
    ///
    /// Aliases are not followed: a path step into an alias node reports an error
    /// so edits never silently target a shared anchor through one of its
    /// references. A missing key, an ambiguous duplicate key, an out-of-range
    /// index, or a type mismatch each returns a span-bearing error identifying
    /// the failing segment.
    pub fn resolve_path(&self, document: usize, path: &[PathSegment]) -> Result<NodeId> {
        let doc = self.documents().get(document).ok_or_else(|| {
            Error::new(
                format!("lossless document index {document} is out of range"),
                None,
            )
        })?;
        let mut current = doc.root().ok_or_else(|| {
            Error::new(
                format!("lossless document index {document} has no root node"),
                None,
            )
        })?;
        for (depth, segment) in path.iter().enumerate() {
            current = self.resolve_path_segment(current, segment, depth)?;
        }
        Ok(current)
    }

    /// Replaces the source of the node addressed by `path` and returns edited
    /// YAML.
    ///
    /// This is a path-addressed convenience over [`Self::resolve_path`] and
    /// [`Self::replace_node_source`]: the path must address the value node to
    /// rewrite, for example `[PathSegment::from("services"),
    /// PathSegment::from("db"), PathSegment::from("image")]`. The replacement is
    /// raw YAML source for the selected node range, and the full stream is
    /// reparsed before the edited string is returned.
    pub fn replace_value_at_path(
        &self,
        document: usize,
        path: &[PathSegment],
        replacement: impl Into<String>,
    ) -> Result<String> {
        let node = self.resolve_path(document, path)?;
        self.replace_node_source(node, replacement)
    }

    /// Deletes the entry or item addressed by `path` and returns edited YAML.
    ///
    /// The final path segment selects what to delete from its resolved parent
    /// container: a [`PathSegment::Key`] deletes that scalar-keyed entry from a
    /// block or flow mapping, and a [`PathSegment::Index`] deletes that item from
    /// a block or flow sequence. The block/flow style is detected from the parent
    /// node, so callers need not pick a style-specific helper. An empty path is
    /// rejected because the document root has no container to delete from.
    pub fn delete_at_path(&self, document: usize, path: &[PathSegment]) -> Result<String> {
        let Some((last, parent_path)) = path.split_last() else {
            return Err(Error::new(
                "lossless delete requires a non-empty path",
                None,
            ));
        };
        let parent = self.resolve_path(document, parent_path)?;
        let parent_kind = self
            .node(parent)
            .ok_or_else(|| Error::new("lossless path node id is out of bounds", None))?
            .kind();
        match (last, parent_kind) {
            (PathSegment::Key(key), LosslessNodeKind::Mapping { style, .. }) => match style {
                CollectionStyle::Block => self.delete_block_mapping_entry_source(parent, key),
                CollectionStyle::Flow => self.delete_flow_mapping_entry_source(parent, key),
            },
            (PathSegment::Index(index), LosslessNodeKind::Sequence { style, .. }) => match style {
                CollectionStyle::Block => self.delete_block_sequence_item_source(parent, *index),
                CollectionStyle::Flow => self.delete_flow_sequence_item_source(parent, *index),
            },
            (PathSegment::Key(key), _) => Err(Error::new(
                format!("lossless delete of key {key:?} requires a mapping parent"),
                self.node(parent).map(LosslessNode::span),
            )),
            (PathSegment::Index(index), _) => Err(Error::new(
                format!("lossless delete of index {index} requires a sequence parent"),
                self.node(parent).map(LosslessNode::span),
            )),
        }
    }

    /// Inserts one mapping entry into the mapping addressed by `path`.
    ///
    /// `entry_source` is unindented YAML that must parse as exactly one mapping
    /// entry, for example `labels:\n  role: web`. The block/flow style is
    /// detected from the addressed node, so callers use one method for either
    /// mapping shape. The edited stream is reparsed before it is returned.
    pub fn insert_entry_at_path(
        &self,
        document: usize,
        path: &[PathSegment],
        entry_source: impl Into<String>,
    ) -> Result<String> {
        let node = self.resolve_path(document, path)?;
        match self
            .node(node)
            .ok_or_else(|| Error::new("lossless path node id is out of bounds", None))?
            .kind()
        {
            LosslessNodeKind::Mapping {
                style: CollectionStyle::Block,
                ..
            } => self.insert_block_mapping_entry_source(node, entry_source),
            LosslessNodeKind::Mapping {
                style: CollectionStyle::Flow,
                ..
            } => self.insert_flow_mapping_entry_source(node, entry_source),
            _ => Err(Error::new(
                "lossless entry insertion requires a mapping node",
                self.node(node).map(LosslessNode::span),
            )),
        }
    }

    /// Inserts one item at `index` into the sequence addressed by `path`.
    ///
    /// `item_source` is unindented YAML that must parse as exactly one node. The
    /// block/flow style is detected from the addressed node, so callers use one
    /// method for either sequence shape. The edited stream is reparsed before it
    /// is returned.
    pub fn insert_item_at_path(
        &self,
        document: usize,
        path: &[PathSegment],
        index: usize,
        item_source: impl Into<String>,
    ) -> Result<String> {
        let node = self.resolve_path(document, path)?;
        match self
            .node(node)
            .ok_or_else(|| Error::new("lossless path node id is out of bounds", None))?
            .kind()
        {
            LosslessNodeKind::Sequence {
                style: CollectionStyle::Block,
                ..
            } => self.insert_block_sequence_item_source(node, index, item_source),
            LosslessNodeKind::Sequence {
                style: CollectionStyle::Flow,
                ..
            } => self.insert_flow_sequence_item_source(node, index, item_source),
            _ => Err(Error::new(
                "lossless item insertion requires a sequence node",
                self.node(node).map(LosslessNode::span),
            )),
        }
    }

    fn resolve_path_segment(
        &self,
        node: NodeId,
        segment: &PathSegment,
        depth: usize,
    ) -> Result<NodeId> {
        let current = self
            .node(node)
            .ok_or_else(|| Error::new("lossless path node id is out of bounds", None))?;
        match (segment, current.kind()) {
            (PathSegment::Key(key), LosslessNodeKind::Mapping { entries, .. }) => {
                let mut matches = entries.iter().filter_map(|(key_id, value_id)| {
                    match self.node(*key_id)?.kind() {
                        LosslessNodeKind::Scalar { value, .. } if value == key => Some(*value_id),
                        _ => None,
                    }
                });
                let Some(value) = matches.next() else {
                    return Err(Error::new(
                        format!("lossless path segment {depth} key {key:?} was not found"),
                        Some(current.span()),
                    ));
                };
                if matches.next().is_some() {
                    return Err(Error::new(
                        format!("lossless path segment {depth} key {key:?} is ambiguous"),
                        Some(current.span()),
                    ));
                }
                Ok(value)
            }
            (PathSegment::Index(index), LosslessNodeKind::Sequence { children, .. }) => {
                children.get(*index).copied().ok_or_else(|| {
                    Error::new(
                        format!(
                            "lossless path segment {depth} index {index} is out of bounds for a sequence of length {}",
                            children.len()
                        ),
                        Some(current.span()),
                    )
                })
            }
            (PathSegment::Key(key), _) => Err(Error::new(
                format!("lossless path segment {depth} key {key:?} requires a mapping node"),
                Some(current.span()),
            )),
            (PathSegment::Index(index), _) => Err(Error::new(
                format!("lossless path segment {depth} index {index} requires a sequence node"),
                Some(current.span()),
            )),
        }
    }

    fn collect_effective_mapping_entries(
        &self,
        mapping: NodeId,
        explicit_source: LosslessEffectiveMappingSource,
        stack: &mut Vec<NodeId>,
    ) -> Result<Vec<LosslessEffectiveMappingEntry>> {
        if stack.contains(&mapping) {
            return Err(Error::new(
                "recursive lossless merge expansion is not supported",
                self.node(mapping).map(LosslessNode::span),
            ));
        }
        stack.push(mapping);

        let mapping_node = self
            .node(mapping)
            .ok_or_else(|| Error::new("lossless mapping node id is out of bounds", None))?;
        let LosslessNodeKind::Mapping { entries, .. } = mapping_node.kind() else {
            return Err(Error::new(
                "lossless effective entries require a mapping node",
                Some(mapping_node.span()),
            ));
        };

        let mut explicit_entries = Vec::new();
        let mut merged_entries = Vec::new();
        for (key, value) in entries {
            if self.is_lossless_merge_key(*key)? {
                self.collect_lossless_merge_value(*key, *value, &mut merged_entries, stack)?;
            } else {
                explicit_entries.push(LosslessEffectiveMappingEntry {
                    key: *key,
                    value: *value,
                    source: explicit_source,
                    overridden: false,
                });
            }
        }

        stack.pop();
        explicit_entries.extend(merged_entries);
        Ok(explicit_entries)
    }

    fn collect_lossless_merge_value(
        &self,
        merge_key: NodeId,
        value: NodeId,
        output: &mut Vec<LosslessEffectiveMappingEntry>,
        stack: &mut Vec<NodeId>,
    ) -> Result<()> {
        let value_node = self
            .node(value)
            .ok_or_else(|| Error::new("lossless merge value node id is out of bounds", None))?;
        match value_node.kind() {
            LosslessNodeKind::Alias { alias, target, .. } => {
                let target_node = self
                    .anchor(*target)
                    .and_then(|anchor| self.node(anchor.node()))
                    .ok_or_else(|| {
                        Error::new(
                            "lossless merge alias target is out of bounds",
                            Some(value_node.span()),
                        )
                    })?;
                if !matches!(target_node.kind(), LosslessNodeKind::Mapping { .. }) {
                    return Err(Error::new(
                        "lossless merge alias must target a mapping",
                        Some(value_node.span()),
                    ));
                }
                output.extend(self.collect_effective_mapping_entries(
                    target_node.id(),
                    LosslessEffectiveMappingSource::Merge {
                        merge_key,
                        alias: Some(*alias),
                        target_anchor: Some(*target),
                    },
                    stack,
                )?);
                Ok(())
            }
            LosslessNodeKind::Sequence { children, .. } => {
                for child in children {
                    self.collect_lossless_merge_value(merge_key, *child, output, stack)?;
                }
                Ok(())
            }
            LosslessNodeKind::Mapping { .. } => {
                output.extend(self.collect_effective_mapping_entries(
                    value_node.id(),
                    LosslessEffectiveMappingSource::Merge {
                        merge_key,
                        alias: None,
                        target_anchor: None,
                    },
                    stack,
                )?);
                Ok(())
            }
            LosslessNodeKind::Scalar { .. } => Err(Error::new(
                "lossless merge value must be a mapping, alias, or sequence",
                Some(value_node.span()),
            )),
        }
    }

    fn mark_effective_mapping_overrides(&self, entries: &mut [LosslessEffectiveMappingEntry]) {
        let mut seen = Vec::<String>::new();
        for entry in entries {
            let Some(key) = self.scalar_key(entry.key) else {
                continue;
            };
            if seen.iter().any(|seen| seen == key) {
                entry.overridden = true;
            } else {
                seen.push(key.to_owned());
            }
        }
    }

    fn is_lossless_merge_key(&self, key: NodeId) -> Result<bool> {
        let key_node = self
            .node(key)
            .ok_or_else(|| Error::new("lossless mapping key id is out of bounds", None))?;
        let LosslessNodeKind::Scalar { value, .. } = key_node.kind() else {
            return Ok(false);
        };
        if value != "<<" {
            return Ok(false);
        }
        Ok(match key_node.tag() {
            None => true,
            Some(tag) => tag.tag.is_yaml_core("merge"),
        })
    }

    fn scalar_key(&self, key: NodeId) -> Option<&str> {
        match self.node(key)?.kind() {
            LosslessNodeKind::Scalar { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// Owned source-preserving editor for common config refactors.
///
/// `ConfigEditor` is the high-level companion to [`LosslessEdit`]. It owns the
/// working YAML source and applies each operation against the current source by
/// reparsing before the edit. This keeps chained path edits stable when earlier
/// operations shift later spans.
#[derive(Clone, Debug)]
pub struct ConfigEditor {
    source: String,
    options: LoadOptions,
    file_path: Option<PathBuf>,
}

impl ConfigEditor {
    /// Creates an editor from YAML source.
    pub fn new(input: impl Into<String>) -> Result<Self> {
        Self::new_with_options(input, LoadOptions::new())
    }

    /// Creates an editor from YAML source with explicit load options.
    pub fn new_with_options(input: impl Into<String>, options: LoadOptions) -> Result<Self> {
        let source = input.into();
        parse_lossless_with_options(&source, options)?;
        Ok(Self {
            source,
            options,
            file_path: None,
        })
    }

    /// Reads a YAML file and creates an editor for its contents.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_file_with_options(path, LoadOptions::new())
    }

    /// Reads a YAML file and creates an editor with explicit load options.
    pub fn from_file_with_options(path: impl AsRef<Path>, options: LoadOptions) -> Result<Self> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| {
            Error::new(
                format!("failed to read YAML file {}: {error}", path.display()),
                None,
            )
        })?;
        parse_lossless_with_options(&source, options)?;
        Ok(Self {
            source,
            options,
            file_path: Some(path.to_path_buf()),
        })
    }

    /// Returns the current working source.
    pub fn as_source(&self) -> &str {
        &self.source
    }

    /// Replaces a value in document 0 with serialized YAML for `value`.
    pub fn set<P, T>(&mut self, path: P, value: T) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        self.set_in_document(0, path, value)
    }

    /// Replaces a value in the selected document with serialized YAML.
    pub fn set_in_document<P, T>(&mut self, document: usize, path: P, value: T) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        let path = path.into();
        self.apply(|stream, options| set_path_source(stream, document, &path, &value, options))
    }

    /// Removes a mapping entry or sequence item from document 0.
    pub fn remove<P>(&mut self, path: P) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
    {
        self.remove_in_document(0, path)
    }

    /// Removes a mapping entry or sequence item from the selected document.
    pub fn remove_in_document<P>(&mut self, document: usize, path: P) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
    {
        let path = path.into();
        self.apply(|stream, options| remove_path_source(stream, document, &path, options))
    }

    /// Renames a scalar mapping key in document 0.
    pub fn rename<P>(&mut self, path: P, new_key: impl AsRef<str>) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
    {
        self.rename_in_document(0, path, new_key)
    }

    /// Renames a scalar mapping key in the selected document.
    pub fn rename_in_document<P>(
        &mut self,
        document: usize,
        path: P,
        new_key: impl AsRef<str>,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
    {
        let path = path.into();
        let new_key = serialize_key_fragment(new_key.as_ref())?;
        self.apply(|stream, options| rename_path_source(stream, document, &path, &new_key, options))
    }

    /// Inserts a serialized key/value entry into a mapping in document 0.
    pub fn insert<P, T>(
        &mut self,
        mapping_path: P,
        key: impl AsRef<str>,
        value: T,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        self.insert_in_document(0, mapping_path, key, value)
    }

    /// Inserts a serialized key/value entry into a mapping in the selected document.
    pub fn insert_in_document<P, T>(
        &mut self,
        document: usize,
        mapping_path: P,
        key: impl AsRef<str>,
        value: T,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        let mapping_path = mapping_path.into();
        let key = key.as_ref().to_owned();
        self.apply(|stream, options| {
            insert_entry_source(stream, document, &mapping_path, &key, &value, options)
        })
    }

    /// Appends a serialized item to a sequence in document 0.
    pub fn push<P, T>(&mut self, sequence_path: P, value: T) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        self.push_in_document(0, sequence_path, value)
    }

    /// Appends a serialized item to a sequence in the selected document.
    pub fn push_in_document<P, T>(
        &mut self,
        document: usize,
        sequence_path: P,
        value: T,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        let sequence_path = sequence_path.into();
        self.apply(|stream, options| {
            push_item_source(stream, document, &sequence_path, &value, options)
        })
    }

    /// Inserts a serialized sequence item at `index` in document 0.
    pub fn insert_item<P, T>(
        &mut self,
        sequence_path: P,
        index: usize,
        value: T,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        self.insert_item_in_document(0, sequence_path, index, value)
    }

    /// Inserts a serialized sequence item at `index` in the selected document.
    pub fn insert_item_in_document<P, T>(
        &mut self,
        document: usize,
        sequence_path: P,
        index: usize,
        value: T,
    ) -> Result<&mut Self>
    where
        P: Into<ConfigPath>,
        T: Serialize,
    {
        let sequence_path = sequence_path.into();
        self.apply(|stream, options| {
            insert_item_source(stream, document, &sequence_path, index, &value, options)
        })
    }

    /// Validates and returns the edited source.
    pub fn finish(self) -> Result<String> {
        parse_lossless_with_options(&self.source, self.options)?;
        Ok(self.source)
    }

    /// Validates the edited source and writes it to the file opened by
    /// [`Self::from_file`] or [`edit_file`].
    pub fn finish_to_file(self) -> Result<()> {
        let Some(path) = self.file_path.clone() else {
            return Err(Error::new(
                "config editor was not created from a file",
                None,
            ));
        };
        let source = self.finish()?;
        fs::write(&path, source).map_err(|error| {
            Error::new(
                format!("failed to write YAML file {}: {error}", path.display()),
                None,
            )
        })
    }

    fn apply(
        &mut self,
        edit: impl FnOnce(&LosslessStream, LoadOptions) -> Result<String>,
    ) -> Result<&mut Self> {
        let stream = parse_lossless_with_options(&self.source, self.options)?;
        let edited = edit(&stream, self.options)?;
        parse_lossless_with_options(&edited, self.options)?;
        self.source = edited;
        Ok(self)
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
        let replacement = replacement.into();
        if let Some((key, _mapping)) = self.implicit_null_block_mapping_value_parent(node)?
            && let Some((span, replacement)) =
                self.implicit_null_mapping_value_replacement(key, node, &replacement)?
        {
            return self.push_replacement(span, replacement);
        }
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

    /// Replaces the value source for one scalar-keyed mapping entry.
    ///
    /// The key is matched against the parser-resolved scalar key text. The
    /// replacement is raw YAML source for the entry value; untouched source,
    /// including the key spelling, comments, anchors, tags, and surrounding
    /// formatting, is kept byte-for-byte.
    pub fn replace_mapping_value_source(
        &mut self,
        mapping: NodeId,
        key: &str,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        let entry = self.unique_mapping_entry_by_key(mapping, key)?;
        let replacement = replacement.into();
        let mapping_node = self.mapping_node(mapping)?;
        let is_block_mapping = matches!(
            mapping_node.kind(),
            LosslessNodeKind::Mapping {
                style: CollectionStyle::Block,
                ..
            }
        );
        if is_block_mapping
            && let Some((span, replacement)) =
                self.implicit_null_mapping_value_replacement(entry.key, entry.value, &replacement)?
        {
            return self.push_replacement(span, replacement);
        }
        let value_span = self.checked_node_span(entry.value)?;
        self.push_replacement(value_span, replacement)
    }

    /// Inserts one complete mapping entry into a block mapping.
    ///
    /// `entry_source` is unindented YAML source that must parse as exactly one
    /// mapping entry, for example `labels:\n  role: web`. The inserted source is
    /// indented to match the target block mapping's existing entries, and the
    /// final stream is reparsed before [`Self::finish`] returns it.
    pub fn insert_block_mapping_entry_source(
        &mut self,
        mapping: NodeId,
        entry_source: impl Into<String>,
    ) -> Result<&mut Self> {
        self.insert_block_mapping_entry_source_with_options(
            mapping,
            entry_source,
            LoadOptions::new(),
        )
    }

    fn insert_block_mapping_entry_source_with_options(
        &mut self,
        mapping: NodeId,
        entry_source: impl Into<String>,
        options: LoadOptions,
    ) -> Result<&mut Self> {
        let mapping = self.mapping_node(mapping)?;
        let LosslessNodeKind::Mapping { style, entries } = mapping.kind() else {
            unreachable!("mapping_node only returns mapping nodes");
        };
        if *style != CollectionStyle::Block {
            return Err(Error::new(
                "structural mapping entry insertion requires a block mapping",
                Some(mapping.span()),
            ));
        }
        let Some((first_key, _)) = entries.first() else {
            return Err(Error::new(
                "structural mapping entry insertion requires a non-empty block mapping",
                Some(mapping.span()),
            ));
        };
        let Some((_, last_value)) = entries.last() else {
            unreachable!("entries.first checked non-empty mapping");
        };
        let first_key = self
            .stream
            .node(*first_key)
            .ok_or_else(|| Error::new("lossless mapping key id is out of bounds", None))?;
        let last_value = self
            .stream
            .node(*last_value)
            .ok_or_else(|| Error::new("lossless mapping value id is out of bounds", None))?;
        let entry_source = entry_source.into();
        ensure_single_mapping_entry_fragment_with_options(&entry_source, mapping.span(), options)?;
        let indent = block_mapping_entry_indent(&self.stream.source, first_key.span().start);
        let offset = line_end_including_newline(&self.stream.source, last_value.span().end);
        let mut insertion = indent_entry_source(&entry_source, &indent);
        if offset == self.stream.source.len() && !self.stream.source.ends_with('\n') {
            insertion.insert(0, '\n');
        }
        self.insert_source(offset, insertion)
    }

    /// Deletes one scalar-keyed entry from a block mapping.
    ///
    /// The deleted span starts at the key's source line and ends after the
    /// value's last source line, so inline comments and nested block values for
    /// that entry are removed while unrelated surrounding bytes are preserved.
    pub fn delete_block_mapping_entry_source(
        &mut self,
        mapping: NodeId,
        key: &str,
    ) -> Result<&mut Self> {
        let mapping_node = self.mapping_node(mapping)?;
        let LosslessNodeKind::Mapping { style, .. } = mapping_node.kind() else {
            unreachable!("mapping_node only returns mapping nodes");
        };
        if *style != CollectionStyle::Block {
            return Err(Error::new(
                "structural mapping entry deletion requires a block mapping",
                Some(mapping_node.span()),
            ));
        }
        let entry = self.unique_mapping_entry_by_key(mapping, key)?;
        let key_node = self
            .stream
            .node(entry.key)
            .ok_or_else(|| Error::new("lossless mapping key id is out of bounds", None))?;
        let value_node = self
            .stream
            .node(entry.value)
            .ok_or_else(|| Error::new("lossless mapping value id is out of bounds", None))?;
        let source = &self.stream.source;
        let end = line_end_including_newline(source, value_node.span().end);
        if let Some((dash_end, _)) =
            compact_sequence_mapping_key_prefix(source, key_node.span().start)
        {
            let span = self.stream.source_span(dash_end, end)?;
            return self.replace_source_span(span, "\n");
        }
        let start = line_start(source, key_node.span().start);
        let span = self.stream.source_span(start, end)?;
        self.delete_source_span(span)
    }

    /// Inserts one complete mapping entry into a flow mapping.
    ///
    /// `entry_source` is raw YAML source that must parse as exactly one mapping
    /// entry, for example `replicas: 2`. The entry is appended before the flow
    /// mapping's closing `}` and the final stream is reparsed before
    /// [`Self::finish`] returns it.
    pub fn insert_flow_mapping_entry_source(
        &mut self,
        mapping: NodeId,
        entry_source: impl Into<String>,
    ) -> Result<&mut Self> {
        self.insert_flow_mapping_entry_source_with_options(
            mapping,
            entry_source,
            LoadOptions::new(),
        )
    }

    fn insert_flow_mapping_entry_source_with_options(
        &mut self,
        mapping: NodeId,
        entry_source: impl Into<String>,
        options: LoadOptions,
    ) -> Result<&mut Self> {
        let mapping_node = self.mapping_node(mapping)?;
        let LosslessNodeKind::Mapping { style, entries } = mapping_node.kind() else {
            unreachable!("mapping_node only returns mapping nodes");
        };
        if *style != CollectionStyle::Flow {
            return Err(Error::new(
                "structural mapping entry insertion requires a flow mapping",
                Some(mapping_node.span()),
            ));
        }
        let entry_source = entry_source.into();
        ensure_single_mapping_entry_fragment_with_options(
            &entry_source,
            mapping_node.span(),
            options,
        )?;
        let insertion = if let Some((_, last_value)) = entries.last() {
            let last_value = self
                .stream
                .node(*last_value)
                .ok_or_else(|| Error::new("lossless mapping value id is out of bounds", None))?;
            let offset = last_value.span().end;
            return self.insert_source(offset, format!(", {entry_source}"));
        } else {
            entry_source
        };
        let offset = self.flow_collection_closing_offset(mapping_node, b'}')?;
        self.insert_source(offset, insertion)
    }

    /// Deletes one scalar-keyed entry from a flow mapping.
    ///
    /// The deletion also removes the adjacent comma separator so the remaining
    /// flow mapping reparses while preserving unrelated source bytes.
    pub fn delete_flow_mapping_entry_source(
        &mut self,
        mapping: NodeId,
        key: &str,
    ) -> Result<&mut Self> {
        let mapping_node = self.mapping_node(mapping)?;
        let LosslessNodeKind::Mapping { style, entries } = mapping_node.kind() else {
            unreachable!("mapping_node only returns mapping nodes");
        };
        if *style != CollectionStyle::Flow {
            return Err(Error::new(
                "structural mapping entry deletion requires a flow mapping",
                Some(mapping_node.span()),
            ));
        }
        let (entry_index, entry) = self.unique_mapping_entry_index_by_key(mapping, key)?;
        let key_node = self
            .stream
            .node(entry.key)
            .ok_or_else(|| Error::new("lossless mapping key id is out of bounds", None))?;
        let value_node = self
            .stream
            .node(entry.value)
            .ok_or_else(|| Error::new("lossless mapping value id is out of bounds", None))?;
        let (start, end) = if entries.len() == 1 {
            (key_node.span().start, value_node.span().end)
        } else if entry_index + 1 < entries.len() {
            let next_key = self
                .stream
                .node(entries[entry_index + 1].0)
                .ok_or_else(|| Error::new("lossless mapping key id is out of bounds", None))?;
            (key_node.span().start, next_key.span().start)
        } else {
            let previous_value = self
                .stream
                .node(entries[entry_index - 1].1)
                .ok_or_else(|| Error::new("lossless mapping value id is out of bounds", None))?;
            (previous_value.span().end, value_node.span().end)
        };
        let span = self.stream.source_span(start, end)?;
        self.delete_source_span(span)
    }

    /// Replaces the value source for one sequence item.
    ///
    /// The replacement is raw YAML source for the item value. For block
    /// sequences, the leading dash and surrounding indentation are preserved;
    /// for flow sequences, only the selected item node's source is replaced.
    pub fn replace_sequence_item_source(
        &mut self,
        sequence: NodeId,
        index: usize,
        replacement: impl Into<String>,
    ) -> Result<&mut Self> {
        self.replace_sequence_item_source_with_options(
            sequence,
            index,
            replacement,
            LoadOptions::new(),
        )
    }

    fn replace_sequence_item_source_with_options(
        &mut self,
        sequence: NodeId,
        index: usize,
        replacement: impl Into<String>,
        options: LoadOptions,
    ) -> Result<&mut Self> {
        let sequence_node = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { style, .. } = sequence_node.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        let item = self.sequence_item(sequence, index)?;
        let item_node = self
            .stream
            .node(item)
            .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
        let replacement = replacement.into();
        ensure_single_node_fragment_with_options(
            &replacement,
            sequence_node.span(),
            "sequence item source",
            options,
        )?;
        if *style == CollectionStyle::Block {
            let start = line_start(&self.stream.source, item_node.span().start);
            let end = line_end_including_newline(&self.stream.source, item_node.span().end);
            let span = self.stream.source_span(start, end)?;
            let indent = line_indent(&self.stream.source, start);
            let replacement = format_block_sequence_item_source(&replacement, indent);
            self.push_replacement(span, replacement)
        } else {
            let item_span = self.checked_node_span(item)?;
            self.push_replacement(item_span, replacement)
        }
    }

    /// Inserts one complete item into a block sequence.
    ///
    /// `item_source` is unindented YAML source for the item value, for example
    /// `name: build\nrun: cargo build`. The inserted source is rendered with
    /// the target sequence's existing dash indentation, and the final stream is
    /// reparsed before [`Self::finish`] returns it.
    pub fn insert_block_sequence_item_source(
        &mut self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
    ) -> Result<&mut Self> {
        self.insert_block_sequence_item_source_with_options(
            sequence,
            index,
            item_source,
            LoadOptions::new(),
        )
    }

    fn insert_block_sequence_item_source_with_options(
        &mut self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
        options: LoadOptions,
    ) -> Result<&mut Self> {
        let sequence = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { style, children } = sequence.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        if *style != CollectionStyle::Block {
            return Err(Error::new(
                "structural sequence item insertion requires a block sequence",
                Some(sequence.span()),
            ));
        }
        if children.is_empty() {
            return Err(Error::new(
                "structural sequence item insertion requires a non-empty block sequence",
                Some(sequence.span()),
            ));
        }
        if index > children.len() {
            return Err(Error::new(
                format!(
                    "lossless sequence item index {index} is out of bounds for {} items",
                    children.len()
                ),
                Some(sequence.span()),
            ));
        }

        let first_child = self
            .stream
            .node(children[0])
            .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
        let item_source = item_source.into();
        ensure_single_node_fragment_with_options(
            &item_source,
            sequence.span(),
            "sequence item source",
            options,
        )?;
        let indent = line_indent(
            &self.stream.source,
            line_start(&self.stream.source, first_child.span().start),
        );
        let mut insertion = format_block_sequence_item_source(&item_source, indent);
        let offset = if index == children.len() {
            let last_child = self
                .stream
                .node(children[children.len() - 1])
                .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
            let offset = line_end_including_newline(&self.stream.source, last_child.span().end);
            if offset == self.stream.source.len() && !self.stream.source.ends_with('\n') {
                insertion.insert(0, '\n');
            }
            offset
        } else {
            let target_child = self
                .stream
                .node(children[index])
                .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
            line_start(&self.stream.source, target_child.span().start)
        };
        self.insert_source(offset, insertion)
    }

    /// Deletes one item from a block sequence.
    ///
    /// The deleted span starts at the item's dash line and ends after the
    /// item's last source line, so inline comments and nested block values for
    /// that item are removed while unrelated surrounding bytes are preserved.
    pub fn delete_block_sequence_item_source(
        &mut self,
        sequence: NodeId,
        index: usize,
    ) -> Result<&mut Self> {
        let sequence_node = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { style, .. } = sequence_node.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        if *style != CollectionStyle::Block {
            return Err(Error::new(
                "structural sequence item deletion requires a block sequence",
                Some(sequence_node.span()),
            ));
        }
        let item = self.sequence_item(sequence, index)?;
        let item_node = self
            .stream
            .node(item)
            .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
        let start = line_start(&self.stream.source, item_node.span().start);
        let end = line_end_including_newline(&self.stream.source, item_node.span().end);
        let span = self.stream.source_span(start, end)?;
        self.delete_source_span(span)
    }

    /// Inserts one complete item into a flow sequence.
    ///
    /// `item_source` is raw YAML source for the item value. The inserted source
    /// is placed before the selected item index, or before the closing `]` when
    /// appending, and the final stream is reparsed before [`Self::finish`]
    /// returns it.
    pub fn insert_flow_sequence_item_source(
        &mut self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
    ) -> Result<&mut Self> {
        self.insert_flow_sequence_item_source_with_options(
            sequence,
            index,
            item_source,
            LoadOptions::new(),
        )
    }

    fn insert_flow_sequence_item_source_with_options(
        &mut self,
        sequence: NodeId,
        index: usize,
        item_source: impl Into<String>,
        options: LoadOptions,
    ) -> Result<&mut Self> {
        let sequence_node = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { style, children } = sequence_node.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        if *style != CollectionStyle::Flow {
            return Err(Error::new(
                "structural sequence item insertion requires a flow sequence",
                Some(sequence_node.span()),
            ));
        }
        if index > children.len() {
            return Err(Error::new(
                format!(
                    "lossless sequence item index {index} is out of bounds for {} items",
                    children.len()
                ),
                Some(sequence_node.span()),
            ));
        }
        let item_source = item_source.into();
        ensure_single_node_fragment_with_options(
            &item_source,
            sequence_node.span(),
            "sequence item source",
            options,
        )?;
        let insertion = if children.is_empty() {
            item_source
        } else if index == children.len() {
            format!(", {item_source}")
        } else {
            format!("{item_source}, ")
        };
        let offset = if index == children.len() {
            if let Some(last_child) = children.last() {
                self.stream
                    .node(*last_child)
                    .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?
                    .span()
                    .end
            } else {
                self.flow_collection_closing_offset(sequence_node, b']')?
            }
        } else {
            self.stream
                .node(children[index])
                .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?
                .span()
                .start
        };
        self.insert_source(offset, insertion)
    }

    /// Deletes one item from a flow sequence.
    ///
    /// The deletion also removes the adjacent comma separator so the remaining
    /// flow sequence reparses while preserving unrelated source bytes.
    pub fn delete_flow_sequence_item_source(
        &mut self,
        sequence: NodeId,
        index: usize,
    ) -> Result<&mut Self> {
        let sequence_node = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { style, children } = sequence_node.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        if *style != CollectionStyle::Flow {
            return Err(Error::new(
                "structural sequence item deletion requires a flow sequence",
                Some(sequence_node.span()),
            ));
        }
        let item = self.sequence_item(sequence, index)?;
        let item_node = self
            .stream
            .node(item)
            .ok_or_else(|| Error::new("lossless sequence item id is out of bounds", None))?;
        let (start, end) =
            if children.len() == 1 {
                (item_node.span().start, item_node.span().end)
            } else if index + 1 < children.len() {
                let next_item = self.stream.node(children[index + 1]).ok_or_else(|| {
                    Error::new("lossless sequence item id is out of bounds", None)
                })?;
                (item_node.span().start, next_item.span().start)
            } else {
                let previous_item = self.stream.node(children[index - 1]).ok_or_else(|| {
                    Error::new("lossless sequence item id is out of bounds", None)
                })?;
                (previous_item.span().end, item_node.span().end)
            };
        let span = self.stream.source_span(start, end)?;
        self.delete_source_span(span)
    }

    /// Returns validated edited YAML with untouched source bytes preserved.
    pub fn finish(self) -> Result<String> {
        self.finish_with_options(LoadOptions::new())
    }

    fn finish_with_options(mut self, options: LoadOptions) -> Result<String> {
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

        parse_lossless_with_options(&output, options)?;
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

    fn implicit_null_block_mapping_value_parent(
        &self,
        value: NodeId,
    ) -> Result<Option<(NodeId, NodeId)>> {
        let Some(value_node) = self.stream.node(value) else {
            return Err(Error::new("lossless node id is out of bounds", None));
        };
        if !is_implicit_null_mapping_value_node(self.stream, value_node) {
            return Ok(None);
        }

        for mapping in self.stream.nodes() {
            if let LosslessNodeKind::Mapping {
                style: CollectionStyle::Block,
                entries,
            } = mapping.kind()
                && let Some((key, _)) = entries
                    .iter()
                    .find(|(_, entry_value)| *entry_value == value)
            {
                return Ok(Some((*key, mapping.id())));
            }
        }

        Ok(None)
    }

    fn implicit_null_mapping_value_replacement(
        &self,
        key: NodeId,
        value: NodeId,
        replacement: &str,
    ) -> Result<Option<(Span, String)>> {
        let key_node = self
            .stream
            .node(key)
            .ok_or_else(|| Error::new("config mapping key node id is out of bounds", None))?;
        let value_node = self
            .stream
            .node(value)
            .ok_or_else(|| Error::new("config mapping value node id is out of bounds", None))?;
        if !is_implicit_null_mapping_value_node(self.stream, value_node) {
            return Ok(None);
        }

        let source = &self.stream.source;
        let key_line_start = line_start(source, key_node.span().start);
        let value_line_start = line_start(source, value_node.span().start);
        if key_line_start != value_line_start {
            return Ok(None);
        }

        let value_line_end = line_end_including_newline(source, value_node.span().end);
        let body_end = line_body_end(source, value_line_end);
        let after_colon = block_mapping_value_after_colon(source, key_node, body_end)?;
        let tail = source
            .get(after_colon..body_end)
            .ok_or_else(|| Error::new("config mapping value tail span is invalid", None))?;
        let comment = comment_start(tail);
        let value_gap_end = comment
            .map(|offset| after_colon + offset)
            .unwrap_or(body_end);
        let value_gap = source
            .get(after_colon..value_gap_end)
            .ok_or_else(|| Error::new("config mapping value gap span is invalid", None))?;
        if !value_gap.trim().is_empty() {
            return Ok(None);
        }

        let span = self.stream.source_span(after_colon, value_gap_end)?;
        let replacement = format_implicit_null_inline_replacement(replacement, comment.is_some());
        Ok(Some((span, replacement)))
    }

    fn mapping_node(&self, mapping: NodeId) -> Result<&LosslessNode> {
        let node = self
            .stream
            .node(mapping)
            .ok_or_else(|| Error::new("lossless mapping node id is out of bounds", None))?;
        if !matches!(node.kind(), LosslessNodeKind::Mapping { .. }) {
            return Err(Error::new(
                "lossless structural edit target is not a mapping",
                Some(node.span()),
            ));
        }
        Ok(node)
    }

    fn unique_mapping_entry_by_key(&self, mapping: NodeId, key: &str) -> Result<MappingEntry> {
        self.unique_mapping_entry_index_by_key(mapping, key)
            .map(|(_, entry)| entry)
    }

    fn unique_mapping_entry_index_by_key(
        &self,
        mapping: NodeId,
        key: &str,
    ) -> Result<(usize, MappingEntry)> {
        let mapping_node = self.mapping_node(mapping)?;
        let LosslessNodeKind::Mapping { entries, .. } = mapping_node.kind() else {
            unreachable!("mapping_node only returns mapping nodes");
        };
        let mut matches = entries
            .iter()
            .enumerate()
            .filter_map(|(index, (key_id, value_id))| {
                let key_node = self.stream.node(*key_id)?;
                match key_node.kind() {
                    LosslessNodeKind::Scalar { value, .. } if value == key => Some((
                        index,
                        MappingEntry {
                            key: *key_id,
                            value: *value_id,
                        },
                    )),
                    _ => None,
                }
            });
        let Some(entry) = matches.next() else {
            return Err(Error::new(
                format!("lossless mapping entry {key:?} was not found"),
                Some(mapping_node.span()),
            ));
        };
        if matches.next().is_some() {
            return Err(Error::new(
                format!("lossless mapping entry {key:?} is ambiguous"),
                Some(mapping_node.span()),
            ));
        }
        Ok(entry)
    }

    fn sequence_node(&self, sequence: NodeId) -> Result<&LosslessNode> {
        let node = self
            .stream
            .node(sequence)
            .ok_or_else(|| Error::new("lossless sequence node id is out of bounds", None))?;
        if !matches!(node.kind(), LosslessNodeKind::Sequence { .. }) {
            return Err(Error::new(
                "lossless structural edit target is not a sequence",
                Some(node.span()),
            ));
        }
        Ok(node)
    }

    fn sequence_item(&self, sequence: NodeId, index: usize) -> Result<NodeId> {
        let sequence_node = self.sequence_node(sequence)?;
        let LosslessNodeKind::Sequence { children, .. } = sequence_node.kind() else {
            unreachable!("sequence_node only returns sequence nodes");
        };
        let Some(item) = children.get(index).copied() else {
            return Err(Error::new(
                format!(
                    "lossless sequence item index {index} is out of bounds for {} items",
                    children.len()
                ),
                Some(sequence_node.span()),
            ));
        };
        Ok(item)
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

    fn flow_collection_closing_offset(&self, node: &LosslessNode, delimiter: u8) -> Result<usize> {
        let span = node.span();
        let Some(offset) = span.end.checked_sub(1) else {
            return Err(Error::new(
                "lossless flow collection span is empty",
                Some(span),
            ));
        };
        if self.stream.source.as_bytes().get(offset) != Some(&delimiter) {
            return Err(Error::new(
                "lossless flow collection closing delimiter was not found",
                Some(span),
            ));
        }
        Ok(offset)
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

#[derive(Clone, Copy, Debug)]
struct MappingEntry {
    key: NodeId,
    value: NodeId,
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
///
/// The trivia text is not stored as an independent copy; instead the trivia
/// holds a shared handle to the retained source and borrows its text from the
/// [`span`](LosslessTrivia::span). This keeps the document bytes stored once.
#[derive(Clone, Debug)]
pub struct LosslessTrivia {
    kind: LosslessTriviaKind,
    span: Span,
    source: Arc<str>,
}

// Equality is defined over the observable fields (`kind`, `span`, `text()`)
// rather than the backing `source` handle, so two trivia with the same kind,
// span, and text compare equal regardless of which document's bytes back them.
// Deriving `PartialEq` would instead compare the entire shared source.
impl PartialEq for LosslessTrivia {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.span == other.span && self.text() == other.text()
    }
}

impl Eq for LosslessTrivia {}

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
        // The span is constructed in `scan_trivia` to point at the exact text
        // range within the retained source, so this slice always succeeds.
        self.source
            .get(self.span.start..self.span.end)
            .unwrap_or("")
    }
}

struct Builder {
    source: Arc<str>,
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
    fn new(source: Arc<str>, events: Vec<Event>, trivia: Vec<LosslessTrivia>) -> Self {
        Self {
            source,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResolvedConfigStep {
    Key(String),
    Index(usize),
}

fn decode_json_pointer_token(token: &str) -> Result<String> {
    let mut decoded = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '~' {
            decoded.push(ch);
            continue;
        }
        match chars.next() {
            Some('0') => decoded.push('~'),
            Some('1') => decoded.push('/'),
            Some(other) => {
                return Err(Error::new(
                    format!("invalid JSON Pointer escape '~{other}' in config path"),
                    None,
                ));
            }
            None => {
                return Err(Error::new(
                    "invalid trailing '~' escape in config JSON Pointer path",
                    None,
                ));
            }
        }
    }
    Ok(decoded)
}

fn document_root(stream: &LosslessStream, document: usize) -> Result<NodeId> {
    let doc = stream.documents().get(document).ok_or_else(|| {
        Error::new(
            format!("config document index {document} is out of range"),
            None,
        )
    })?;
    doc.root().ok_or_else(|| {
        Error::new(
            format!("config document index {document} has no root node"),
            None,
        )
    })
}

fn resolve_config_path(
    stream: &LosslessStream,
    document: usize,
    path: &ConfigPath,
) -> Result<NodeId> {
    let mut current = document_root(stream, document)?;
    for (depth, segment) in path.segments.iter().enumerate() {
        current = resolve_config_child(stream, current, segment, depth)?;
    }
    Ok(current)
}

fn resolve_config_parent(
    stream: &LosslessStream,
    document: usize,
    path: &ConfigPath,
) -> Result<(NodeId, ResolvedConfigStep)> {
    let Some((last, parent_segments)) = path.segments.split_last() else {
        return Err(Error::new(
            "config path operation requires a non-empty path",
            None,
        ));
    };
    let parent_path = ConfigPath {
        segments: parent_segments.to_vec(),
    };
    let parent = resolve_config_path(stream, document, &parent_path)?;
    let parent_node = stream
        .node(parent)
        .ok_or_else(|| Error::new("config path parent node id is out of bounds", None))?;
    let last = resolve_config_step_for_parent(parent_node, last, parent_segments.len())?;
    Ok((parent, last))
}

fn resolve_config_child(
    stream: &LosslessStream,
    node: NodeId,
    segment: &ConfigPathStep,
    depth: usize,
) -> Result<NodeId> {
    let current = stream
        .node(node)
        .ok_or_else(|| Error::new("config path node id is out of bounds", None))?;
    match resolve_config_step_for_parent(current, segment, depth)? {
        ResolvedConfigStep::Key(key) => resolve_mapping_key(stream, current, &key, depth),
        ResolvedConfigStep::Index(index) => resolve_sequence_index(current, index, depth),
    }
}

fn resolve_config_step_for_parent(
    parent: &LosslessNode,
    segment: &ConfigPathStep,
    depth: usize,
) -> Result<ResolvedConfigStep> {
    match (segment, parent.kind()) {
        (ConfigPathStep::Key(key), LosslessNodeKind::Mapping { .. }) => {
            Ok(ResolvedConfigStep::Key(key.clone()))
        }
        (ConfigPathStep::Index(index), LosslessNodeKind::Sequence { .. }) => {
            Ok(ResolvedConfigStep::Index(*index))
        }
        (ConfigPathStep::PointerToken(token), LosslessNodeKind::Mapping { .. }) => {
            Ok(ResolvedConfigStep::Key(token.clone()))
        }
        (ConfigPathStep::PointerToken(token), LosslessNodeKind::Sequence { .. }) => {
            let index = token.parse::<usize>().map_err(|_| {
                Error::new(
                    format!("config path segment {depth} token {token:?} is not a sequence index"),
                    Some(parent.span()),
                )
            })?;
            Ok(ResolvedConfigStep::Index(index))
        }
        (ConfigPathStep::Key(key), _) => Err(Error::new(
            format!("config path segment {depth} key {key:?} requires a mapping node"),
            Some(parent.span()),
        )),
        (ConfigPathStep::Index(index), _) => Err(Error::new(
            format!("config path segment {depth} index {index} requires a sequence node"),
            Some(parent.span()),
        )),
        (ConfigPathStep::PointerToken(token), _) => Err(Error::new(
            format!(
                "config path segment {depth} token {token:?} requires a mapping or sequence node"
            ),
            Some(parent.span()),
        )),
    }
}

fn resolve_mapping_key(
    stream: &LosslessStream,
    current: &LosslessNode,
    key: &str,
    depth: usize,
) -> Result<NodeId> {
    let LosslessNodeKind::Mapping { entries, .. } = current.kind() else {
        return Err(Error::new(
            format!("config path segment {depth} key {key:?} requires a mapping node"),
            Some(current.span()),
        ));
    };
    let mut matches =
        entries
            .iter()
            .filter_map(|(key_id, value_id)| match stream.node(*key_id)?.kind() {
                LosslessNodeKind::Scalar { value, .. } if value == key => Some(*value_id),
                _ => None,
            });
    let Some(value) = matches.next() else {
        return Err(Error::new(
            format!("config path segment {depth} key {key:?} was not found"),
            Some(current.span()),
        ));
    };
    if matches.next().is_some() {
        return Err(Error::new(
            format!("config path segment {depth} key {key:?} is ambiguous"),
            Some(current.span()),
        ));
    }
    Ok(value)
}

fn resolve_sequence_index(current: &LosslessNode, index: usize, depth: usize) -> Result<NodeId> {
    let LosslessNodeKind::Sequence { children, .. } = current.kind() else {
        return Err(Error::new(
            format!("config path segment {depth} index {index} requires a sequence node"),
            Some(current.span()),
        ));
    };
    children.get(index).copied().ok_or_else(|| {
        Error::new(
            format!(
                "config path segment {depth} index {index} is out of bounds for a sequence of length {}",
                children.len()
            ),
            Some(current.span()),
        )
    })
}

fn set_path_source<T>(
    stream: &LosslessStream,
    document: usize,
    path: &ConfigPath,
    value: &T,
    options: LoadOptions,
) -> Result<String>
where
    T: Serialize,
{
    if path.is_empty() {
        let root = document_root(stream, document)?;
        let replacement = serialize_edit_fragment(value, EmitOptions::structural())?;
        let mut edit = stream.edit();
        edit.replace_node_source(root, replacement)?;
        return edit.finish_with_options(options);
    }
    let (parent, last) = resolve_config_parent(stream, document, path)?;
    let parent_node = stream
        .node(parent)
        .ok_or_else(|| Error::new("config path parent node id is out of bounds", None))?;
    match (last, parent_node.kind()) {
        (ResolvedConfigStep::Key(key), LosslessNodeKind::Mapping { style, .. }) => {
            let replacement = serialize_value_for_style(value, *style)?;
            let mut edit = stream.edit();
            match style {
                CollectionStyle::Block => {
                    replace_block_mapping_value_source(
                        &mut edit,
                        parent,
                        &key,
                        &replacement,
                        options,
                    )?;
                }
                CollectionStyle::Flow => {
                    edit.replace_mapping_value_source(parent, &key, replacement)?;
                }
            }
            edit.finish_with_options(options)
        }
        (ResolvedConfigStep::Index(index), LosslessNodeKind::Sequence { style, .. }) => {
            let replacement = serialize_value_for_style(value, *style)?;
            let mut edit = stream.edit();
            edit.replace_sequence_item_source_with_options(parent, index, replacement, options)?;
            edit.finish_with_options(options)
        }
        (ResolvedConfigStep::Key(key), _) => Err(Error::new(
            format!("config set of key {key:?} requires a mapping parent"),
            Some(parent_node.span()),
        )),
        (ResolvedConfigStep::Index(index), _) => Err(Error::new(
            format!("config set of index {index} requires a sequence parent"),
            Some(parent_node.span()),
        )),
    }
}

fn replace_block_mapping_value_source(
    edit: &mut LosslessEdit<'_>,
    mapping: NodeId,
    key: &str,
    replacement: &str,
    options: LoadOptions,
) -> Result<()> {
    let mapping_node = edit.mapping_node(mapping)?;
    let LosslessNodeKind::Mapping { style, .. } = mapping_node.kind() else {
        unreachable!("mapping_node only returns mapping nodes");
    };
    if *style != CollectionStyle::Block {
        return Err(Error::new(
            "config block mapping value replacement requires a block mapping",
            Some(mapping_node.span()),
        ));
    }

    let entry = edit.unique_mapping_entry_by_key(mapping, key)?;
    let key_node = edit
        .stream
        .node(entry.key)
        .ok_or_else(|| Error::new("config mapping key node id is out of bounds", None))?;
    let value_node = edit
        .stream
        .node(entry.value)
        .ok_or_else(|| Error::new("config mapping value node id is out of bounds", None))?;
    let replacement_root = single_fragment_root_with_options(
        replacement,
        value_node.span(),
        "mapping value source",
        options,
    )?;
    let needs_nested_block_value =
        replacement.contains('\n') || is_nested_block_value_fragment(&replacement_root);

    if !needs_nested_block_value {
        edit.replace_mapping_value_source(mapping, key, replacement.to_owned())?;
        return Ok(());
    }

    let source = &edit.stream.source;
    let key_line_start = line_start(source, key_node.span().start);
    let value_line_start = line_start(source, value_node.span().start);
    let value_line_end = line_end_including_newline(source, value_node.span().end);
    let (span_start, formatted) = if value_line_start == key_line_start {
        let line_body_end = line_body_end(source, value_line_end);
        let span_start = block_mapping_value_after_colon(source, key_node, line_body_end)?;
        let trailing = source
            .get(value_node.span().end..line_body_end)
            .ok_or_else(|| Error::new("config mapping trailing comment span is invalid", None))?;
        let mut formatted = String::new();
        let trailing = trailing.trim_start();
        if trailing.starts_with('#') {
            formatted.push(' ');
            formatted.push_str(trailing.trim_end());
        }
        formatted.push('\n');
        let child_indent = format!("{}  ", line_indent(source, key_line_start));
        formatted.push_str(&indent_entry_source(replacement, &child_indent));
        (span_start, formatted)
    } else {
        (
            value_line_start,
            indent_entry_source(replacement, line_indent(source, value_line_start)),
        )
    };
    let span = edit.stream.source_span(span_start, value_line_end)?;
    edit.replace_source_span(span, formatted)?;
    Ok(())
}

fn remove_path_source(
    stream: &LosslessStream,
    document: usize,
    path: &ConfigPath,
    options: LoadOptions,
) -> Result<String> {
    let (parent, last) = resolve_config_parent(stream, document, path)?;
    let parent_node = stream
        .node(parent)
        .ok_or_else(|| Error::new("config path parent node id is out of bounds", None))?;
    let mut edit = stream.edit();
    match (last, parent_node.kind()) {
        (ResolvedConfigStep::Key(key), LosslessNodeKind::Mapping { style, .. }) => match style {
            CollectionStyle::Block => match parent_node.kind() {
                LosslessNodeKind::Mapping { entries, .. } if entries.len() == 1 => {
                    edit.replace_node_source(parent, "{}")?;
                }
                _ => {
                    edit.delete_block_mapping_entry_source(parent, &key)?;
                }
            },
            CollectionStyle::Flow => {
                edit.delete_flow_mapping_entry_source(parent, &key)?;
            }
        },
        (ResolvedConfigStep::Index(index), LosslessNodeKind::Sequence { style, .. }) => match style
        {
            CollectionStyle::Block => match parent_node.kind() {
                LosslessNodeKind::Sequence { children, .. } if children.len() == 1 => {
                    edit.replace_node_source(parent, "[]")?;
                }
                _ => {
                    edit.delete_block_sequence_item_source(parent, index)?;
                }
            },
            CollectionStyle::Flow => {
                edit.delete_flow_sequence_item_source(parent, index)?;
            }
        },
        (ResolvedConfigStep::Key(key), _) => {
            return Err(Error::new(
                format!("config remove of key {key:?} requires a mapping parent"),
                Some(parent_node.span()),
            ));
        }
        (ResolvedConfigStep::Index(index), _) => {
            return Err(Error::new(
                format!("config remove of index {index} requires a sequence parent"),
                Some(parent_node.span()),
            ));
        }
    };
    edit.finish_with_options(options)
}

fn rename_path_source(
    stream: &LosslessStream,
    document: usize,
    path: &ConfigPath,
    new_key_source: &str,
    options: LoadOptions,
) -> Result<String> {
    let (parent, last) = resolve_config_parent(stream, document, path)?;
    let ResolvedConfigStep::Key(old_key) = last else {
        return Err(Error::new(
            "config rename requires a mapping key path",
            stream.node(parent).map(LosslessNode::span),
        ));
    };
    let entry = stream
        .edit()
        .unique_mapping_entry_by_key(parent, &old_key)?;
    let key_node = stream
        .node(entry.key)
        .ok_or_else(|| Error::new("config rename key node id is out of bounds", None))?;
    let mut edit = stream.edit();
    edit.replace_node_source(key_node.id(), new_key_source.to_owned())?;
    edit.finish_with_options(options)
}

fn insert_entry_source<T>(
    stream: &LosslessStream,
    document: usize,
    mapping_path: &ConfigPath,
    key: &str,
    value: &T,
    options: LoadOptions,
) -> Result<String>
where
    T: Serialize,
{
    let mapping = resolve_config_path(stream, document, mapping_path)?;
    let mapping_node = stream
        .node(mapping)
        .ok_or_else(|| Error::new("config mapping path node id is out of bounds", None))?;
    let LosslessNodeKind::Mapping { style, .. } = mapping_node.kind() else {
        return Err(Error::new(
            "config insert requires a mapping node",
            Some(mapping_node.span()),
        ));
    };
    let entry_source = format_typed_entry_source(key, value, *style, options)?;
    let mut edit = stream.edit();
    match style {
        CollectionStyle::Block => {
            edit.insert_block_mapping_entry_source_with_options(mapping, entry_source, options)?
        }
        CollectionStyle::Flow => {
            edit.insert_flow_mapping_entry_source_with_options(mapping, entry_source, options)?
        }
    };
    edit.finish_with_options(options)
}

fn push_item_source<T>(
    stream: &LosslessStream,
    document: usize,
    sequence_path: &ConfigPath,
    value: &T,
    options: LoadOptions,
) -> Result<String>
where
    T: Serialize,
{
    let sequence = resolve_config_path(stream, document, sequence_path)?;
    let sequence_node = stream
        .node(sequence)
        .ok_or_else(|| Error::new("config sequence path node id is out of bounds", None))?;
    let LosslessNodeKind::Sequence { children, .. } = sequence_node.kind() else {
        return Err(Error::new(
            "config push requires a sequence node",
            Some(sequence_node.span()),
        ));
    };
    insert_item_source(
        stream,
        document,
        sequence_path,
        children.len(),
        value,
        options,
    )
}

fn insert_item_source<T>(
    stream: &LosslessStream,
    document: usize,
    sequence_path: &ConfigPath,
    index: usize,
    value: &T,
    options: LoadOptions,
) -> Result<String>
where
    T: Serialize,
{
    let sequence = resolve_config_path(stream, document, sequence_path)?;
    let sequence_node = stream
        .node(sequence)
        .ok_or_else(|| Error::new("config sequence path node id is out of bounds", None))?;
    let LosslessNodeKind::Sequence { style, .. } = sequence_node.kind() else {
        return Err(Error::new(
            "config item insertion requires a sequence node",
            Some(sequence_node.span()),
        ));
    };
    let item_source = serialize_value_for_style(value, *style)?;
    let mut edit = stream.edit();
    match style {
        CollectionStyle::Block => edit.insert_block_sequence_item_source_with_options(
            sequence,
            index,
            item_source,
            options,
        )?,
        CollectionStyle::Flow => edit.insert_flow_sequence_item_source_with_options(
            sequence,
            index,
            item_source,
            options,
        )?,
    };
    edit.finish_with_options(options)
}

fn serialize_key_fragment(key: &str) -> Result<String> {
    let source = serialize_edit_fragment(
        &key,
        EmitOptions::structural().with_collection_style(EmitCollectionStyle::Flow),
    )?;
    if source.contains('\n') {
        return Err(Error::new(
            "config mapping keys must emit as a single YAML scalar",
            None,
        ));
    }
    ensure_scalar_fragment(&source, Span::default())?;
    Ok(source)
}

fn serialize_value_for_style<T>(value: &T, style: CollectionStyle) -> Result<String>
where
    T: Serialize,
{
    let options = match style {
        CollectionStyle::Block => EmitOptions::structural(),
        CollectionStyle::Flow => {
            EmitOptions::structural().with_collection_style(EmitCollectionStyle::Flow)
        }
    };
    let source = serialize_edit_fragment(value, options)?;
    if style == CollectionStyle::Flow && source.contains('\n') {
        return Err(Error::new(
            "config edit for flow collections requires inline YAML output",
            None,
        ));
    }
    Ok(source)
}

fn serialize_edit_fragment<T>(value: &T, options: EmitOptions) -> Result<String>
where
    T: Serialize,
{
    let mut source = crate::to_string_with_options(value, options)?;
    if source.ends_with('\n') {
        source.pop();
    }
    Ok(source)
}

fn format_typed_entry_source<T>(
    key: &str,
    value: &T,
    style: CollectionStyle,
    options: LoadOptions,
) -> Result<String>
where
    T: Serialize,
{
    let key_source = serialize_key_fragment(key)?;
    let value_source = serialize_value_for_style(value, style)?;
    let needs_nested_block_value = if style == CollectionStyle::Block {
        let value_root = single_fragment_root_with_options(
            &value_source,
            Span::default(),
            "mapping entry value source",
            options,
        )?;
        is_nested_block_value_fragment(&value_root)
    } else {
        false
    };
    if value_source.contains('\n') || needs_nested_block_value {
        let mut entry = String::with_capacity(key_source.len() + value_source.len() + 4);
        entry.push_str(&key_source);
        entry.push_str(":\n");
        entry.push_str(&indent_fragment(&value_source, "  "));
        Ok(entry)
    } else {
        Ok(format!("{key_source}: {value_source}"))
    }
}

fn indent_fragment(source: &str, indent: &str) -> String {
    let mut output = String::with_capacity(source.len() + indent.len() * 2);
    for line in source.split_inclusive('\n') {
        if line == "\n" {
            output.push('\n');
        } else {
            output.push_str(indent);
            output.push_str(line);
        }
    }
    output
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

fn scan_trivia(input: &Arc<str>) -> Vec<LosslessTrivia> {
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
                source: Arc::clone(input),
            });
        } else if let Some(comment) = comment_start(raw_body) {
            let start = bom_len + comment;
            trivia.push(LosslessTrivia {
                kind: LosslessTriviaKind::Comment,
                span: Span::new(offset + start, offset + raw.len(), line, start + 1),
                source: Arc::clone(input),
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

fn ensure_single_mapping_entry_fragment_with_options(
    entry_source: &str,
    span: Span,
    options: LoadOptions,
) -> Result<()> {
    let root =
        single_fragment_root_with_options(entry_source, span, "mapping entry source", options)?;
    match root.kind() {
        LosslessNodeKind::Mapping { entries, .. } if entries.len() == 1 => Ok(()),
        _ => Err(Error::new(
            "mapping entry source must parse as exactly one mapping entry",
            Some(span),
        )),
    }
}

fn ensure_single_node_fragment_with_options(
    fragment: &str,
    span: Span,
    label: &str,
    options: LoadOptions,
) -> Result<()> {
    single_fragment_root_with_options(fragment, span, label, options).map(|_| ())
}

fn single_fragment_root_with_options(
    fragment: &str,
    span: Span,
    label: &str,
    options: LoadOptions,
) -> Result<LosslessNode> {
    let parsed = parse_lossless_with_options(fragment, options)
        .map_err(|error| Error::new(format!("{label} is not valid YAML: {error}"), Some(span)))?;
    if parsed.documents().len() != 1 {
        return Err(Error::new(
            format!("{label} must parse as one YAML document"),
            Some(span),
        ));
    }
    let root = parsed.documents()[0]
        .root()
        .ok_or_else(|| Error::new(format!("{label} must parse as one YAML node"), Some(span)))?;
    let root = parsed
        .node(root)
        .cloned()
        .ok_or_else(|| Error::new(format!("{label} root node is missing"), Some(span)))?;
    Ok(root)
}

fn is_nested_block_value_fragment(node: &LosslessNode) -> bool {
    matches!(
        node.kind(),
        LosslessNodeKind::Mapping {
            style: CollectionStyle::Block,
            ..
        } | LosslessNodeKind::Sequence {
            style: CollectionStyle::Block,
            ..
        }
    )
}

fn is_implicit_null_mapping_value_node(stream: &LosslessStream, node: &LosslessNode) -> bool {
    matches!(
        node.kind(),
        LosslessNodeKind::Scalar {
            value,
            style: ScalarStyle::Plain,
        } if value == "null"
    ) && stream
        .source_fragment(node.span())
        .is_some_and(|fragment| fragment.starts_with(':') && fragment[1..].trim().is_empty())
}

fn block_mapping_value_after_colon(
    source: &str,
    key_node: &LosslessNode,
    line_body_end: usize,
) -> Result<usize> {
    let separator = source
        .get(key_node.span().end..line_body_end)
        .ok_or_else(|| Error::new("config mapping separator span is invalid", None))?;
    separator
        .find(':')
        .map(|offset| key_node.span().end + offset + 1)
        .ok_or_else(|| Error::new("config mapping separator colon is missing", None))
}

fn format_implicit_null_inline_replacement(replacement: &str, before_comment: bool) -> String {
    if replacement.is_empty() {
        return String::new();
    }

    let mut formatted = if replacement.starts_with(char::is_whitespace) {
        replacement.to_owned()
    } else {
        format!(" {replacement}")
    };
    if before_comment && !formatted.ends_with(char::is_whitespace) {
        formatted.push(' ');
    }
    formatted
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0)
}

fn line_end_including_newline(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[offset..]
        .find('\n')
        .map(|position| offset + position + 1)
        .unwrap_or(source.len())
}

fn line_body_end(source: &str, line_end: usize) -> usize {
    if line_end > 0 && source.as_bytes().get(line_end - 1) == Some(&b'\n') {
        let without_lf = line_end - 1;
        if without_lf > 0 && source.as_bytes().get(without_lf - 1) == Some(&b'\r') {
            without_lf - 1
        } else {
            without_lf
        }
    } else {
        line_end
    }
}

fn line_indent(source: &str, line_start: usize) -> &str {
    let line = &source[line_start..line_end_including_newline(source, line_start)];
    let indent_len = line
        .bytes()
        .take_while(|byte| matches!(*byte, b' ' | b'\t'))
        .count();
    &line[..indent_len]
}

fn block_mapping_entry_indent(source: &str, key_start: usize) -> String {
    compact_sequence_mapping_key_indent(source, key_start).unwrap_or_else(|| {
        let line_start = line_start(source, key_start);
        line_indent(source, line_start).to_owned()
    })
}

fn compact_sequence_mapping_key_indent(source: &str, key_start: usize) -> Option<String> {
    let (_, prefix) = compact_sequence_mapping_key_prefix(source, key_start)?;
    Some(
        prefix
            .bytes()
            .map(|byte| if byte == b'\t' { '\t' } else { ' ' })
            .collect(),
    )
}

fn compact_sequence_mapping_key_prefix(source: &str, key_start: usize) -> Option<(usize, &str)> {
    let line_start = line_start(source, key_start);
    let prefix = source.get(line_start..key_start)?;
    let dash = prefix
        .bytes()
        .position(|byte| !matches!(byte, b' ' | b'\t'))?;
    if prefix.as_bytes().get(dash) != Some(&b'-') {
        return None;
    }
    let after_dash = prefix.as_bytes().get(dash + 1..)?;
    if after_dash.is_empty() || !after_dash.iter().all(|byte| matches!(*byte, b' ' | b'\t')) {
        return None;
    }
    Some((line_start + dash + 1, prefix))
}

fn indent_entry_source(entry_source: &str, indent: &str) -> String {
    let normalized = if entry_source.ends_with('\n') {
        entry_source.to_owned()
    } else {
        format!("{entry_source}\n")
    };
    let mut indented = String::with_capacity(normalized.len() + indent.len() * 2);
    for line in normalized.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if body.is_empty() {
            indented.push('\n');
        } else {
            indented.push_str(indent);
            indented.push_str(body);
            if line.ends_with('\n') {
                indented.push('\n');
            }
        }
    }
    indented
}

fn format_block_sequence_item_source(item_source: &str, indent: &str) -> String {
    let normalized = if item_source.ends_with('\n') {
        item_source.to_owned()
    } else {
        format!("{item_source}\n")
    };
    let mut formatted = String::with_capacity(normalized.len() + indent.len() * 2);
    let mut lines = normalized.split_inclusive('\n');
    let first = lines.next().unwrap_or("\n");
    let first_body = first.strip_suffix('\n').unwrap_or(first);
    formatted.push_str(indent);
    formatted.push_str("- ");
    formatted.push_str(first_body);
    if first.ends_with('\n') {
        formatted.push('\n');
    }
    let child_indent = format!("{indent}  ");
    for line in lines {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if body.is_empty() {
            formatted.push('\n');
        } else {
            formatted.push_str(&child_indent);
            formatted.push_str(body);
            if line.ends_with('\n') {
                formatted.push('\n');
            }
        }
    }
    formatted
}
