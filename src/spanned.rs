//! [`Spanned<T>`] captures the source [`Span`] a value occupied during
//! deserialization, so callers can report *where* a configuration value came
//! from — not only what it was.
//!
//! Error spans only appear on failure; `Spanned<T>` exposes the location of a
//! *successful* read, which is what config linters, language servers, and
//! "this setting came from line N" tooling need. It is built directly on this
//! crate's spanful [`Node`](crate::Node) tree: when a span-bearing deserializer
//! is asked for a private marker struct, it hands back the current node's span
//! alongside the normally deserialized value. No second parse and no retained
//! source buffer are required, because [`Node`](crate::Node) already carries
//! line, column, and byte offsets.
//!
//! ```
//! use serde::Deserialize;
//! use saneyaml::Spanned;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     name: Spanned<String>,
//! }
//!
//! let yaml = "name: api\n";
//! let config: Config = saneyaml::from_str(yaml)?;
//! let name = config.name;
//! assert_eq!(name.line(), 1);
//! assert_eq!(&yaml[name.start()..name.end()], "api");
//! assert_eq!(name.into_inner(), "api");
//! # Ok::<(), saneyaml::Error>(())
//! ```
//!
//! Supported on the span-bearing read paths: [`from_str`](crate::from_str),
//! [`from_slice`](crate::from_slice), and [`from_node`](crate::from_node),
//! including nested struct fields. On the spanless [`from_value`](crate::from_value)
//! path the value still deserializes, but the span is [`Span::default`] (line 0).

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;

use serde::de::{
    self, Deserialize, DeserializeSeed, Deserializer, IntoDeserializer, SeqAccess, Visitor,
};
use serde::ser::{Serialize, Serializer};

use crate::error::{Error, Span};

/// Private marker-struct name a [`Spanned`] read asks for. Spelled so it can
/// never collide with a real Rust type name, mirroring how `toml`/`serde-spanned`
/// smuggle a position request through Serde's type-erased API.
pub(crate) const NAME: &str = "$saneyaml::private::Spanned";

/// Field names paired with [`NAME`]; present only to satisfy the
/// `deserialize_struct` contract. The four span components plus the value are
/// produced positionally as a sequence, so the names are never matched.
pub(crate) const FIELDS: &[&str] = &[
    "$saneyaml::private::start",
    "$saneyaml::private::end",
    "$saneyaml::private::line",
    "$saneyaml::private::column",
    "$saneyaml::private::value",
];

/// A value deserialized from YAML, paired with the source [`Span`] it came from.
///
/// Equality, ordering, and hashing consider only the inner value, never the
/// span, so wrapping a field in `Spanned` does not change its identity as a
/// mapping key or its comparison behavior.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spanned<T> {
    span: Span,
    value: T,
}

impl<T> Spanned<T> {
    /// Creates a spanned value from an explicit span and value.
    pub fn new(span: Span, value: T) -> Self {
        Self { span, value }
    }

    /// Returns the source span the value occupied.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the zero-based byte offset where the value starts.
    pub fn start(&self) -> usize {
        self.span.start
    }

    /// Returns the zero-based byte offset just past the value.
    pub fn end(&self) -> usize {
        self.span.end
    }

    /// Returns the one-based source line of the value start.
    pub fn line(&self) -> usize {
        self.span.line
    }

    /// Returns the one-based UTF-8 byte column of the value start.
    pub fn column(&self) -> usize {
        self.span.column
    }

    /// Returns a reference to the inner value.
    pub fn get_ref(&self) -> &T {
        &self.value
    }

    /// Returns a mutable reference to the inner value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consumes the wrapper, returning the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> AsRef<T> for Spanned<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

impl<T: PartialOrd> PartialOrd for Spanned<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Ord> Ord for Spanned<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T: Hash> Hash for Spanned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<'de, T> Deserialize<'de> for Spanned<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(NAME, FIELDS, SpannedVisitor(PhantomData))
    }
}

/// Serializes transparently as the inner value; the span is a read-side concern.
impl<T: Serialize> Serialize for Spanned<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

struct SpannedVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for SpannedVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Spanned<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a spanned YAML value")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let start = seq
            .next_element::<u64>()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let end = seq
            .next_element::<u64>()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let line = seq
            .next_element::<u64>()?
            .ok_or_else(|| de::Error::invalid_length(2, &self))?;
        let column = seq
            .next_element::<u64>()?
            .ok_or_else(|| de::Error::invalid_length(3, &self))?;
        let value = seq
            .next_element::<T>()?
            .ok_or_else(|| de::Error::invalid_length(4, &self))?;
        Ok(Spanned {
            span: Span::new(start as usize, end as usize, line as usize, column as usize),
            value,
        })
    }
}

/// Answers a [`Spanned`] read from a span-bearing deserializer.
///
/// Each [`Deserializer`](serde::Deserializer) that carries spans calls this from
/// its `deserialize_struct` when it sees [`NAME`], passing the current node span
/// and itself as the value deserializer.
pub(crate) fn deserialize_spanned<'de, D, V>(
    span: Span,
    value: D,
    visitor: V,
) -> Result<V::Value, Error>
where
    D: Deserializer<'de, Error = Error>,
    V: Visitor<'de>,
{
    visitor.visit_seq(SpannedSeq {
        span,
        value: Some(value),
        index: 0,
        _marker: PhantomData,
    })
}

/// A five-element [`SeqAccess`] yielding `start, end, line, column, value`,
/// where the value comes from the wrapped deserializer `D`.
struct SpannedSeq<'de, D> {
    span: Span,
    value: Option<D>,
    index: u8,
    _marker: PhantomData<&'de ()>,
}

impl<'de, D> SeqAccess<'de> for SpannedSeq<'de, D>
where
    D: Deserializer<'de, Error = Error>,
{
    type Error = Error;

    fn next_element_seed<S>(&mut self, seed: S) -> Result<Option<S::Value>, Error>
    where
        S: DeserializeSeed<'de>,
    {
        let index = self.index;
        self.index = self.index.saturating_add(1);
        match index {
            0 => seed
                .deserialize(u64_deserializer(self.span.start as u64))
                .map(Some),
            1 => seed
                .deserialize(u64_deserializer(self.span.end as u64))
                .map(Some),
            2 => seed
                .deserialize(u64_deserializer(self.span.line as u64))
                .map(Some),
            3 => seed
                .deserialize(u64_deserializer(self.span.column as u64))
                .map(Some),
            4 => {
                let value = self
                    .value
                    .take()
                    .expect("spanned value deserializer is consumed exactly once");
                seed.deserialize(value).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(usize::from(5u8.saturating_sub(self.index)))
    }
}

fn u64_deserializer(value: u64) -> serde::de::value::U64Deserializer<Error> {
    value.into_deserializer()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize, Serialize)]
    struct Config {
        name: Spanned<String>,
        port: Spanned<u16>,
    }

    #[test]
    fn captures_field_spans_from_str() {
        let yaml = "name: api\nport: 8080\n";
        let config: Config = crate::from_str(yaml).unwrap();

        assert_eq!(config.name.get_ref().as_str(), "api");
        assert_eq!(config.name.line(), 1);
        assert_eq!(&yaml[config.name.start()..config.name.end()], "api");

        assert_eq!(*config.port.get_ref(), 8080);
        assert_eq!(config.port.line(), 2);
        assert_eq!(&yaml[config.port.start()..config.port.end()], "8080");
    }

    #[test]
    fn deref_and_into_inner() {
        let yaml = "name: web\nport: 80\n";
        let config: Config = crate::from_str(yaml).unwrap();
        // Deref to the inner value.
        assert_eq!(config.name.len(), 3);
        assert_eq!(config.name.into_inner(), "web");
    }

    #[test]
    fn equality_ignores_span() {
        let left: Spanned<String> = crate::from_str("a\n").unwrap();
        let right = Spanned::new(Span::new(99, 100, 7, 7), "a".to_string());
        // Different spans, same value: equal.
        assert_eq!(left, right);
    }

    #[test]
    fn serializes_transparently() {
        let yaml = "name: api\nport: 8080\n";
        let config: Config = crate::from_str(yaml).unwrap();
        let emitted = crate::to_string(&config).unwrap();
        assert!(emitted.contains("name: api"), "got: {emitted}");
        assert!(emitted.contains("port: 8080"), "got: {emitted}");
        // No span leakage in the emitted form.
        assert!(!emitted.contains("private"), "got: {emitted}");
    }

    #[test]
    fn from_value_is_spanless_but_reads_value() {
        let value = crate::Value::from("api");
        let spanned: Spanned<String> = crate::from_value(value).unwrap();
        assert_eq!(spanned.get_ref().as_str(), "api");
        // Spanless path: default (zero) span.
        assert_eq!(spanned.line(), 0);
        assert_eq!(spanned.span(), Span::default());
    }
}
