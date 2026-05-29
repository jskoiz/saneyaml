//! Load options and schema selection for constructed YAML document trees.

use crate::{Error, Node, Result, Span, de, parse};
use serde::de::DeserializeOwned;
use std::io::Read;

/// Default maximum YAML input size accepted by loading entrypoints.
pub const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Default alias expansion budget multiplier per input byte.
pub const DEFAULT_ALIAS_EXPANSION_FACTOR: usize = 64;

/// Minimum alias expansion budget used by default loading options.
pub const DEFAULT_MIN_ALIAS_EXPANSION_NODES: usize = 1024;

/// Scalar construction schema used by tree and Serde loading.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Schema {
    /// YAML 1.2-oriented core schema used by the default entrypoints.
    #[default]
    Yaml12,
    /// Explicit YAML 1.1 compatibility schema for legacy configuration files.
    Yaml11,
    /// Selects scalar construction from each document's `%YAML` version directive.
    ///
    /// Documents with `%YAML 1.1` use [`Schema::Yaml11`]. Documents without a
    /// version directive, with `%YAML 1.2`, or with newer numeric versions use
    /// [`Schema::Yaml12`].
    YamlVersionDirective,
}

/// Options for loading YAML into constructed trees or Serde values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadOptions {
    pub(crate) schema: Schema,
    max_input_bytes: Option<usize>,
    max_alias_expansion_nodes: Option<usize>,
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
        }
    }

    /// Creates load options using explicit YAML 1.1 compatibility construction.
    pub const fn yaml_1_1() -> Self {
        Self {
            schema: Schema::Yaml11,
            max_input_bytes: Some(DEFAULT_MAX_INPUT_BYTES),
            max_alias_expansion_nodes: None,
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

    pub(crate) fn alias_expansion_budget(self, input_len: usize) -> usize {
        self.max_alias_expansion_nodes.unwrap_or_else(|| {
            input_len
                .saturating_mul(DEFAULT_ALIAS_EXPANSION_FACTOR)
                .max(DEFAULT_MIN_ALIAS_EXPANSION_NODES)
        })
    }

    pub(crate) fn check_input_len(self, len: usize) -> Result<()> {
        if let Some(max) = self.max_input_bytes {
            if len > max {
                return Err(self.input_limit_error());
            }
        }
        Ok(())
    }

    pub(crate) fn input_limit_error(self) -> Error {
        let max = self
            .max_input_bytes
            .expect("input_limit_error requires a configured limit");
        Error::new(
            format!("YAML input exceeds configured limit of {max} bytes"),
            Span::default(),
        )
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
