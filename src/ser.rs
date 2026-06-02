//! Serde serialization entrypoints for YAML values and writers.
//!
//! ```rust
//! let mut mapping = yaml::Mapping::new();
//! mapping.insert(
//!     yaml::Value::String("name".to_owned()),
//!     yaml::Value::String("api".to_owned()),
//! );
//! let value = yaml::Value::Mapping(mapping);
//!
//! let output = yaml::to_string_with_options(
//!     &value,
//!     yaml::EmitOptions::structural().with_key_order(yaml::KeyOrder::Sort),
//! )?;
//! assert!(output.contains("name: api"));
//!
//! let mut bytes = Vec::new();
//! yaml::to_writer(&mut bytes, &value)?;
//! assert!(!bytes.is_empty());
//! # Ok::<(), yaml::Error>(())
//! ```

use crate::{
    EmitOptions, Error, Mapping, Node, NodeValue, Number, Result, Span, Tag, TaggedNode,
    TaggedValue, Timestamp, Value,
};
use serde::Serialize;
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use std::io::Write;

const NUMBER_STRUCT: &str = "$yaml::Number";
const BYTES_UNSUPPORTED: &str =
    "serialization and deserialization of bytes in YAML is not implemented";
const MAX_SERIALIZE_HINT_PREALLOC: usize = 4096;

/// Converts a serializable value into a YAML [`Value`].
pub fn to_value<T>(value: T) -> Result<Value>
where
    T: Serialize,
{
    value.serialize(ValueSerializer)
}

/// Serializes a value into a YAML string.
pub fn to_string<T>(value: &T) -> Result<String>
where
    T: ?Sized + Serialize,
{
    let node = serialized_node(value)?;
    crate::emit::to_string(&node)
}

/// Serializes a value into a YAML string using explicit emission options.
///
/// [`EmitOptions::structural`] is the default. [`EmitOptions::byte_compatible`]
/// is opt-in for the supported `serde_yaml` writer-byte corpus.
pub fn to_string_with_options<T>(value: &T, options: EmitOptions) -> Result<String>
where
    T: ?Sized + Serialize,
{
    let node = serialized_node_with_options(value, options)?;
    crate::emit::to_string_with_options(&node, options)
}

/// Serializes a value as YAML and writes it to an output sink.
pub fn to_writer<W, T>(mut writer: W, value: &T) -> Result<()>
where
    W: Write,
    T: ?Sized + Serialize,
{
    to_writer_with_options(&mut writer, value, EmitOptions::structural())
}

/// Serializes a value as YAML using explicit emission options and writes it to
/// an output sink.
///
/// [`EmitOptions::structural`] is the default. [`EmitOptions::byte_compatible`]
/// is opt-in for the supported `serde_yaml` writer-byte corpus.
pub fn to_writer_with_options<W, T>(mut writer: W, value: &T, options: EmitOptions) -> Result<()>
where
    W: Write,
    T: ?Sized + Serialize,
{
    let output = to_string_with_options(value, options)?;
    writer.write_all(output.as_bytes()).map_err(|error| {
        Error::new(
            format!("failed to write YAML output: {error}"),
            Span::default(),
        )
    })
}

fn serialized_node<T>(value: &T) -> Result<Node>
where
    T: ?Sized + Serialize,
{
    let value = value.serialize(DocumentValueSerializer)?;
    Ok(node_from_value(value))
}

fn serialized_node_with_options<T>(value: &T, options: EmitOptions) -> Result<Node>
where
    T: ?Sized + Serialize,
{
    if options.is_byte_compatible() {
        value.serialize(ByteCompatibleRootSerializer)
    } else {
        serialized_node(value)
    }
}

fn node_from_value(value: Value) -> Node {
    Node::new(value.into(), Default::default())
}

fn byte_compatible_single_quoted_node(value: impl Into<String>) -> Node {
    Node::new(NodeValue::String(value.into()), Default::default())
        .with_scalar_source(crate::emit::BYTE_COMPATIBLE_SINGLE_QUOTED_SOURCE)
}

/// Streaming YAML serializer for writing one document at a time.
pub struct Serializer<W> {
    writer: W,
    document_written: bool,
    emit_options: EmitOptions,
}

impl<W> Serializer<W>
where
    W: Write,
{
    /// Creates a serializer that writes YAML documents to `writer`.
    pub fn new(writer: W) -> Self {
        Self::with_options(writer, EmitOptions::structural())
    }

    /// Creates a serializer that writes YAML documents to `writer` using
    /// explicit emission options.
    ///
    /// [`EmitOptions::structural`] is the default. [`EmitOptions::byte_compatible`]
    /// is opt-in for the supported `serde_yaml` writer-byte corpus.
    pub fn with_options(writer: W, emit_options: EmitOptions) -> Self {
        Self {
            writer,
            document_written: false,
            emit_options,
        }
    }

    /// Flushes the wrapped writer.
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush().map_err(write_error)
    }

    /// Flushes and returns the wrapped writer.
    pub fn into_inner(mut self) -> Result<W> {
        self.flush()?;
        Ok(self.writer)
    }

    fn write_value(&mut self, value: Value) -> Result<()> {
        self.write_node(node_from_value(value))
    }

    fn write_node(&mut self, node: Node) -> Result<()> {
        let mut output = String::new();
        if self.document_written {
            output.push_str("---\n");
        }
        output.push_str(&crate::emit::to_string_with_options(
            &node,
            self.emit_options,
        )?);
        self.writer
            .write_all(output.as_bytes())
            .map_err(write_error)?;
        self.document_written = true;
        Ok(())
    }
}

impl<'a, W> ser::Serializer for &'a mut Serializer<W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;
    type SerializeSeq = DocumentSequenceSerializer<'a, W>;
    type SerializeTuple = DocumentSequenceSerializer<'a, W>;
    type SerializeTupleStruct = DocumentSequenceSerializer<'a, W>;
    type SerializeTupleVariant = DocumentTupleVariantSerializer<'a, W>;
    type SerializeMap = DocumentMappingSerializer<'a, W>;
    type SerializeStruct = DocumentStructSerializer<'a, W>;
    type SerializeStructVariant = DocumentStructVariantSerializer<'a, W>;

    fn serialize_bool(self, value: bool) -> Result<()> {
        self.write_value(Value::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_i128(self, value: i128) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_u8(self, value: u8) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_u128(self, value: u128) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_f32(self, value: f32) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<()> {
        self.write_value(Value::from(value))
    }

    fn serialize_char(self, value: char) -> Result<()> {
        if self.emit_options.is_byte_compatible() {
            return self.write_node(byte_compatible_single_quoted_node(value.to_string()));
        }
        self.write_value(Value::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<()> {
        self.write_value(Value::String(value.to_string()))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
        Err(bytes_unsupported_error())
    }

    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        self.write_value(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.write_value(Value::String(variant.to_string()))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        validate_variant_tag(variant)?;
        self.write_value(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: value.serialize(DocumentValueSerializer)?,
        })))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<DocumentSequenceSerializer<'a, W>> {
        Ok(DocumentSequenceSerializer {
            serializer: self,
            inner: sequence_serializer_with_capacity(len, true),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<DocumentSequenceSerializer<'a, W>> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<DocumentSequenceSerializer<'a, W>> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<DocumentTupleVariantSerializer<'a, W>> {
        validate_variant_tag(variant)?;
        Ok(DocumentTupleVariantSerializer {
            serializer: self,
            inner: tuple_variant_serializer_with_capacity(variant, len, true),
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<DocumentMappingSerializer<'a, W>> {
        Ok(DocumentMappingSerializer {
            serializer: self,
            inner: MappingSerializer::new(len, len == Some(1), true),
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<DocumentStructSerializer<'a, W>> {
        Ok(DocumentStructSerializer {
            serializer: self,
            inner: StructSerializer {
                entries: mapping_with_hinted_capacity(Some(len)),
                reject_bytes: true,
            },
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<DocumentStructVariantSerializer<'a, W>> {
        validate_variant_tag(variant)?;
        Ok(DocumentStructVariantSerializer {
            serializer: self,
            inner: StructVariantSerializer {
                variant,
                entries: mapping_with_hinted_capacity(Some(len)),
                reject_bytes: true,
            },
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + std::fmt::Display,
    {
        self.write_value(Value::String(value.to_string()))
    }
}

#[doc(hidden)]
pub struct DocumentSequenceSerializer<'a, W> {
    serializer: &'a mut Serializer<W>,
    inner: SequenceSerializer,
}

impl<W> SerializeSeq for DocumentSequenceSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(&mut self.inner, value)
    }

    fn end(self) -> Result<()> {
        let value = SerializeSeq::end(self.inner)?;
        self.serializer.write_value(value)
    }
}

impl<W> SerializeTuple for DocumentSequenceSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        SerializeSeq::end(self)
    }
}

impl<W> SerializeTupleStruct for DocumentSequenceSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        SerializeSeq::end(self)
    }
}

#[doc(hidden)]
pub struct DocumentTupleVariantSerializer<'a, W> {
    serializer: &'a mut Serializer<W>,
    inner: TupleVariantSerializer,
}

impl<W> SerializeTupleVariant for DocumentTupleVariantSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

#[doc(hidden)]
pub struct DocumentMappingSerializer<'a, W> {
    serializer: &'a mut Serializer<W>,
    inner: MappingSerializer,
}

impl<W> SerializeMap for DocumentMappingSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_key(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_value(value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        self.inner.serialize_entry(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

#[doc(hidden)]
pub struct DocumentStructSerializer<'a, W> {
    serializer: &'a mut Serializer<W>,
    inner: StructSerializer,
}

impl<W> SerializeStruct for DocumentStructSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

#[doc(hidden)]
pub struct DocumentStructVariantSerializer<'a, W> {
    serializer: &'a mut Serializer<W>,
    inner: StructVariantSerializer,
}

impl<W> SerializeStructVariant for DocumentStructVariantSerializer<'_, W>
where
    W: Write,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.serialize_field(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

/// Serializer that builds a spanless YAML [`Value`].
pub struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = SequenceSerializer;
    type SerializeTuple = SequenceSerializer;
    type SerializeTupleStruct = SequenceSerializer;
    type SerializeTupleVariant = TupleVariantSerializer;
    type SerializeMap = MappingSerializer;
    type SerializeStruct = StructSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Value> {
        Ok(Value::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i128(self, value: i128) -> Result<Value> {
        if let Ok(value) = u64::try_from(value) {
            self.serialize_u64(value)
        } else if let Ok(value) = i64::try_from(value) {
            self.serialize_i64(value)
        } else {
            Ok(Value::String(value.to_string()))
        }
    }

    fn serialize_u8(self, value: u8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u128(self, value: u128) -> Result<Value> {
        if let Ok(value) = u64::try_from(value) {
            self.serialize_u64(value)
        } else {
            Ok(Value::String(value.to_string()))
        }
    }

    fn serialize_f32(self, value: f32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_char(self, value: char) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Value> {
        Ok(Value::Sequence(
            value.iter().copied().map(Value::from).collect(),
        ))
    }

    fn serialize_none(self) -> Result<Value> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Value> {
        Ok(Value::String(variant.to_string()))
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        if name == NUMBER_STRUCT {
            return value.serialize(PreserveNumberSerializer);
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        validate_variant_tag(variant)?;
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: value.serialize(self)?,
        })))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SequenceSerializer> {
        Ok(sequence_serializer_with_capacity(len, false))
    }

    fn serialize_tuple(self, len: usize) -> Result<SequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<TupleVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(tuple_variant_serializer_with_capacity(variant, len, false))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MappingSerializer> {
        Ok(MappingSerializer::new(len, len == Some(1), false))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<StructSerializer> {
        Ok(StructSerializer {
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: false,
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<StructVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(StructVariantSerializer {
            variant,
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: false,
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + std::fmt::Display,
    {
        Ok(Value::String(value.to_string()))
    }
}

struct DocumentValueSerializer;

impl ser::Serializer for DocumentValueSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = SequenceSerializer;
    type SerializeTuple = SequenceSerializer;
    type SerializeTupleStruct = SequenceSerializer;
    type SerializeTupleVariant = TupleVariantSerializer;
    type SerializeMap = MappingSerializer;
    type SerializeStruct = StructSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Value> {
        ValueSerializer.serialize_bool(value)
    }

    fn serialize_i8(self, value: i8) -> Result<Value> {
        ValueSerializer.serialize_i8(value)
    }

    fn serialize_i16(self, value: i16) -> Result<Value> {
        ValueSerializer.serialize_i16(value)
    }

    fn serialize_i32(self, value: i32) -> Result<Value> {
        ValueSerializer.serialize_i32(value)
    }

    fn serialize_i64(self, value: i64) -> Result<Value> {
        ValueSerializer.serialize_i64(value)
    }

    fn serialize_i128(self, value: i128) -> Result<Value> {
        ValueSerializer.serialize_i128(value)
    }

    fn serialize_u8(self, value: u8) -> Result<Value> {
        ValueSerializer.serialize_u8(value)
    }

    fn serialize_u16(self, value: u16) -> Result<Value> {
        ValueSerializer.serialize_u16(value)
    }

    fn serialize_u32(self, value: u32) -> Result<Value> {
        ValueSerializer.serialize_u32(value)
    }

    fn serialize_u64(self, value: u64) -> Result<Value> {
        ValueSerializer.serialize_u64(value)
    }

    fn serialize_u128(self, value: u128) -> Result<Value> {
        ValueSerializer.serialize_u128(value)
    }

    fn serialize_f32(self, value: f32) -> Result<Value> {
        ValueSerializer.serialize_f32(value)
    }

    fn serialize_f64(self, value: f64) -> Result<Value> {
        ValueSerializer.serialize_f64(value)
    }

    fn serialize_char(self, value: char) -> Result<Value> {
        ValueSerializer.serialize_char(value)
    }

    fn serialize_str(self, value: &str) -> Result<Value> {
        ValueSerializer.serialize_str(value)
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Value> {
        Err(bytes_unsupported_error())
    }

    fn serialize_none(self) -> Result<Value> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value> {
        ValueSerializer.serialize_unit()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Value> {
        ValueSerializer.serialize_unit_variant("", 0, variant)
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        if name == NUMBER_STRUCT {
            return value.serialize(PreserveNumberSerializer);
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        validate_variant_tag(variant)?;
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: value.serialize(self)?,
        })))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SequenceSerializer> {
        Ok(sequence_serializer_with_capacity(len, true))
    }

    fn serialize_tuple(self, len: usize) -> Result<SequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<TupleVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(tuple_variant_serializer_with_capacity(variant, len, true))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MappingSerializer> {
        Ok(MappingSerializer::new(len, len == Some(1), true))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<StructSerializer> {
        Ok(StructSerializer {
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: true,
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<StructVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(StructVariantSerializer {
            variant,
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: true,
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + std::fmt::Display,
    {
        ValueSerializer.collect_str(value)
    }
}

struct ByteCompatibleRootSerializer;

impl ser::Serializer for ByteCompatibleRootSerializer {
    type Ok = Node;
    type Error = Error;
    type SerializeSeq = RootSequenceSerializer;
    type SerializeTuple = RootSequenceSerializer;
    type SerializeTupleStruct = RootSequenceSerializer;
    type SerializeTupleVariant = RootTupleVariantSerializer;
    type SerializeMap = RootMappingSerializer;
    type SerializeStruct = RootStructSerializer;
    type SerializeStructVariant = RootStructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Node> {
        Ok(node_from_value(Value::Bool(value)))
    }

    fn serialize_i8(self, value: i8) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_i16(self, value: i16) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_i32(self, value: i32) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_i64(self, value: i64) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_i128(self, value: i128) -> Result<Node> {
        Ok(node_from_value(ValueSerializer.serialize_i128(value)?))
    }

    fn serialize_u8(self, value: u8) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_u16(self, value: u16) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_u32(self, value: u32) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_u64(self, value: u64) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_u128(self, value: u128) -> Result<Node> {
        Ok(node_from_value(ValueSerializer.serialize_u128(value)?))
    }

    fn serialize_f32(self, value: f32) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_f64(self, value: f64) -> Result<Node> {
        Ok(node_from_value(Value::from(value)))
    }

    fn serialize_char(self, value: char) -> Result<Node> {
        Ok(byte_compatible_single_quoted_node(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Node> {
        Ok(node_from_value(Value::String(value.to_string())))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Node> {
        Err(bytes_unsupported_error())
    }

    fn serialize_none(self) -> Result<Node> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Node> {
        Ok(node_from_value(Value::Null))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Node> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Node> {
        Ok(node_from_value(Value::String(variant.to_string())))
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        if name == NUMBER_STRUCT {
            return Ok(node_from_value(value.serialize(PreserveNumberSerializer)?));
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        validate_variant_tag(variant)?;
        Ok(node_from_value(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: value.serialize(DocumentValueSerializer)?,
        }))))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<RootSequenceSerializer> {
        Ok(RootSequenceSerializer(sequence_serializer_with_capacity(
            len, true,
        )))
    }

    fn serialize_tuple(self, len: usize) -> Result<RootSequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<RootSequenceSerializer> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<RootTupleVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(RootTupleVariantSerializer(
            tuple_variant_serializer_with_capacity(variant, len, true),
        ))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<RootMappingSerializer> {
        Ok(RootMappingSerializer(MappingSerializer::new(
            len,
            len == Some(1),
            true,
        )))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<RootStructSerializer> {
        Ok(RootStructSerializer(StructSerializer {
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: true,
        }))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<RootStructVariantSerializer> {
        validate_variant_tag(variant)?;
        Ok(RootStructVariantSerializer(StructVariantSerializer {
            variant,
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: true,
        }))
    }

    fn collect_str<T>(self, value: &T) -> Result<Node>
    where
        T: ?Sized + std::fmt::Display,
    {
        Ok(node_from_value(Value::String(value.to_string())))
    }
}

struct RootSequenceSerializer(SequenceSerializer);

impl SerializeSeq for RootSequenceSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(&mut self.0, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeSeq::end(self.0)?))
    }
}

impl SerializeTuple for RootSequenceSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTuple::serialize_element(&mut self.0, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeTuple::end(self.0)?))
    }
}

impl SerializeTupleStruct for RootSequenceSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTupleStruct::serialize_field(&mut self.0, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeTupleStruct::end(self.0)?))
    }
}

struct RootTupleVariantSerializer(TupleVariantSerializer);

impl SerializeTupleVariant for RootTupleVariantSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTupleVariant::serialize_field(&mut self.0, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeTupleVariant::end(self.0)?))
    }
}

struct RootMappingSerializer(MappingSerializer);

impl SerializeMap for RootMappingSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeMap::serialize_key(&mut self.0, key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeMap::serialize_value(&mut self.0, value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        SerializeMap::serialize_entry(&mut self.0, key, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeMap::end(self.0)?))
    }
}

struct RootStructSerializer(StructSerializer);

impl SerializeStruct for RootStructSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeStruct::serialize_field(&mut self.0, key, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeStruct::end(self.0)?))
    }
}

struct RootStructVariantSerializer(StructVariantSerializer);

impl SerializeStructVariant for RootStructVariantSerializer {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeStructVariant::serialize_field(&mut self.0, key, value)
    }

    fn end(self) -> Result<Node> {
        Ok(node_from_value(SerializeStructVariant::end(self.0)?))
    }
}

fn serialize_nested_value<T>(value: &T, reject_bytes: bool) -> Result<Value>
where
    T: ?Sized + Serialize,
{
    if reject_bytes {
        value.serialize(DocumentValueSerializer)
    } else {
        value.serialize(ValueSerializer)
    }
}

fn hinted_capacity(len: Option<usize>) -> usize {
    len.unwrap_or(0).min(MAX_SERIALIZE_HINT_PREALLOC)
}

fn sequence_serializer_with_capacity(len: Option<usize>, reject_bytes: bool) -> SequenceSerializer {
    SequenceSerializer {
        items: Vec::with_capacity(hinted_capacity(len)),
        reject_bytes,
    }
}

fn tuple_variant_serializer_with_capacity(
    variant: &'static str,
    len: usize,
    reject_bytes: bool,
) -> TupleVariantSerializer {
    TupleVariantSerializer {
        variant,
        items: Vec::with_capacity(hinted_capacity(Some(len))),
        reject_bytes,
    }
}

fn mapping_with_hinted_capacity(len: Option<usize>) -> Mapping {
    Mapping::with_capacity(hinted_capacity(len))
}

#[doc(hidden)]
pub struct SequenceSerializer {
    items: Vec<Value>,
    reject_bytes: bool,
}

impl SerializeSeq for SequenceSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.items
            .push(serialize_nested_value(value, self.reject_bytes)?);
        Ok(())
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Sequence(self.items))
    }
}

impl SerializeTuple for SequenceSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleStruct for SequenceSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value> {
        SerializeSeq::end(self)
    }
}

#[doc(hidden)]
pub struct TupleVariantSerializer {
    variant: &'static str,
    items: Vec<Value>,
    reject_bytes: bool,
}

impl SerializeTupleVariant for TupleVariantSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.items
            .push(serialize_nested_value(value, self.reject_bytes)?);
        Ok(())
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(self.variant),
            value: Value::Sequence(self.items),
        })))
    }
}

#[doc(hidden)]
pub struct MappingSerializer {
    entries: Mapping,
    next_key: Option<SerializedKey>,
    tagged: Option<TaggedValue>,
    detect_tag: bool,
    reject_bytes: bool,
}

enum SerializedKey {
    Value(Value),
    Tag(Tag),
}

impl SerializedKey {
    fn into_value(self) -> Value {
        match self {
            SerializedKey::Value(value) => value,
            SerializedKey::Tag(tag) => Value::String(tag.to_string()),
        }
    }
}

impl SerializeMap for MappingSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if self.next_key.is_some() {
            return Err(serialize_error(
                "serialize_key called before serialize_value",
            ));
        }
        self.next_key = Some(serialize_mapping_key(
            key,
            self.should_detect_tag(),
            self.reject_bytes,
        )?);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| serialize_error("serialize_value called before serialize_key"))?;
        let value = serialize_nested_value(value, self.reject_bytes)?;
        self.insert_entry(key, value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        let key = serialize_mapping_key(key, self.should_detect_tag(), self.reject_bytes)?;
        let value = serialize_nested_value(value, self.reject_bytes)?;
        self.insert_entry(key, value)
    }

    fn end(mut self) -> Result<Value> {
        if self.next_key.is_some() {
            return Err(serialize_error(
                "serialized mapping has a key without a value",
            ));
        }
        if self.entries.is_empty()
            && let Some(tagged) = self.tagged.take()
        {
            return Ok(Value::Tagged(Box::new(tagged)));
        }
        self.flush_tagged()?;
        Ok(Value::Mapping(self.entries))
    }
}

impl MappingSerializer {
    fn new(len: Option<usize>, detect_tag: bool, reject_bytes: bool) -> Self {
        Self {
            entries: mapping_with_hinted_capacity(len),
            next_key: None,
            tagged: None,
            detect_tag,
            reject_bytes,
        }
    }

    fn should_detect_tag(&self) -> bool {
        self.detect_tag && self.entries.is_empty() && self.tagged.is_none()
    }

    fn insert_entry(&mut self, key: SerializedKey, value: Value) -> Result<()> {
        match key {
            SerializedKey::Tag(tag) if self.should_detect_tag() => {
                self.tagged = Some(TaggedValue { tag, value });
                Ok(())
            }
            key => {
                self.flush_tagged()?;
                insert_unique(&mut self.entries, key.into_value(), value)
            }
        }
    }

    fn flush_tagged(&mut self) -> Result<()> {
        if let Some(tagged) = self.tagged.take() {
            insert_unique(
                &mut self.entries,
                Value::String(tagged.tag.to_string()),
                tagged.value,
            )?;
        }
        Ok(())
    }
}

fn serialize_mapping_key<T>(key: &T, detect_tag: bool, reject_bytes: bool) -> Result<SerializedKey>
where
    T: ?Sized + Serialize,
{
    if detect_tag {
        key.serialize(TagDetectingKeySerializer { reject_bytes })
    } else {
        Ok(SerializedKey::Value(serialize_nested_value(
            key,
            reject_bytes,
        )?))
    }
}

#[derive(Clone, Copy)]
struct TagDetectingKeySerializer {
    reject_bytes: bool,
}

impl ser::Serializer for TagDetectingKeySerializer {
    type Ok = SerializedKey;
    type Error = Error;
    type SerializeSeq = KeySequenceSerializer;
    type SerializeTuple = KeySequenceSerializer;
    type SerializeTupleStruct = KeySequenceSerializer;
    type SerializeTupleVariant = KeyTupleVariantSerializer;
    type SerializeMap = KeyMappingSerializer;
    type SerializeStruct = KeyStructSerializer;
    type SerializeStructVariant = KeyStructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::Bool(value)))
    }

    fn serialize_i8(self, value: i8) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_i16(self, value: i16) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_i32(self, value: i32) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_i64(self, value: i64) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_i128(self, value: i128) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(ValueSerializer.serialize_i128(value)?))
    }

    fn serialize_u8(self, value: u8) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_u16(self, value: u16) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_u32(self, value: u32) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_u64(self, value: u64) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_u128(self, value: u128) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(ValueSerializer.serialize_u128(value)?))
    }

    fn serialize_f32(self, value: f32) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_f64(self, value: f64) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::from(value)))
    }

    fn serialize_char(self, value: char) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::String(value.to_string())))
    }

    fn serialize_str(self, value: &str) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::String(value.to_string())))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<SerializedKey> {
        if self.reject_bytes {
            return Err(bytes_unsupported_error());
        }
        Ok(SerializedKey::Value(Value::Sequence(
            value.iter().copied().map(Value::from).collect(),
        )))
    }

    fn serialize_none(self) -> Result<SerializedKey> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<SerializedKey>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::Null))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<SerializedKey> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(Value::String(variant.to_string())))
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<SerializedKey>
    where
        T: ?Sized + Serialize,
    {
        if name == NUMBER_STRUCT {
            Ok(SerializedKey::Value(
                value.serialize(PreserveNumberSerializer)?,
            ))
        } else {
            value.serialize(self)
        }
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<SerializedKey>
    where
        T: ?Sized + Serialize,
    {
        validate_variant_tag(variant)?;
        Ok(SerializedKey::Value(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: serialize_nested_value(value, self.reject_bytes)?,
        }))))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(KeySequenceSerializer(sequence_serializer_with_capacity(
            len,
            self.reject_bytes,
        )))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        Ok(KeySequenceSerializer(sequence_serializer_with_capacity(
            Some(len),
            self.reject_bytes,
        )))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(KeySequenceSerializer(sequence_serializer_with_capacity(
            Some(len),
            self.reject_bytes,
        )))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        validate_variant_tag(variant)?;
        Ok(KeyTupleVariantSerializer(
            tuple_variant_serializer_with_capacity(variant, len, self.reject_bytes),
        ))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(KeyMappingSerializer(MappingSerializer::new(
            len,
            len == Some(1),
            self.reject_bytes,
        )))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        Ok(KeyStructSerializer(StructSerializer {
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: self.reject_bytes,
        }))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        validate_variant_tag(variant)?;
        Ok(KeyStructVariantSerializer(StructVariantSerializer {
            variant,
            entries: mapping_with_hinted_capacity(Some(len)),
            reject_bytes: self.reject_bytes,
        }))
    }

    fn collect_str<T>(self, value: &T) -> Result<SerializedKey>
    where
        T: ?Sized + std::fmt::Display,
    {
        let (value, is_tag) = display_text_and_tag(value);
        if is_tag {
            Ok(SerializedKey::Tag(Tag::new(value)))
        } else {
            Ok(SerializedKey::Value(Value::String(value)))
        }
    }
}

#[derive(Clone, Copy)]
enum DisplayTagState {
    Empty,
    Bang,
    Tag,
    NotTag,
}

impl DisplayTagState {
    fn observe(&mut self, text: &str) {
        if text.is_empty() {
            if matches!(self, DisplayTagState::Bang) {
                *self = DisplayTagState::Tag;
            }
            return;
        }
        *self = match self {
            DisplayTagState::Empty if text == "!" => DisplayTagState::Bang,
            DisplayTagState::Bang => DisplayTagState::Tag,
            DisplayTagState::Tag | DisplayTagState::NotTag | DisplayTagState::Empty => {
                DisplayTagState::NotTag
            }
        };
    }

    fn is_tag(self) -> bool {
        matches!(self, DisplayTagState::Tag)
    }
}

struct DisplayTagCapture {
    text: String,
    state: DisplayTagState,
}

impl std::fmt::Write for DisplayTagCapture {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.state.observe(text);
        self.text.push_str(text);
        Ok(())
    }
}

fn display_text_and_tag<T>(value: &T) -> (String, bool)
where
    T: ?Sized + std::fmt::Display,
{
    let mut capture = DisplayTagCapture {
        text: String::new(),
        state: DisplayTagState::Empty,
    };
    std::fmt::write(&mut capture, format_args!("{value}"))
        .expect("formatting into capture succeeds");
    (capture.text, capture.state.is_tag())
}

struct KeySequenceSerializer(SequenceSerializer);

impl SerializeSeq for KeySequenceSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(&mut self.0, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeSeq::end(self.0)?))
    }
}

impl SerializeTuple for KeySequenceSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTuple::serialize_element(&mut self.0, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeTuple::end(self.0)?))
    }
}

impl SerializeTupleStruct for KeySequenceSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTupleStruct::serialize_field(&mut self.0, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeTupleStruct::end(self.0)?))
    }
}

struct KeyTupleVariantSerializer(TupleVariantSerializer);

impl SerializeTupleVariant for KeyTupleVariantSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeTupleVariant::serialize_field(&mut self.0, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeTupleVariant::end(self.0)?))
    }
}

struct KeyMappingSerializer(MappingSerializer);

impl SerializeMap for KeyMappingSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeMap::serialize_key(&mut self.0, key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeMap::serialize_value(&mut self.0, value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        SerializeMap::serialize_entry(&mut self.0, key, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeMap::end(self.0)?))
    }
}

struct KeyStructSerializer(StructSerializer);

impl SerializeStruct for KeyStructSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeStruct::serialize_field(&mut self.0, key, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeStruct::end(self.0)?))
    }
}

struct KeyStructVariantSerializer(StructVariantSerializer);

impl SerializeStructVariant for KeyStructVariantSerializer {
    type Ok = SerializedKey;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeStructVariant::serialize_field(&mut self.0, key, value)
    }

    fn end(self) -> Result<SerializedKey> {
        Ok(SerializedKey::Value(SerializeStructVariant::end(self.0)?))
    }
}

#[doc(hidden)]
pub struct StructSerializer {
    entries: Mapping,
    reject_bytes: bool,
}

impl SerializeStruct for StructSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        insert_unique(
            &mut self.entries,
            Value::String(key.to_string()),
            serialize_nested_value(value, self.reject_bytes)?,
        )
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Mapping(self.entries))
    }
}

#[doc(hidden)]
pub struct StructVariantSerializer {
    variant: &'static str,
    entries: Mapping,
    reject_bytes: bool,
}

impl SerializeStructVariant for StructVariantSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        insert_unique(
            &mut self.entries,
            Value::String(key.to_string()),
            serialize_nested_value(value, self.reject_bytes)?,
        )
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(self.variant),
            value: Value::Mapping(self.entries),
        })))
    }
}

fn insert_unique(mapping: &mut Mapping, key: Value, value: Value) -> Result<()> {
    if mapping.insert(key, value).is_some() {
        return Err(serialize_error(
            "serialized mapping contains a duplicate key",
        ));
    }
    Ok(())
}

fn write_error(error: std::io::Error) -> Error {
    Error::new(
        format!("failed to write YAML output: {error}"),
        Span::default(),
    )
}

fn serialize_error(message: &'static str) -> Error {
    Error::new(message, Span::default())
}

fn bytes_unsupported_error() -> Error {
    serialize_error(BYTES_UNSUPPORTED)
}

fn validate_variant_tag(tag: &str) -> Result<()> {
    if tag.is_empty() {
        return Err(serialize_error("empty YAML tag is not allowed"));
    }
    Ok(())
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Number(number) => number.serialize(serializer),
            Value::String(value) => serializer.serialize_str(value),
            Value::Sequence(items) => items.serialize(serializer),
            Value::Mapping(mapping) => mapping.serialize(serializer),
            Value::Tagged(tagged) => tagged.serialize(serializer),
        }
    }
}

impl Serialize for Mapping {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        struct NumberRepr<'a>(&'a Number);

        impl Serialize for NumberRepr<'_> {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: ser::Serializer,
            {
                match self.0 {
                    Number::Integer(value) => serializer.serialize_i128(*value),
                    Number::Unsigned(value) => serializer.serialize_u128(*value),
                    Number::Float(value) => serializer.serialize_f64(*value),
                }
            }
        }

        serializer.serialize_newtype_struct(NUMBER_STRUCT, &NumberRepr(self))
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct PreserveNumberSerializer;

impl ser::Serializer for PreserveNumberSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = ser::Impossible<Value, Error>;
    type SerializeTuple = ser::Impossible<Value, Error>;
    type SerializeTupleStruct = ser::Impossible<Value, Error>;
    type SerializeTupleVariant = ser::Impossible<Value, Error>;
    type SerializeMap = ser::Impossible<Value, Error>;
    type SerializeStruct = ser::Impossible<Value, Error>;
    type SerializeStructVariant = ser::Impossible<Value, Error>;

    fn serialize_bool(self, value: bool) -> Result<Value> {
        Ok(Value::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i128(self, value: i128) -> Result<Value> {
        Ok(Value::Number(Number::Integer(value)))
    }

    fn serialize_u8(self, value: u8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u128(self, value: u128) -> Result<Value> {
        Ok(Value::Number(Number::Unsigned(value)))
    }

    fn serialize_f32(self, value: f32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_char(self, value: char) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Value> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_none(self) -> Result<Value> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_some<T>(self, _value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_unit(self) -> Result<Value> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<Value> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, _value: &T) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Value>
    where
        T: ?Sized + Serialize,
    {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(serialize_error("invalid numeric serialization payload"))
    }

    fn collect_str<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + std::fmt::Display,
    {
        Ok(Value::String(value.to_string()))
    }
}

impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Serialize for TaggedValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        struct SerializeTag<'a>(&'a Tag);

        impl std::fmt::Display for SerializeTag<'_> {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let tag = self.0.to_string();
                if let Some(rest) = tag.strip_prefix('!') {
                    formatter.write_str("!")?;
                    formatter.write_str(rest)
                } else {
                    formatter.write_str(&tag)
                }
            }
        }

        impl Serialize for SerializeTag<'_> {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: ser::Serializer,
            {
                serializer.collect_str(self)
            }
        }

        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&SerializeTag(&self.tag), &self.value)?;
        map.end()
    }
}

impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl Serialize for NodeValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            NodeValue::Null => serializer.serialize_unit(),
            NodeValue::Bool(value) => serializer.serialize_bool(*value),
            NodeValue::Number(number) => number.serialize(serializer),
            NodeValue::String(value) => serializer.serialize_str(value),
            NodeValue::Sequence(items) => items.serialize(serializer),
            NodeValue::Mapping(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
            NodeValue::Tagged(tagged) => tagged.serialize(serializer),
        }
    }
}

impl Serialize for TaggedNode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let tagged = TaggedValue {
            tag: self.tag.clone(),
            value: Value::from(&self.value),
        };
        tagged.serialize(serializer)
    }
}
