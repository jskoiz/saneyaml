//! Pure-Rust YAML parser, emitter, and Serde integration for
//! configuration-shaped YAML.
//!
//! The preview API focuses on YAML 1.2 parser events, loaded document trees
//! with default merge-key expansion, `serde_yaml`-style
//! `Value`/`Mapping`/`Number` workflows, typed Serde reads, structural writes,
//! and line/column diagnostics. See `MIGRATION.md`,
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
mod parse;
mod schema;
mod ser;

/// Serde helper modules matching selected `serde_yaml::with` paths.
pub mod with;

/// Mapping types and iterators for YAML [`Mapping`].
pub mod mapping {
    pub use crate::ast::{
        Entry, IntoIter, IntoKeys, IntoValues, Iter, IterMut, Keys, Mapping, MappingIndex as Index,
        OccupiedEntry, VacantEntry, Values, ValuesMut,
    };
}

/// Value-oriented API matching the `serde_yaml::value` module shape.
pub mod value {
    pub use crate::ast::{Index, Mapping, Number, Sequence, Tag, TaggedValue, Value};
    pub use crate::de::from_value;
    pub use crate::ser::{ValueSerializer as Serializer, to_value};
}

pub use ast::{
    Entry, Index, Mapping, Node, NodeValue, Number, OccupiedEntry, ScalarSource, Sequence, Tag,
    TaggedNode, TaggedValue, VacantEntry, Value,
};
pub use de::{
    Deserializer, from_documents_reader, from_documents_slice, from_documents_str, from_node,
    from_reader, from_slice, from_str, from_value,
};
pub use error::{Diagnostic, Error, Location, RelatedDiagnostic, Result, Span};
pub use parse::{
    CollectionStyle, Event, EventAnchor, EventDocumentDirectives, EventMeta, EventTag,
    EventTagDirective, EventYamlVersion, ScalarStyle, parse_bytes, parse_documents, parse_events,
    parse_str,
};
pub use schema::{LoadOptions, Schema};
pub use ser::{Serializer, to_string, to_value, to_writer};
