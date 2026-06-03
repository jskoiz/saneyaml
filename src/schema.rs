//! Load options and schema selection for constructed YAML document trees.
//!
//! ```rust
//! let value: yaml::Value = yaml::LoadOptions::legacy_serde_yaml()
//!     .max_input_bytes(1024)
//!     .from_str("flag: on\n")?;
//! assert_eq!(value.get("flag").and_then(yaml::Value::as_bool), Some(true));
//! # Ok::<(), yaml::Error>(())
//! ```

use crate::{BorrowedNode, Error, Node, Result, Span, de, parse};
use serde::de::DeserializeOwned;
use std::io::Read;

/// Default maximum YAML input size accepted by loading entrypoints.
pub const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Default alias expansion budget multiplier per input byte.
pub const DEFAULT_ALIAS_EXPANSION_FACTOR: usize = 64;

/// Minimum alias expansion budget used by default loading options.
pub const DEFAULT_MIN_ALIAS_EXPANSION_NODES: usize = 1024;

/// Default maximum constructed YAML nesting depth accepted by loading entrypoints.
pub const DEFAULT_MAX_NESTING_DEPTH: usize = 128;

/// Default maximum resolved scalar size accepted by loading entrypoints.
///
/// The 1 MiB ceiling leaves room for unusually large but plausible config
/// values while rejecting scalar bombs well below the global input ceiling.
pub const DEFAULT_MAX_SCALAR_BYTES: usize = 1024 * 1024;

/// Default maximum number of entries accepted in one sequence or mapping.
///
/// This is a per-sequence or per-mapping-pair limit. It is intentionally much
/// lower than the input byte ceiling so compact wide collections cannot force
/// unbounded construction work by default.
pub const DEFAULT_MAX_COLLECTION_ITEMS: usize = 16 * 1024;

/// Scalar construction schema used by tree and Serde loading.
///
/// `Yaml12` is the default-compatible spelling and uses the same YAML
/// 1.2-oriented config behavior as [`Schema::Core`]. `Yaml11` is the legacy
/// spelling for [`Schema::LegacySerdeYaml`]. The retained versioned names keep
/// existing call sites working while the named modes make scalar resolution
/// choices explicit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Schema {
    /// YAML 1.2-oriented core schema used by the default entrypoints.
    #[default]
    Yaml12,
    /// Explicit YAML 1.2 Core-compatible construction.
    Core,
    /// YAML 1.2 JSON schema construction.
    Json,
    /// YAML Failsafe construction, leaving every scalar as a string.
    Failsafe,
    /// Explicit YAML 1.1 compatibility schema for legacy configuration files.
    Yaml11,
    /// Legacy libyaml/serde_yaml-era construction for migration call sites.
    LegacySerdeYaml,
    /// Selects scalar construction from each document's `%YAML` version directive.
    ///
    /// Documents with `%YAML 1.1` use [`Schema::LegacySerdeYaml`]. Documents
    /// without a version directive, with `%YAML 1.2`, or with newer numeric
    /// versions use [`Schema::Yaml12`].
    YamlVersionDirective,
}

impl Schema {
    pub(crate) const fn is_legacy_compatible(self) -> bool {
        matches!(self, Self::Yaml11 | Self::LegacySerdeYaml)
    }
}

/// Options for loading YAML into constructed trees or Serde values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadOptions {
    pub(crate) schema: Schema,
    max_input_bytes: Option<usize>,
    max_alias_expansion_nodes: Option<usize>,
    max_nesting_depth: Option<usize>,
    max_scalar_bytes: Option<usize>,
    max_collection_items: Option<usize>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadOptions {
    /// Creates default load options.
    pub const fn new() -> Self {
        Self {
            schema: Schema::Yaml12,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options using explicit YAML 1.2 Core-compatible construction.
    pub const fn core() -> Self {
        Self {
            schema: Schema::Core,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options using YAML 1.2 JSON schema construction.
    pub const fn json() -> Self {
        Self {
            schema: Schema::Json,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options using YAML Failsafe construction.
    pub const fn failsafe() -> Self {
        Self {
            schema: Schema::Failsafe,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options using explicit YAML 1.1 compatibility construction.
    pub const fn yaml_1_1() -> Self {
        Self {
            schema: Schema::Yaml11,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options using legacy libyaml/serde_yaml-era construction.
    pub const fn legacy_serde_yaml() -> Self {
        Self {
            schema: Schema::LegacySerdeYaml,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Creates load options that follow each document's `%YAML` version directive.
    ///
    /// `%YAML 1.1` documents use YAML 1.1 compatibility construction. Documents
    /// without a version directive, with `%YAML 1.2`, or with newer numeric
    /// versions use the YAML 1.2-oriented default construction.
    pub const fn yaml_version_directive() -> Self {
        Self {
            schema: Schema::YamlVersionDirective,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
            max_nesting_depth: Some(DEFAULT_MAX_NESTING_DEPTH),
            max_scalar_bytes: Some(DEFAULT_MAX_SCALAR_BYTES),
            max_collection_items: Some(DEFAULT_MAX_COLLECTION_ITEMS),
        }
    }

    /// Returns options with the selected scalar construction schema.
    pub const fn schema(mut self, schema: Schema) -> Self {
        self.schema = schema;
        self
    }

    /// Returns the selected scalar construction schema.
    pub const fn selected_schema(self) -> Schema {
        self.schema
    }

    /// Returns options with a maximum input size in bytes.
    pub const fn max_input_bytes(mut self, max_input_bytes: usize) -> Self {
        self.max_input_bytes = Some(max_input_bytes);
        self
    }

    /// Returns options without an input size limit.
    pub const fn without_input_limit(mut self) -> Self {
        self.max_input_bytes = None;
        self
    }

    /// Returns the configured maximum input size in bytes.
    pub const fn selected_max_input_bytes(self) -> Option<usize> {
        self.max_input_bytes
    }

    /// Returns options with a maximum number of alias-expanded nodes.
    ///
    /// The default budget remains input-size derived. This option lets callers
    /// loading untrusted configuration tighten that expansion work directly.
    pub const fn max_alias_expansion_nodes(mut self, max_alias_expansion_nodes: usize) -> Self {
        self.max_alias_expansion_nodes = Some(max_alias_expansion_nodes);
        self
    }

    /// Returns the configured maximum number of alias-expanded nodes.
    ///
    /// `None` means the default input-size-derived budget is selected.
    pub const fn selected_max_alias_expansion_nodes(self) -> Option<usize> {
        self.max_alias_expansion_nodes
    }

    /// Returns options with a maximum constructed YAML nesting depth.
    pub const fn max_nesting_depth(mut self, max_nesting_depth: usize) -> Self {
        self.max_nesting_depth = Some(max_nesting_depth);
        self
    }

    /// Returns options without a constructed nesting-depth limit.
    pub const fn without_nesting_depth_limit(mut self) -> Self {
        self.max_nesting_depth = None;
        self
    }

    /// Returns the configured maximum constructed nesting depth.
    pub const fn selected_max_nesting_depth(self) -> Option<usize> {
        self.max_nesting_depth
    }

    /// Returns options with a maximum resolved scalar size in bytes.
    pub const fn max_scalar_bytes(mut self, max_scalar_bytes: usize) -> Self {
        self.max_scalar_bytes = Some(max_scalar_bytes);
        self
    }

    /// Returns options without a resolved scalar-size limit.
    pub const fn without_scalar_limit(mut self) -> Self {
        self.max_scalar_bytes = None;
        self
    }

    /// Returns the configured maximum resolved scalar size in bytes.
    pub const fn selected_max_scalar_bytes(self) -> Option<usize> {
        self.max_scalar_bytes
    }

    /// Returns options with a maximum number of entries per sequence or mapping.
    pub const fn max_collection_items(mut self, max_collection_items: usize) -> Self {
        self.max_collection_items = Some(max_collection_items);
        self
    }

    /// Returns options without a per-collection item limit.
    pub const fn without_collection_limit(mut self) -> Self {
        self.max_collection_items = None;
        self
    }

    /// Returns the configured maximum number of entries per sequence or mapping.
    pub const fn selected_max_collection_items(self) -> Option<usize> {
        self.max_collection_items
    }

    pub(crate) fn alias_expansion_budget(self, input_len: usize) -> usize {
        self.max_alias_expansion_nodes.unwrap_or_else(|| {
            input_len
                .saturating_mul(DEFAULT_ALIAS_EXPANSION_FACTOR)
                .max(DEFAULT_MIN_ALIAS_EXPANSION_NODES)
        })
    }

    pub(crate) fn check_input_len(self, len: usize) -> Result<()> {
        if let Some(max) = self.max_input_bytes
            && len > max
        {
            return Err(self.input_limit_error());
        }
        Ok(())
    }

    pub(crate) fn input_limit_error(self) -> Error {
        let max = self
            .max_input_bytes
            .expect("input_limit_error requires a configured limit");
        Error::limit(
            format!("YAML input exceeds configured limit of {max} bytes"),
            Span::default(),
        )
    }

    pub(crate) fn check_nesting_depth(self, depth: usize, span: Span) -> Result<()> {
        if self.max_nesting_depth.is_some_and(|max| depth > max) {
            return Err(Error::limit("maximum YAML nesting depth exceeded", span));
        }
        Ok(())
    }

    pub(crate) fn check_scalar_bytes(self, len: usize, span: Span) -> Result<()> {
        if let Some(max) = self.max_scalar_bytes
            && len > max
        {
            return Err(Error::limit(
                format!("YAML scalar exceeds configured limit of {max} bytes"),
                span,
            ));
        }
        Ok(())
    }

    pub(crate) fn check_collection_items(self, len: usize, span: Span) -> Result<()> {
        if let Some(max) = self.max_collection_items
            && len > max
        {
            return Err(Error::limit(
                format!("YAML collection exceeds configured limit of {max} entries"),
                span,
            ));
        }
        Ok(())
    }

    /// Parses a single UTF-8 YAML document from bytes using these options.
    pub fn parse_bytes(self, input: &[u8]) -> Result<Node> {
        parse::parse_bytes_with_options(input, self)
    }

    /// Parses a single YAML document from a string using these options.
    pub fn parse_str(self, input: &str) -> Result<Node> {
        parse::parse_str_with_options(input, self)
    }

    /// Parses all documents in a YAML stream using these options.
    pub fn parse_documents(self, input: &str) -> Result<Vec<Node>> {
        parse::parse_documents_with_options(input, self)
    }

    /// Parses all documents into spanless trees that can borrow scalar strings from `input`.
    pub fn parse_borrowed_documents<'de>(self, input: &'de str) -> Result<Vec<BorrowedNode<'de>>> {
        parse::parse_borrowed_documents_with_options(input, self)
    }

    /// Creates a pull-based raw event stream using these options.
    pub fn stream_events(self, input: &str) -> Result<parse::EventStream> {
        parse::EventStream::from_str_with_options(input, self)
    }

    /// Creates a pull-based raw event stream from UTF-8 bytes using these options.
    pub fn stream_events_slice(self, input: &[u8]) -> Result<parse::EventStream> {
        parse::EventStream::from_slice_with_options(input, self)
    }

    /// Reads YAML bytes and creates a pull-based raw event stream using these options.
    pub fn stream_events_reader<R>(self, reader: R) -> Result<parse::EventStream>
    where
        R: Read,
    {
        parse::EventStream::from_reader_with_options(reader, self)
    }

    /// Creates a pull-based parsed document stream using these options.
    pub fn stream_documents(self, input: &str) -> Result<parse::DocumentStream> {
        parse::DocumentStream::from_str_with_options(input, self)
    }

    /// Creates a pull-based parsed document stream from UTF-8 bytes using these options.
    pub fn stream_documents_slice(self, input: &[u8]) -> Result<parse::DocumentStream> {
        parse::DocumentStream::from_slice_with_options(input, self)
    }

    /// Reads YAML bytes and creates a pull-based parsed document stream using these options.
    pub fn stream_documents_reader<R>(self, reader: R) -> Result<parse::DocumentStream>
    where
        R: Read,
    {
        parse::DocumentStream::from_reader_with_options(reader, self)
    }

    /// Deserializes a single YAML document from a string using these options.
    pub fn from_str<'de, T>(self, input: &'de str) -> Result<T>
    where
        T: serde::Deserialize<'de>,
    {
        de::from_str_with_options(input, self)
    }

    /// Deserializes a single UTF-8 YAML document from bytes using these options.
    pub fn from_slice<'de, T>(self, input: &'de [u8]) -> Result<T>
    where
        T: serde::Deserialize<'de>,
    {
        de::from_slice_with_options(input, self)
    }

    /// Reads all bytes from a reader and deserializes one YAML document.
    pub fn from_reader<R, T>(self, reader: R) -> Result<T>
    where
        R: Read,
        T: DeserializeOwned,
    {
        de::from_reader_with_options(reader, self)
    }

    /// Deserializes every document in a YAML stream from a string.
    pub fn from_documents_str<T>(self, input: &str) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        de::from_documents_str_with_options(input, self)
    }

    /// Deserializes every document in a UTF-8 YAML stream from bytes.
    pub fn from_documents_slice<T>(self, input: &[u8]) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        de::from_documents_slice_with_options(input, self)
    }

    /// Reads all bytes from a reader and deserializes every YAML document.
    pub fn from_documents_reader<T, R>(self, reader: R) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
        R: Read,
    {
        de::from_documents_reader_with_options(reader, self)
    }

    /// Creates a streaming Serde deserializer from a YAML string.
    pub fn deserializer_from_str<'de>(self, input: &'de str) -> de::Deserializer<'de> {
        de::Deserializer::from_str_with_options(input, self)
    }

    /// Creates a streaming Serde deserializer from UTF-8 YAML bytes.
    pub fn deserializer_from_slice<'de>(self, input: &'de [u8]) -> de::Deserializer<'de> {
        de::Deserializer::from_slice_with_options(input, self)
    }

    /// Reads a YAML stream and creates a streaming Serde deserializer.
    pub fn deserializer_from_reader<R>(self, reader: R) -> de::Deserializer<'static>
    where
        R: Read,
    {
        de::Deserializer::from_reader_with_options(reader, self)
    }
}
