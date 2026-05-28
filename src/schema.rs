//! Load options and schema selection for constructed YAML document trees.

use crate::{Node, Result, de, parse};
use serde::de::DeserializeOwned;
use std::io::Read;

/// Scalar construction schema used by tree and Serde loading.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Schema {
    /// YAML 1.2-oriented core schema used by the default entrypoints.
    #[default]
    Yaml12,
    /// Explicit YAML 1.1 compatibility schema for legacy configuration files.
    Yaml11,
}

/// Options for loading YAML into constructed trees or Serde values.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LoadOptions {
    pub(crate) schema: Schema,
}

impl LoadOptions {
    /// Creates default load options.
    pub const fn new() -> Self {
        Self {
            schema: Schema::Yaml12,
        }
    }

    /// Creates load options using explicit YAML 1.1 compatibility construction.
    pub const fn yaml_1_1() -> Self {
        Self {
            schema: Schema::Yaml11,
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
