//! Serde helper modules matching selected `serde_yaml::with` paths.

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
/// This crate's deserializer already accepts both scalar enum variants and
/// single-key mapping variants. The helper exists so read-side replacement code
/// using `#[serde(with = "serde_yaml::with::singleton_map")]` can move to the
/// corresponding `yaml::with::singleton_map` path without changing data shape.
pub mod singleton_map {
    use crate::with::tagged_to_singleton_map;
    use serde::{Deserialize, Deserializer};
    use serde::{Serialize, Serializer, ser::Error};

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer)
    }

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize,
        S: Serializer,
    {
        let value = crate::to_value(value).map_err(S::Error::custom)?;
        tagged_to_singleton_map(value, false).serialize(serializer)
    }
}

/// Recursively deserialize enum values from singleton mapping representation.
///
/// `serde_yaml::with::singleton_map_recursive` rewrites nested enum
/// deserialization to use single-key mappings. This crate's deserializer
/// already accepts that representation recursively, so the read-side helper is
/// a public API compatibility shim for `#[serde(with = "...")]` users.
pub mod singleton_map_recursive {
    use crate::with::tagged_to_singleton_map;
    use serde::{Deserialize, Deserializer};
    use serde::{Serialize, Serializer, ser::Error};

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer)
    }

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize,
        S: Serializer,
    {
        let value = crate::to_value(value).map_err(S::Error::custom)?;
        tagged_to_singleton_map(value, true).serialize(serializer)
    }
}
