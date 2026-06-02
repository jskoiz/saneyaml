//! Serde helper modules matching selected `serde_yaml::with` paths.
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, PartialEq, Deserialize, Serialize)]
//! enum Mode {
//!     Http { port: u16 },
//! }
//!
//! #[derive(Deserialize, Serialize)]
//! struct Config {
//!     #[serde(with = "yaml::with::singleton_map")]
//!     mode: Mode,
//! }
//!
//! let config: Config = yaml::from_str("mode:\n  Http:\n    port: 8080\n")?;
//! assert_eq!(config.mode, Mode::Http { port: 8080 });
//!
//! let output = yaml::to_string(&config)?;
//! assert!(output.contains("Http"));
//! # Ok::<(), yaml::Error>(())
//! ```

use crate::{Mapping, Value};

fn tagged_to_singleton_map(value: Value, recursive: bool) -> Value {
    match value {
        Value::Tagged(tagged) => {
            let key = Value::String(tagged.tag.serde_variant().into_owned());
            let value = if recursive {
                tagged_to_singleton_map(tagged.value, true)
            } else {
                tagged.value
            };
            let mut mapping = Mapping::with_capacity(1);
            mapping.insert(key, value);
            Value::Mapping(mapping)
        }
        Value::Sequence(items) if recursive => Value::Sequence(
            items
                .into_iter()
                .map(|value| tagged_to_singleton_map(value, true))
                .collect(),
        ),
        Value::Mapping(entries) if recursive => Value::Mapping(
            entries
                .into_iter()
                .map(|(key, value)| {
                    (
                        tagged_to_singleton_map(key, true),
                        tagged_to_singleton_map(value, true),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// Deserialize enum values from a singleton mapping representation.
///
/// The read-side helper mirrors `serde_yaml::with::singleton_map`: scalar unit
/// variants remain accepted, data-carrying variants must be represented as a
/// mapping with exactly one variant key, and YAML tag-style enum input is not
/// treated as a singleton map.
pub mod singleton_map {
    use crate::with::tagged_to_singleton_map;
    use serde::de::{
        self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess,
        Unexpected, VariantAccess, Visitor,
    };
    use serde::{Serialize, Serializer, ser::Error};
    use std::fmt;

    /// Deserializes a value through the singleton-map helper path.
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(SingletonMap {
            delegate: deserializer,
        })
    }

    /// Serializes enum tags as one-entry YAML mappings.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize,
        S: Serializer,
    {
        let value = crate::to_value(value).map_err(S::Error::custom)?;
        tagged_to_singleton_map(value, false).serialize(serializer)
    }

    struct SingletonMap<D> {
        delegate: D,
    }

    impl<'de, D> Deserializer<'de> for SingletonMap<D>
    where
        D: Deserializer<'de>,
    {
        type Error = D::Error;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_any(visitor)
        }

        fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_bool(visitor)
        }

        fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_i8(visitor)
        }

        fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_i16(visitor)
        }

        fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_i32(visitor)
        }

        fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_i64(visitor)
        }

        fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_i128(visitor)
        }

        fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_u8(visitor)
        }

        fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_u16(visitor)
        }

        fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_u32(visitor)
        }

        fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_u64(visitor)
        }

        fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_u128(visitor)
        }

        fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_f32(visitor)
        }

        fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_f64(visitor)
        }

        fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_char(visitor)
        }

        fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_str(visitor)
        }

        fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_string(visitor)
        }

        fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_bytes(visitor)
        }

        fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_byte_buf(visitor)
        }

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_option(SingletonMapAsEnum {
                name: "",
                delegate: visitor,
            })
        }

        fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_unit(visitor)
        }

        fn deserialize_unit_struct<V>(
            self,
            name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_unit_struct(name, visitor)
        }

        fn deserialize_newtype_struct<V>(
            self,
            name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_newtype_struct(name, visitor)
        }

        fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_seq(visitor)
        }

        fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_tuple(len, visitor)
        }

        fn deserialize_tuple_struct<V>(
            self,
            name: &'static str,
            len: usize,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_tuple_struct(name, len, visitor)
        }

        fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_map(visitor)
        }

        fn deserialize_struct<V>(
            self,
            name: &'static str,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_struct(name, fields, visitor)
        }

        fn deserialize_enum<V>(
            self,
            name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_any(SingletonMapAsEnum {
                name,
                delegate: visitor,
            })
        }

        fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_identifier(visitor)
        }

        fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_ignored_any(visitor)
        }

        fn is_human_readable(&self) -> bool {
            self.delegate.is_human_readable()
        }
    }

    struct SingletonMapAsEnum<D> {
        name: &'static str,
        delegate: D,
    }

    impl<'de, V> Visitor<'de> for SingletonMapAsEnum<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.delegate.expecting(formatter)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::StrDeserializer::new(value))
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::BorrowedStrDeserializer::new(value))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::StringDeserializer::new(value))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_none()
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            self.delegate.visit_some(SingletonMap {
                delegate: deserializer,
            })
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_unit()
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            self.delegate.visit_enum(SingletonMapAsEnum {
                name: self.name,
                delegate: map,
            })
        }
    }

    impl<'de, D> EnumAccess<'de> for SingletonMapAsEnum<D>
    where
        D: MapAccess<'de>,
    {
        type Error = D::Error;
        type Variant = Self;

        fn variant_seed<V>(mut self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
        where
            V: DeserializeSeed<'de>,
        {
            match self.delegate.next_key_seed(seed)? {
                Some(value) => Ok((value, self)),
                None => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }
    }

    impl<'de, D> VariantAccess<'de> for SingletonMapAsEnum<D>
    where
        D: MapAccess<'de>,
    {
        type Error = D::Error;

        fn unit_variant(self) -> Result<(), Self::Error> {
            Err(de::Error::invalid_type(Unexpected::Map, &"unit variant"))
        }

        fn newtype_variant_seed<T>(mut self, seed: T) -> Result<T::Value, Self::Error>
        where
            T: DeserializeSeed<'de>,
        {
            let value = self.delegate.next_value_seed(seed)?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }

        fn tuple_variant<V>(mut self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self
                .delegate
                .next_value_seed(TupleVariantSeed { len, visitor })?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }

        fn struct_variant<V>(
            mut self,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self.delegate.next_value_seed(StructVariantSeed {
                name: self.name,
                fields,
                visitor,
            })?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }
    }

    struct TupleVariantSeed<V> {
        len: usize,
        visitor: V,
    }

    impl<'de, V> DeserializeSeed<'de> for TupleVariantSeed<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_tuple(self.len, self.visitor)
        }
    }

    struct StructVariantSeed<V> {
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    }

    impl<'de, V> DeserializeSeed<'de> for StructVariantSeed<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct(self.name, self.fields, self.visitor)
        }
    }
}

/// Recursively deserialize enum values from singleton mapping representation.
///
/// `serde_yaml::with::singleton_map_recursive` rewrites nested enum
/// deserialization to use single-key mappings. This helper applies the same
/// enum-shape rule recursively through sequences, mappings, options, structs,
/// and newtype wrappers.
pub mod singleton_map_recursive {
    use crate::with::tagged_to_singleton_map;
    use serde::de::{
        self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess,
        SeqAccess, Unexpected, VariantAccess, Visitor,
    };
    use serde::{Serialize, Serializer, ser::Error};
    use std::fmt;

    /// Deserializes a value through the recursive singleton-map helper path.
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(SingletonMapRecursive {
            delegate: deserializer,
        })
    }

    /// Serializes nested enum tags as one-entry YAML mappings.
    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize,
        S: Serializer,
    {
        let value = crate::to_value(value).map_err(S::Error::custom)?;
        tagged_to_singleton_map(value, true).serialize(serializer)
    }

    struct SingletonMapRecursive<D> {
        delegate: D,
    }

    impl<'de, D> Deserializer<'de> for SingletonMapRecursive<D>
    where
        D: Deserializer<'de>,
    {
        type Error = D::Error;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_any(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_bool(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_i8(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_i16(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_i32(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_i64(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_i128(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_u8(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_u16(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_u32(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_u64(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_u128(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_f32(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_f64(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_char(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_str(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_string(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_bytes(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_byte_buf(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_option(SingletonMapRecursiveAsEnum {
                    name: "",
                    delegate: visitor,
                })
        }

        fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_unit(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_unit_struct<V>(
            self,
            name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_unit_struct(name, SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_newtype_struct<V>(
            self,
            name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_newtype_struct(name, SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_seq(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_tuple(len, SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_tuple_struct<V>(
            self,
            name: &'static str,
            len: usize,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_tuple_struct(
                name,
                len,
                SingletonMapRecursive { delegate: visitor },
            )
        }

        fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_map(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_struct<V>(
            self,
            name: &'static str,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_struct(
                name,
                fields,
                SingletonMapRecursive { delegate: visitor },
            )
        }

        fn deserialize_enum<V>(
            self,
            name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate.deserialize_any(SingletonMapRecursiveAsEnum {
                name,
                delegate: visitor,
            })
        }

        fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_identifier(SingletonMapRecursive { delegate: visitor })
        }

        fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.delegate
                .deserialize_ignored_any(SingletonMapRecursive { delegate: visitor })
        }

        fn is_human_readable(&self) -> bool {
            self.delegate.is_human_readable()
        }
    }

    impl<'de, V> Visitor<'de> for SingletonMapRecursive<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.delegate.expecting(formatter)
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_bool(value)
        }

        fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_i8(value)
        }

        fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_i16(value)
        }

        fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_i32(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_i64(value)
        }

        fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_i128(value)
        }

        fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_u8(value)
        }

        fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_u16(value)
        }

        fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_u32(value)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_u64(value)
        }

        fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_u128(value)
        }

        fn visit_f32<E>(self, value: f32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_f32(value)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_f64(value)
        }

        fn visit_char<E>(self, value: char) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_char(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_str(value)
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_borrowed_str(value)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_string(value)
        }

        fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_bytes(value)
        }

        fn visit_borrowed_bytes<E>(self, value: &'de [u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_borrowed_bytes(value)
        }

        fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_byte_buf(value)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_none()
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            self.delegate.visit_some(SingletonMapRecursive {
                delegate: deserializer,
            })
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_unit()
        }

        fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            self.delegate.visit_newtype_struct(SingletonMapRecursive {
                delegate: deserializer,
            })
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            self.delegate
                .visit_seq(SingletonMapRecursive { delegate: seq })
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            self.delegate
                .visit_map(SingletonMapRecursive { delegate: map })
        }
    }

    impl<'de, T> DeserializeSeed<'de> for SingletonMapRecursive<T>
    where
        T: DeserializeSeed<'de>,
    {
        type Value = T::Value;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            self.delegate.deserialize(SingletonMapRecursive {
                delegate: deserializer,
            })
        }
    }

    impl<'de, S> SeqAccess<'de> for SingletonMapRecursive<S>
    where
        S: SeqAccess<'de>,
    {
        type Error = S::Error;

        fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
        where
            T: DeserializeSeed<'de>,
        {
            self.delegate
                .next_element_seed(SingletonMapRecursive { delegate: seed })
        }
    }

    impl<'de, M> MapAccess<'de> for SingletonMapRecursive<M>
    where
        M: MapAccess<'de>,
    {
        type Error = M::Error;

        fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
        where
            K: DeserializeSeed<'de>,
        {
            self.delegate
                .next_key_seed(SingletonMapRecursive { delegate: seed })
        }

        fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
        where
            V: DeserializeSeed<'de>,
        {
            self.delegate
                .next_value_seed(SingletonMapRecursive { delegate: seed })
        }
    }

    struct SingletonMapRecursiveAsEnum<D> {
        name: &'static str,
        delegate: D,
    }

    impl<'de, V> Visitor<'de> for SingletonMapRecursiveAsEnum<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.delegate.expecting(formatter)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::StrDeserializer::new(value))
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::BorrowedStrDeserializer::new(value))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate
                .visit_enum(de::value::StringDeserializer::new(value))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_none()
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            self.delegate.visit_some(SingletonMapRecursive {
                delegate: deserializer,
            })
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.delegate.visit_unit()
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            self.delegate.visit_enum(SingletonMapRecursiveAsEnum {
                name: self.name,
                delegate: map,
            })
        }
    }

    impl<'de, D> EnumAccess<'de> for SingletonMapRecursiveAsEnum<D>
    where
        D: MapAccess<'de>,
    {
        type Error = D::Error;
        type Variant = Self;

        fn variant_seed<V>(mut self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
        where
            V: DeserializeSeed<'de>,
        {
            match self.delegate.next_key_seed(seed)? {
                Some(value) => Ok((value, self)),
                None => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }
    }

    impl<'de, D> VariantAccess<'de> for SingletonMapRecursiveAsEnum<D>
    where
        D: MapAccess<'de>,
    {
        type Error = D::Error;

        fn unit_variant(self) -> Result<(), Self::Error> {
            Err(de::Error::invalid_type(Unexpected::Map, &"unit variant"))
        }

        fn newtype_variant_seed<T>(mut self, seed: T) -> Result<T::Value, Self::Error>
        where
            T: DeserializeSeed<'de>,
        {
            let value = self
                .delegate
                .next_value_seed(SingletonMapRecursive { delegate: seed })?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }

        fn tuple_variant<V>(mut self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self.delegate.next_value_seed(TupleVariantSeed {
                len,
                visitor: SingletonMapRecursive { delegate: visitor },
            })?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }

        fn struct_variant<V>(
            mut self,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self.delegate.next_value_seed(StructVariantSeed {
                name: self.name,
                fields,
                visitor: SingletonMapRecursive { delegate: visitor },
            })?;
            match self.delegate.next_key()? {
                None => Ok(value),
                Some(IgnoredAny) => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }
    }

    struct TupleVariantSeed<V> {
        len: usize,
        visitor: V,
    }

    impl<'de, V> DeserializeSeed<'de> for TupleVariantSeed<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_tuple(self.len, self.visitor)
        }
    }

    struct StructVariantSeed<V> {
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    }

    impl<'de, V> DeserializeSeed<'de> for StructVariantSeed<V>
    where
        V: Visitor<'de>,
    {
        type Value = V::Value;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct(self.name, self.fields, self.visitor)
        }
    }
}
