use crate::{
    Error, Mapping, Node, NodeValue, Number, Result, Span, Tag, TaggedNode, TaggedValue, Value,
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

pub fn to_value<T>(value: T) -> Result<Value>
where
    T: Serialize,
{
    value.serialize(ValueSerializer)
}

pub fn to_string<T>(value: &T) -> Result<String>
where
    T: ?Sized + Serialize,
{
    let node = serialized_node(value)?;
    crate::emit::to_string(&node)
}

pub fn to_writer<W, T>(mut writer: W, value: &T) -> Result<()>
where
    W: Write,
    T: ?Sized + Serialize,
{
    let output = to_string(value)?;
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
    reject_document_bytes(value)?;
    let value = value.serialize(ValueSerializer)?;
    Ok(Node::new(value.into(), Default::default()))
}

pub struct Serializer<W> {
    writer: W,
    document_written: bool,
}

impl<W> Serializer<W>
where
    W: Write,
{
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            document_written: false,
        }
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush().map_err(write_error)
    }

    pub fn into_inner(mut self) -> Result<W> {
        self.flush()?;
        Ok(self.writer)
    }

    fn write_value(&mut self, value: Value) -> Result<()> {
        let mut output = String::new();
        if self.document_written {
            output.push_str("---\n");
        }
        output.push_str(&crate::emit::to_string(&Node::new(
            value.into(),
            Default::default(),
        ))?);
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
        reject_document_bytes(value)?;
        self.write_value(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(variant),
            value: value.serialize(ValueSerializer)?,
        })))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<DocumentSequenceSerializer<'a, W>> {
        Ok(DocumentSequenceSerializer {
            serializer: self,
            inner: SequenceSerializer {
                items: Vec::with_capacity(len.unwrap_or(0)),
            },
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
            inner: TupleVariantSerializer {
                variant,
                items: Vec::with_capacity(len),
            },
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<DocumentMappingSerializer<'a, W>> {
        Ok(DocumentMappingSerializer {
            serializer: self,
            inner: MappingSerializer::new(len, len == Some(1)),
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
                entries: Mapping::with_capacity(len),
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
                entries: Mapping::with_capacity(len),
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
        reject_document_bytes(value)?;
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
        reject_document_bytes(value)?;
        self.inner.serialize_field(value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

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
        reject_document_bytes(key)?;
        self.inner.serialize_key(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)?;
        self.inner.serialize_value(value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        reject_document_bytes(key)?;
        reject_document_bytes(value)?;
        self.inner.serialize_entry(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

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
        reject_document_bytes(value)?;
        self.inner.serialize_field(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

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
        reject_document_bytes(value)?;
        self.inner.serialize_field(key, value)
    }

    fn end(self) -> Result<()> {
        let value = self.inner.end()?;
        self.serializer.write_value(value)
    }
}

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
        Ok(SequenceSerializer {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
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
        Ok(TupleVariantSerializer {
            variant,
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MappingSerializer> {
        Ok(MappingSerializer::new(len, len == Some(1)))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<StructSerializer> {
        Ok(StructSerializer {
            entries: Mapping::with_capacity(len),
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
            entries: Mapping::with_capacity(len),
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<Value>
    where
        T: ?Sized + std::fmt::Display,
    {
        Ok(Value::String(value.to_string()))
    }
}

pub struct SequenceSerializer {
    items: Vec<Value>,
}

impl SerializeSeq for SequenceSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.items.push(value.serialize(ValueSerializer)?);
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

pub struct TupleVariantSerializer {
    variant: &'static str,
    items: Vec<Value>,
}

impl SerializeTupleVariant for TupleVariantSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(self.variant),
            value: Value::Sequence(self.items),
        })))
    }
}

pub struct MappingSerializer {
    entries: Mapping,
    next_key: Option<SerializedKey>,
    tagged: Option<TaggedValue>,
    detect_tag: bool,
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
        self.next_key = Some(serialize_mapping_key(key, self.should_detect_tag())?);
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
        self.insert_entry(key, value.serialize(ValueSerializer)?)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        let key = serialize_mapping_key(key, self.should_detect_tag())?;
        let value = value.serialize(ValueSerializer)?;
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
    fn new(len: Option<usize>, detect_tag: bool) -> Self {
        Self {
            entries: Mapping::with_capacity(len.unwrap_or(0)),
            next_key: None,
            tagged: None,
            detect_tag,
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

fn serialize_mapping_key<T>(key: &T, detect_tag: bool) -> Result<SerializedKey>
where
    T: ?Sized + Serialize,
{
    if detect_tag {
        key.serialize(TagDetectingKeySerializer)
    } else {
        Ok(SerializedKey::Value(key.serialize(ValueSerializer)?))
    }
}

struct TagDetectingKeySerializer;

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
            value: value.serialize(ValueSerializer)?,
        }))))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(KeySequenceSerializer(SequenceSerializer {
            items: Vec::with_capacity(len.unwrap_or(0)),
        }))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        Ok(KeySequenceSerializer(SequenceSerializer {
            items: Vec::with_capacity(len),
        }))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(KeySequenceSerializer(SequenceSerializer {
            items: Vec::with_capacity(len),
        }))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        validate_variant_tag(variant)?;
        Ok(KeyTupleVariantSerializer(TupleVariantSerializer {
            variant,
            items: Vec::with_capacity(len),
        }))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(KeyMappingSerializer(MappingSerializer::new(
            len,
            len == Some(1),
        )))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        Ok(KeyStructSerializer(StructSerializer {
            entries: Mapping::with_capacity(len),
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
            entries: Mapping::with_capacity(len),
        }))
    }

    fn collect_str<T>(self, value: &T) -> Result<SerializedKey>
    where
        T: ?Sized + std::fmt::Display,
    {
        let is_tag = display_writes_like_serde_yaml_tag(value);
        let value = value.to_string();
        if is_tag {
            Ok(SerializedKey::Tag(Tag::new(value)))
        } else {
            Ok(SerializedKey::Value(Value::String(value)))
        }
    }
}

fn display_writes_like_serde_yaml_tag<T>(value: &T) -> bool
where
    T: ?Sized + std::fmt::Display,
{
    enum CheckForTag {
        Empty,
        Bang,
        Tag,
        NotTag,
    }

    impl std::fmt::Write for CheckForTag {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            if text.is_empty() {
                if matches!(self, CheckForTag::Bang) {
                    *self = CheckForTag::Tag;
                }
                return Ok(());
            }
            *self = match self {
                CheckForTag::Empty if text == "!" => CheckForTag::Bang,
                CheckForTag::Bang => CheckForTag::Tag,
                CheckForTag::Tag | CheckForTag::NotTag | CheckForTag::Empty => CheckForTag::NotTag,
            };
            Ok(())
        }
    }

    let mut state = CheckForTag::Empty;
    std::fmt::write(&mut state, format_args!("{value}")).expect("formatting into state succeeds");
    matches!(state, CheckForTag::Tag)
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

pub struct StructSerializer {
    entries: Mapping,
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
            value.serialize(ValueSerializer)?,
        )
    }

    fn end(self) -> Result<Value> {
        Ok(Value::Mapping(self.entries))
    }
}

pub struct StructVariantSerializer {
    variant: &'static str,
    entries: Mapping,
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
            value.serialize(ValueSerializer)?,
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

fn reject_document_bytes<T>(value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    value.serialize(RejectBytesSerializer)
}

struct RejectBytesSerializer;

impl ser::Serializer for RejectBytesSerializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = RejectBytesSeq;
    type SerializeTuple = RejectBytesSeq;
    type SerializeTupleStruct = RejectBytesSeq;
    type SerializeTupleVariant = RejectBytesTupleVariant;
    type SerializeMap = RejectBytesMap;
    type SerializeStruct = RejectBytesStruct;
    type SerializeStructVariant = RejectBytesStructVariant;

    fn serialize_bool(self, _value: bool) -> Result<()> {
        Ok(())
    }

    fn serialize_i8(self, _value: i8) -> Result<()> {
        Ok(())
    }

    fn serialize_i16(self, _value: i16) -> Result<()> {
        Ok(())
    }

    fn serialize_i32(self, _value: i32) -> Result<()> {
        Ok(())
    }

    fn serialize_i64(self, _value: i64) -> Result<()> {
        Ok(())
    }

    fn serialize_i128(self, _value: i128) -> Result<()> {
        Ok(())
    }

    fn serialize_u8(self, _value: u8) -> Result<()> {
        Ok(())
    }

    fn serialize_u16(self, _value: u16) -> Result<()> {
        Ok(())
    }

    fn serialize_u32(self, _value: u32) -> Result<()> {
        Ok(())
    }

    fn serialize_u64(self, _value: u64) -> Result<()> {
        Ok(())
    }

    fn serialize_u128(self, _value: u128) -> Result<()> {
        Ok(())
    }

    fn serialize_f32(self, _value: f32) -> Result<()> {
        Ok(())
    }

    fn serialize_f64(self, _value: f64) -> Result<()> {
        Ok(())
    }

    fn serialize_char(self, _value: char) -> Result<()> {
        Ok(())
    }

    fn serialize_str(self, _value: &str) -> Result<()> {
        Ok(())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
        Err(bytes_unsupported_error())
    }

    fn serialize_none(self) -> Result<()> {
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        Ok(())
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
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
        reject_document_bytes(value)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<RejectBytesSeq> {
        Ok(RejectBytesSeq)
    }

    fn serialize_tuple(self, _len: usize) -> Result<RejectBytesSeq> {
        Ok(RejectBytesSeq)
    }

    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<RejectBytesSeq> {
        Ok(RejectBytesSeq)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<RejectBytesTupleVariant> {
        validate_variant_tag(variant)?;
        Ok(RejectBytesTupleVariant)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<RejectBytesMap> {
        Ok(RejectBytesMap)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<RejectBytesStruct> {
        Ok(RejectBytesStruct)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<RejectBytesStructVariant> {
        validate_variant_tag(variant)?;
        Ok(RejectBytesStructVariant)
    }

    fn collect_str<T>(self, _value: &T) -> Result<()>
    where
        T: ?Sized + std::fmt::Display,
    {
        Ok(())
    }
}

struct RejectBytesSeq;

impl SerializeSeq for RejectBytesSeq {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl SerializeTuple for RejectBytesSeq {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl SerializeTupleStruct for RejectBytesSeq {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

struct RejectBytesTupleVariant;

impl SerializeTupleVariant for RejectBytesTupleVariant {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

struct RejectBytesMap;

impl SerializeMap for RejectBytesMap {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
    where
        K: ?Sized + Serialize,
        V: ?Sized + Serialize,
    {
        reject_document_bytes(key)?;
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

struct RejectBytesStruct;

impl SerializeStruct for RejectBytesStruct {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

struct RejectBytesStructVariant;

impl SerializeStructVariant for RejectBytesStructVariant {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        reject_document_bytes(value)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
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
