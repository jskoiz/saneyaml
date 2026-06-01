//! Pure-Rust YAML parser, emitter, and Serde integration for
//! configuration-shaped YAML.
//!
//! The preview API focuses on YAML 1.2 parser events, pull-based event and
//! document streaming, loaded document trees with default merge-key expansion,
//! explicit and directive-driven YAML 1.1 scalar construction options, `serde_yaml`-style
//! `Value`/`Mapping`/`Number` workflows, typed Serde reads, structural writes,
//! explicit emission fidelity tiers, and line/column diagnostics. See `MIGRATION.md`,
//! `COMPATIBILITY.md`, and `DEVELOPER_PREVIEW.md` for the current adoption
//! contract and intentional non-goals.
//!
//! ```rust
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     name: String,
//! }
//!
//! let config: Config = yaml::from_str("name: api\n")?;
//! assert_eq!(config.name, "api");
//! # Ok::<(), yaml::Error>(())
//! ```
//!
#![forbid(unsafe_code)]

mod ast;
mod de;
mod emit;
mod error;
mod key_identity;
pub mod lossless;
mod parse;
mod schema;
mod ser;
mod yaml11;

/// Serde helper modules matching selected `serde_yaml::with` paths.
pub mod with;

/// Pull-based YAML event and document streaming APIs.
pub mod stream {
    pub use crate::parse::{DocumentStream, EventStream};
}

/// Mapping types and iterators for YAML [`Mapping`].
pub mod mapping {
    pub use crate::ast::{
        Entry, IntoIter, IntoKeys, IntoValues, Iter, IterMut, Keys, Mapping, MappingIndex as Index,
        OccupiedEntry, VacantEntry, Values, ValuesMut,
    };
}

/// Value-oriented API matching the `serde_yaml::value` module shape.
pub mod value {
    pub use crate::ast::{
        Date, Index, Mapping, Number, Sequence, Tag, TaggedValue, Time, TimeZoneOffset, Timestamp,
        Value,
    };
    pub use crate::de::from_value;
    pub use crate::ser::{ValueSerializer as Serializer, to_value};
}

pub use ast::{
    BorrowedNode, BorrowedNodeValue, BorrowedTaggedNode, Date, Entry, Index, Mapping, Node,
    NodeValue, Number, OccupiedEntry, ScalarSource, Sequence, Tag, TaggedNode, TaggedValue, Time,
    TimeZoneOffset, Timestamp, VacantEntry, Value,
};
pub use de::{
    Deserializer, from_documents_reader, from_documents_slice, from_documents_str, from_node,
    from_reader, from_slice, from_str, from_value,
};
pub use emit::EmitOptions;
pub use error::{Diagnostic, Error, Location, RelatedDiagnostic, Result, Span};
pub use lossless::{
    AliasId, AnchorId, LosslessAlias, LosslessAnchor, LosslessDocument, LosslessEdit,
    LosslessEffectiveMappingEntry, LosslessEffectiveMappingSource, LosslessNode, LosslessNodeKind,
    LosslessStream, LosslessTrivia, LosslessTriviaKind, NodeId, PathSegment, parse_lossless,
    parse_lossless_bytes,
};
pub use parse::{
    CollectionStyle, DocumentStream, Event, EventAnchor, EventDocumentDirectives, EventMeta,
    EventStream, EventTag, EventTagDirective, EventYamlVersion, ScalarStyle,
    parse_borrowed_documents, parse_bytes, parse_documents, parse_events, parse_str,
    stream_documents, stream_documents_reader, stream_documents_slice, stream_events,
    stream_events_reader, stream_events_slice,
};
pub use schema::{
    DEFAULT_ALIAS_EXPANSION_FACTOR, DEFAULT_MAX_INPUT_BYTES, DEFAULT_MIN_ALIAS_EXPANSION_NODES,
    LoadOptions, Schema,
};
pub use ser::{
    Serializer, to_string, to_string_with_options, to_value, to_writer, to_writer_with_options,
};
