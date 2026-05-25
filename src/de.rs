use crate::parse::parse_document_results;
use crate::{
    Error, Mapping, Node, NodeValue, Number, Span, Tag, Value, error::utf8_error_span,
    parse_documents, parse_str,
};
use serde::de::{
    self, DeserializeOwned, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::forward_to_deserialize_any;
use std::io::Read;

pub fn from_str<'de, T>(input: &'de str) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    let node = parse_str(input)?;
    from_input_node(&node, input)
}

pub fn from_slice<'de, T>(input: &'de [u8]) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    let input = std::str::from_utf8(input)
        .map_err(|err| Error::new("input is not valid UTF-8", utf8_error_span(input, err)))?;
    from_str(input)
}

pub fn from_reader<R, T>(mut reader: R) -> crate::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut input = Vec::new();
    reader
        .read_to_end(&mut input)
        .map_err(|err| Error::new(format!("failed to read YAML input: {err}"), Span::default()))?;
    from_slice(&input)
}

pub fn from_node<'de, T>(node: &'de Node) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(node).map_err(|error| error.with_span_if_missing(node.span))
}

fn from_input_node<'de, T>(node: &Node, input: &'de str) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(InputNode { node, input }).map_err(|error| error.with_span_if_missing(node.span))
}

pub fn from_value<T>(value: Value) -> crate::Result<T>
where
    T: DeserializeOwned,
{
    T::deserialize(value)
}

pub fn from_documents_str<T>(input: &str) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    parse_documents(input)?
        .iter()
        .map(from_node)
        .collect::<crate::Result<Vec<T>>>()
}

pub fn from_documents_slice<T>(input: &[u8]) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let input = std::str::from_utf8(input)
        .map_err(|err| Error::new("input is not valid UTF-8", utf8_error_span(input, err)))?;
    from_documents_str(input)
}

pub fn from_documents_reader<T, R>(mut reader: R) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
    R: Read,
{
    let mut input = Vec::new();
    reader
        .read_to_end(&mut input)
        .map_err(|err| Error::new(format!("failed to read YAML input: {err}"), Span::default()))?;
    from_documents_slice(&input)
}

#[derive(Debug)]
pub struct Deserializer<'de> {
    documents: std::vec::IntoIter<Document<'de>>,
}

impl<'de> Deserializer<'de> {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &'de str) -> Self {
        Self::from_document_results(parse_document_results(input), Some(input))
    }

    pub fn from_slice(input: &'de [u8]) -> Self {
        match std::str::from_utf8(input) {
            Ok(input) => Self::from_str(input),
            Err(err) => Self::from_parse_result(Err(Error::new(
                "input is not valid UTF-8",
                utf8_error_span(input, err),
            ))),
        }
    }

    pub fn from_reader<R>(mut reader: R) -> Self
    where
        R: Read,
    {
        let mut input = Vec::new();
        match reader.read_to_end(&mut input) {
            Ok(_) => match std::str::from_utf8(&input) {
                Ok(input) => Self::from_document_results(parse_document_results(input), None),
                Err(err) => Self::from_parse_result(Err(Error::new(
                    "input is not valid UTF-8",
                    utf8_error_span(&input, err),
                ))),
            },
            Err(err) => Self::from_parse_result(Err(Error::new(
                format!("failed to read YAML input: {err}"),
                Span::default(),
            ))),
        }
    }

    fn from_parse_result(result: crate::Result<Vec<Node>>) -> Self {
        let documents = match result {
            Ok(documents) => documents
                .into_iter()
                .map(|node| Document {
                    node: Ok(node),
                    input: None,
                })
                .collect(),
            Err(error) => vec![Document {
                node: Err(error),
                input: None,
            }],
        };
        Self {
            documents: documents.into_iter(),
        }
    }

    fn from_document_results(results: Vec<crate::Result<Node>>, input: Option<&'de str>) -> Self {
        Self {
            documents: results
                .into_iter()
                .map(|node| Document { node, input })
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }

    fn into_single_document(mut self) -> Result<Document<'de>, Error> {
        let Some(document) = self.documents.next() else {
            return Ok(Document {
                node: Ok(Node::null(Span::point(0, 1, 1))),
                input: None,
            });
        };
        if let Some(extra) = self.documents.next() {
            let span = extra
                .node
                .as_ref()
                .map(|node| node.span)
                .unwrap_or_else(|error| error.span());
            return Err(Error::new(
                "expected a single YAML document; use the iterator API for streams",
                Some(span),
            ));
        }
        Ok(document)
    }
}

impl<'de> Iterator for Deserializer<'de> {
    type Item = Deserializer<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        self.documents.next().map(|document| Deserializer {
            documents: vec![document].into_iter(),
        })
    }
}

#[derive(Debug)]
struct Document<'de> {
    node: crate::Result<Node>,
    input: Option<&'de str>,
}

impl<'de> Document<'de> {
    fn as_node(&self) -> Result<&Node, Error> {
        self.node.as_ref().map_err(Clone::clone)
    }

    fn into_node(self) -> Result<Node, Error> {
        self.node
    }

    fn into_node_and_input(self) -> Result<(Node, Option<&'de str>), Error> {
        let input = self.input;
        self.into_node().map(|node| (node, input))
    }
}

fn with_span<T>(result: Result<T, Error>, span: Span) -> Result<T, Error> {
    result.map_err(|error| error.with_span_if_missing(span))
}

fn with_optional_span<T>(result: Result<T, Error>, span: Option<Span>) -> Result<T, Error> {
    match span {
        Some(span) => with_span(result, span),
        None => result,
    }
}

fn is_empty_null_node(node: &Node) -> bool {
    matches!(node.value, NodeValue::Null)
        && node
            .scalar_source()
            .is_none_or(|source| source.raw().is_empty())
}

fn string_source_for_scalar(node: &Node) -> Option<&str> {
    match node.value {
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
            node.scalar_source().map(|source| source.raw())
        }
        _ => None,
    }
}

fn integer_source_for_scalar(node: &Node) -> Option<&str> {
    match node.value {
        NodeValue::String(_) => node.scalar_source().map(|source| source.raw()),
        _ => None,
    }
}

fn parse_i128_source(raw: &str, span: Span) -> Result<i128, Error> {
    raw.replace('_', "")
        .parse::<i128>()
        .map_err(|_| Error::new("integer scalar is out of range for i128", Some(span)))
}

fn parse_u128_source(raw: &str, span: Span) -> Result<u128, Error> {
    raw.replace('_', "").parse::<u128>().map_err(|_| {
        Error::new(
            "integer scalar is out of range for unsigned integer",
            Some(span),
        )
    })
}

#[derive(Clone, Copy)]
struct CoercedNumber {
    number: Number,
    span: Option<Span>,
}

fn explicit_core_int_number_node(node: &Node) -> Result<Option<CoercedNumber>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "int") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: Some(node.span),
        })),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_int_text(raw, Some(node.span)).map(|number| {
                Some(CoercedNumber {
                    number,
                    span: Some(node.span),
                })
            })
        }
        _ => Err(type_error("integer", node)),
    }
}

fn explicit_core_float_number_node(node: &Node) -> Result<Option<CoercedNumber>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "float") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: Some(node.span),
        })),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_float_text(raw, Some(node.span)).map(|number| {
                Some(CoercedNumber {
                    number,
                    span: Some(node.span),
                })
            })
        }
        _ => Err(type_error("number", node)),
    }
}

fn explicit_core_tagged_node<'a>(mut node: &'a Node, suffix: &str) -> Option<&'a Node> {
    while let NodeValue::Tagged(tagged) = &node.value {
        if tagged.tag.handle == "!!" && tagged.tag.suffix == suffix {
            return Some(&tagged.value);
        }
        node = &tagged.value;
    }
    None
}

fn explicit_core_int_number_value(value: &Value) -> Result<Option<CoercedNumber>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "int") else {
        return Ok(None);
    };
    match value {
        Value::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: None,
        })),
        Value::String(value) => parse_explicit_core_int_text(value, None)
            .map(|number| Some(CoercedNumber { number, span: None })),
        other => Err(type_error_value("integer", other)),
    }
}

fn explicit_core_float_number_value(value: &Value) -> Result<Option<CoercedNumber>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "float") else {
        return Ok(None);
    };
    match value {
        Value::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: None,
        })),
        Value::String(value) => parse_explicit_core_float_text(value, None)
            .map(|number| Some(CoercedNumber { number, span: None })),
        other => Err(type_error_value("number", other)),
    }
}

fn explicit_core_tagged_value<'a>(mut value: &'a Value, suffix: &str) -> Option<&'a Value> {
    while let Value::Tagged(tagged) = value {
        if tagged.tag.handle == "!!" && tagged.tag.suffix == suffix {
            return Some(&tagged.value);
        }
        value = &tagged.value;
    }
    None
}

fn parse_explicit_core_int_text(raw: &str, span: Option<Span>) -> Result<Number, Error> {
    let compact = raw.replace('_', "");
    let (negative, rest) = match compact.as_str() {
        text if text.starts_with('-') => (true, &text[1..]),
        text if text.starts_with('+') => (false, &text[1..]),
        text => (false, text),
    };
    let (radix, digits) =
        if let Some(digits) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            (16, digits)
        } else if let Some(digits) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
            (8, digits)
        } else if let Some(digits) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
            (2, digits)
        } else {
            (10, rest)
        };

    if digits.is_empty() {
        return Err(Error::new("failed to parse explicit !!int scalar", span));
    }

    let magnitude = u128::from_str_radix(digits, radix)
        .map_err(|_| Error::new("failed to parse explicit !!int scalar", span))?;
    if negative {
        let min_magnitude = i128::MAX as u128 + 1;
        if magnitude == min_magnitude {
            Ok(Number::Integer(i128::MIN))
        } else {
            i128::try_from(magnitude)
                .map(|value| Number::Integer(-value))
                .map_err(|_| Error::new("integer scalar is out of range for i128", span))
        }
    } else if let Ok(value) = i128::try_from(magnitude) {
        Ok(Number::Integer(value))
    } else {
        Ok(Number::Unsigned(magnitude))
    }
}

fn parse_explicit_core_float_text(raw: &str, span: Option<Span>) -> Result<Number, Error> {
    let compact = raw.replace('_', "");
    if compact.eq_ignore_ascii_case(".nan") {
        return Ok(Number::from(f64::NAN));
    }
    if compact.eq_ignore_ascii_case(".inf") || compact.eq_ignore_ascii_case("+.inf") {
        return Ok(Number::from(f64::INFINITY));
    }
    if compact.eq_ignore_ascii_case("-.inf") {
        return Ok(Number::from(f64::NEG_INFINITY));
    }
    compact
        .parse::<f64>()
        .map(Number::from)
        .map_err(|_| Error::new("failed to parse explicit !!float scalar", span))
}

fn visit_i64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => {
            let value = i64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for i64", span))?;
            with_optional_span(visitor.visit_i64(value), span)
        }
        Number::Unsigned(value) => match i64::try_from(value) {
            Ok(value) => with_optional_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::new("expected integer, found unsigned integer", span)),
        },
        Number::Float(_) => Err(Error::new("expected integer, found float", span)),
    }
}

fn visit_u64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => {
            let value = u64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for u64", span))?;
            with_optional_span(visitor.visit_u64(value), span)
        }
        Number::Unsigned(value) => {
            let value = u64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for u64", span))?;
            with_optional_span(visitor.visit_u64(value), span)
        }
        Number::Integer(_) => Err(Error::new("expected unsigned integer, found integer", span)),
        Number::Float(_) => Err(Error::new("expected unsigned integer, found float", span)),
    }
}

fn visit_i128_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_optional_span(visitor.visit_i128(value), span),
        Number::Unsigned(value) => match i128::try_from(value) {
            Ok(value) => with_optional_span(visitor.visit_i128(value), span),
            Err(_) => Err(Error::new("integer scalar is out of range for i128", span)),
        },
        Number::Float(_) => Err(Error::new("expected integer, found float", span)),
    }
}

fn visit_u128_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => with_optional_span(
            visitor.visit_u128(u128::try_from(value).expect("non-negative i128 fits u128")),
            span,
        ),
        Number::Unsigned(value) => with_optional_span(visitor.visit_u128(value), span),
        Number::Integer(_) => Err(Error::new("expected unsigned integer, found integer", span)),
        Number::Float(_) => Err(Error::new("expected unsigned integer, found float", span)),
    }
}

fn visit_f64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let result = match number {
        Number::Integer(value) => visitor.visit_f64(value as f64),
        Number::Unsigned(value) => visitor.visit_f64(value as f64),
        Number::Float(value) => visitor.visit_f64(value),
    };
    with_optional_span(result, span)
}

fn visit_any_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let result = match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => visitor.visit_i64(value),
            Err(_) => visitor.visit_i128(value),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => visitor.visit_u64(value),
            Err(_) => visitor.visit_u128(value),
        },
        Number::Float(value) => visitor.visit_f64(value),
    };
    with_optional_span(result, span)
}

#[derive(Clone, Copy)]
struct InputNode<'tree, 'de> {
    node: &'tree Node,
    input: &'de str,
}

impl<'tree, 'de> InputNode<'tree, 'de> {
    fn untag(self) -> Self {
        let mut node = self.node;
        while let NodeValue::Tagged(tagged) = &node.value {
            node = &tagged.value;
        }
        Self { node, ..self }
    }

    fn borrowed_str(self) -> Option<&'de str> {
        let node = self.untag().node;
        let raw = self.input.get(node.span.start..node.span.end)?;
        match &node.value {
            NodeValue::String(value) => borrowable_string(raw, value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
                let source = string_source_for_scalar(node)?;
                (raw == source).then_some(raw)
            }
            _ => None,
        }
    }

    fn transient_str(self) -> Option<&'tree str> {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => Some(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
                string_source_for_scalar(node)
            }
            _ => None,
        }
    }
}

fn borrowable_string<'de>(raw: &'de str, value: &str) -> Option<&'de str> {
    if raw == value {
        return Some(raw);
    }

    let quote = raw.chars().next()?;
    if !matches!(quote, '"' | '\'') || !raw.ends_with(quote) || raw.len() < 2 {
        return None;
    }
    let inner = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    (inner == value).then_some(inner)
}

impl<'de, 'tree> de::Deserializer<'de> for InputNode<'tree, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.node.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            NodeValue::Number(number) => visit_any_number(*number, Some(self.node.span), visitor),
            NodeValue::String(_) => match self.borrowed_str() {
                Some(value) => visitor.visit_borrowed_str(value),
                None => visitor.visit_str(
                    self.transient_str()
                        .expect("string node has transient string value"),
                ),
            },
            NodeValue::Sequence(items) => visitor.visit_seq(InputSeqDeserializer {
                items,
                input: self.input,
                index: 0,
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(InputMapDeserializer {
                entries,
                input: self.input,
                index: 0,
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(InputTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: InputNode {
                    node: &tagged.value,
                    input: self.input,
                },
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            _ => Err(type_error("bool", node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(type_error("integer", node)),
            },
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(self.node)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(*value), node.span)
            }
            _ => Err(type_error("number", node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(type_error("char", node)),
                }
            }
            _ => Err(type_error("char", node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = self.borrowed_str() {
            return visitor.visit_borrowed_str(value);
        }
        if let Some(value) = self.transient_str() {
            return visitor.visit_str(value);
        }
        Err(type_error("string", self.untag().node))
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => visitor.visit_string(value.clone()),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_string(
                    string_source_for_scalar(node)
                        .expect("scalar source checked")
                        .to_string(),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = self.borrowed_str() {
            return visitor.visit_borrowed_bytes(value.as_bytes());
        }
        if let Some(value) = self.transient_str() {
            return visitor.visit_bytes(value.as_bytes());
        }
        Err(type_error("bytes", self.untag().node))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => visitor.visit_byte_buf(value.as_bytes().to_vec()),
            _ => Err(type_error("bytes", node)),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag();
        match &node.node.value {
            NodeValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(node),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", node)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self.untag())
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag();
        if is_empty_null_node(node.node) {
            return visitor.visit_seq(InputSeqDeserializer {
                items: &[],
                input: node.input,
                index: 0,
            });
        }
        match &node.node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(InputSeqDeserializer {
                items,
                input: node.input,
                index: 0,
            }),
            _ => Err(type_error("sequence", node.node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag();
        if is_empty_null_node(node.node) {
            return visitor.visit_map(InputMapDeserializer {
                entries: &[],
                input: node.input,
                index: 0,
                value: None,
            });
        }
        match &node.node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(InputMapDeserializer {
                entries,
                input: node.input,
                index: 0,
                value: None,
            }),
            _ => Err(type_error("mapping", node.node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.node.value {
            NodeValue::String(variant) => {
                if let Some(variant) = self.borrowed_str() {
                    visitor.visit_enum(variant.into_deserializer())
                } else {
                    visitor.visit_enum(variant.clone().into_deserializer())
                }
            }
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                visitor.visit_enum(InputEnumDeserializer {
                    key: InputNode {
                        node: &entries[0].0,
                        input: self.input,
                    },
                    value: Some(InputNode {
                        node: &entries[0].1,
                        input: self.input,
                    }),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(InputTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: InputNode {
                    node: &tagged.value,
                    input: self.input,
                },
            }),
            _ => Err(type_error("enum string or single-key mapping", self.node)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for &'de Node {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            NodeValue::Number(number) => visit_any_number(*number, Some(self.span), visitor),
            NodeValue::String(value) => visitor.visit_borrowed_str(value),
            NodeValue::Sequence(items) => visitor.visit_seq(SeqDeserializer { items, index: 0 }),
            NodeValue::Mapping(entries) => visitor.visit_map(MapDeserializer {
                entries,
                index: 0,
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(TaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            _ => Err(type_error("bool", node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(type_error("integer", node)),
            },
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(*value), node.span)
            }
            _ => Err(type_error("number", node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(type_error("char", node)),
                }
            }
            _ => Err(type_error("char", node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_borrowed_str(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_borrowed_str(
                    string_source_for_scalar(node).expect("scalar source checked"),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_string(value.clone()),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_string(
                    string_source_for_scalar(node)
                        .expect("scalar source checked")
                        .to_string(),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_borrowed_bytes(value.as_bytes()),
            _ => Err(type_error("bytes", node)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_byte_buf(value.as_bytes().to_vec()),
            _ => Err(type_error("bytes", node)),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(node),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", node)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_node(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        if is_empty_null_node(node) {
            return visitor.visit_seq(SeqDeserializer {
                items: &[],
                index: 0,
            });
        }
        match &node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(SeqDeserializer { items, index: 0 }),
            _ => Err(type_error("sequence", node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        if is_empty_null_node(node) {
            return visitor.visit_map(MapDeserializer {
                entries: &[],
                index: 0,
                value: None,
            });
        }
        match &node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(MapDeserializer {
                entries,
                index: 0,
                value: None,
            }),
            _ => Err(type_error("mapping", node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.as_str().into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                visitor.visit_enum(EnumDeserializer {
                    key: &entries[0].0,
                    value: Some(&entries[0].1),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(TaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
            _ => Err(type_error("enum string or single-key mapping", self)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Node {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let span = self.span;
        match self.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(value),
            NodeValue::Number(number) => visit_any_number(number, Some(span), visitor),
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Sequence(items) => visitor.visit_seq(OwnedSeqDeserializer {
                items: items.into_iter(),
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(OwnedMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(OwnedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Bool(value) => visitor.visit_bool(value),
            other => Err(type_error_owned("bool", &other, node.span)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(Error::new(
                    "expected integer, found unsigned integer",
                    Some(node.span),
                )),
            },
            other => Err(type_error_owned("integer", &other, node.span)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) if value >= 0 => {
                let value = u64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            other => Err(type_error_owned("unsigned integer", &other, node.span)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(&node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(&node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            other => Err(type_error_owned("integer", other, node.span)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(&node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(&node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            other => Err(type_error_owned("unsigned integer", other, node.span)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(value), node.span)
            }
            other => Err(type_error_owned("number", &other, node.span)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", Some(node.span))),
                }
            }
            other => Err(type_error_owned("char", &other, node.span)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if node.source.is_some() =>
            {
                visitor.visit_string(
                    node.source
                        .expect("scalar source checked")
                        .raw()
                        .to_string(),
                )
            }
            other => Err(type_error_owned("string", &other, node.span)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::String(value) => visitor.visit_byte_buf(value.into_bytes()),
            other => Err(type_error_owned("bytes", &other, node.span)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Null => visitor.visit_none(),
            value => visitor.visit_some(Node {
                value,
                span: node.span,
                source: node.source,
            }),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Null => visitor.visit_unit(),
            other => Err(type_error_owned("unit/null", &other, node.span)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_node_owned(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        if is_empty_null_node(&node) {
            return visitor.visit_seq(OwnedSeqDeserializer {
                items: Vec::<Node>::new().into_iter(),
            });
        }
        match node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(OwnedSeqDeserializer {
                items: items.into_iter(),
            }),
            other => Err(type_error_owned("sequence", &other, node.span)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        if is_empty_null_node(&node) {
            return visitor.visit_map(OwnedMapDeserializer {
                entries: Vec::<(Node, Node)>::new().into_iter(),
                value: None,
            });
        }
        match node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(OwnedMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            other => Err(type_error_owned("mapping", &other, node.span)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(OwnedEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(OwnedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            other => Err(type_error_owned(
                "enum string or single-key mapping",
                &other,
                self.span,
            )),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        untag_node_owned(self).deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Value {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(value),
            Value::Number(number) => visit_any_number(number, None, visitor),
            Value::String(value) => visitor.visit_string(value),
            Value::Sequence(items) => visitor.visit_seq(ValueSeqDeserializer {
                items: items.into_iter(),
            }),
            Value::Mapping(entries) => visitor.visit_map(ValueMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            Value::Tagged(tagged) => visitor.visit_enum(ValueTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::Bool(value) => visitor.visit_bool(value),
            other => Err(type_error_value("bool", &other)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => {
                let value = i64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for i64", None))?;
                visitor.visit_i64(value)
            }
            Value::Number(Number::Unsigned(value)) => match i64::try_from(value) {
                Ok(value) => visitor.visit_i64(value),
                Err(_) => Err(Error::new("expected integer, found unsigned integer", None)),
            },
            other => Err(type_error_value("integer", &other)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) if value >= 0 => {
                let value = u64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            Value::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            other => Err(type_error_value("unsigned integer", &other)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => visitor.visit_i128(value),
            Value::Number(Number::Unsigned(value)) => match i128::try_from(value) {
                Ok(value) => visitor.visit_i128(value),
                Err(_) => Err(Error::new("integer scalar is out of range for i128", None)),
            },
            other => Err(type_error_value("integer", &other)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) if value >= 0 => {
                let value = u128::try_from(value).expect("non-negative i128 fits u128");
                visitor.visit_u128(value)
            }
            Value::Number(Number::Unsigned(value)) => visitor.visit_u128(value),
            other => Err(type_error_value("unsigned integer", &other)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_value(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => visitor.visit_f64(value as f64),
            Value::Number(Number::Unsigned(value)) => visitor.visit_f64(value as f64),
            Value::Number(Number::Float(value)) => visitor.visit_f64(value),
            other => Err(type_error_value("number", &other)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", None)),
                }
            }
            other => Err(type_error_value("char", &other)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::String(value) => visitor.visit_string(value),
            other => Err(type_error_value("string", &other)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::String(value) => visitor.visit_byte_buf(value.into_bytes()),
            other => Err(type_error_value("bytes", &other)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(other),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::Null => visitor.visit_unit(),
            other => Err(type_error_value("unit/null", &other)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_value_owned(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::Null => visitor.visit_seq(ValueSeqDeserializer {
                items: Vec::new().into_iter(),
            }),
            Value::Sequence(items) => visitor.visit_seq(ValueSeqDeserializer {
                items: items.into_iter(),
            }),
            other => Err(type_error_value("sequence", &other)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::Null => visitor.visit_map(ValueMapDeserializer {
                entries: Mapping::new().into_iter(),
                value: None,
            }),
            Value::Mapping(entries) => visitor.visit_map(ValueMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            other => Err(type_error_value("mapping", &other)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            Value::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(ValueEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            Value::Tagged(tagged) => visitor.visit_enum(ValueTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            other => Err(type_error_value(
                "enum string or single-key mapping",
                &other,
            )),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct ValueSeqDeserializer {
    items: std::vec::IntoIter<Value>,
}

impl<'de> SeqAccess<'de> for ValueSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        seed.deserialize(item).map(Some)
    }
}

struct ValueMapDeserializer {
    entries: crate::ast::IntoIter,
    value: Option<Value>,
}

impl<'de> MapAccess<'de> for ValueMapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
    }
}

struct ValueEnumDeserializer {
    key: Value,
    value: Option<Value>,
}

impl<'de> EnumAccess<'de> for ValueEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key.clone())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(Value::Null) => Ok(()),
            Some(value) => Err(type_error_value("unit enum variant", &value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

struct OwnedSeqDeserializer {
    items: std::vec::IntoIter<Node>,
}

impl<'de> SeqAccess<'de> for OwnedSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let span = item.span;
        seed.deserialize(item)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

struct OwnedMapDeserializer {
    entries: std::vec::IntoIter<(Node, Node)>,
    value: Option<Node>,
}

impl<'de> MapAccess<'de> for OwnedMapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        let key_span = key.span;
        self.value = Some(value);
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key_span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        let span = value.span;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

struct OwnedEnumDeserializer {
    key: Node,
    value: Option<Node>,
}

impl<'de> EnumAccess<'de> for OwnedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key.clone())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for OwnedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error_owned(
                "unit enum variant",
                &node.value,
                node.span,
            )),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

impl<'de, 'a, 'src> de::Deserializer<'de> for &'a Document<'src>
where
    'a: 'de,
{
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_any(node, visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_bool(node, visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_i64(node, visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_u64(node, visitor)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_f64(node, visitor)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_char(node, visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_str(node, visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_string(node, visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_bytes(node, visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_byte_buf(node, visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_option(node, visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_unit(node, visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_unit_struct(node, name, visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_newtype_struct(node, name, visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_seq(node, visitor)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_tuple(node, len, visitor)
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
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_tuple_struct(node, name, len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_map(node, visitor)
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
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_struct(node, name, fields, visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_enum(node, name, variants, visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_identifier(node, visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

macro_rules! document_forward {
    ($method:ident ( $($arg:ident : $arg_ty:ty),* ; $visitor:ident )) => {
        fn $method<V>(self, $($arg: $arg_ty,)* $visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let (node, input) = self.into_node_and_input()?;
            match input {
                Some(input) => {
                    let span = node.span;
                    de::Deserializer::$method(
                        InputNode {
                            node: &node,
                            input,
                        },
                        $($arg,)*
                        $visitor,
                    )
                    .map_err(|error| error.with_span_if_missing(span))
                }
                None => de::Deserializer::$method(node, $($arg,)* $visitor),
            }
        }
    };
}

impl<'de> de::Deserializer<'de> for Document<'de> {
    type Error = Error;

    document_forward!(deserialize_any(; visitor));
    document_forward!(deserialize_bool(; visitor));
    document_forward!(deserialize_i64(; visitor));

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    document_forward!(deserialize_u64(; visitor));

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    document_forward!(deserialize_f64(; visitor));

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    document_forward!(deserialize_char(; visitor));
    document_forward!(deserialize_str(; visitor));
    document_forward!(deserialize_string(; visitor));
    document_forward!(deserialize_bytes(; visitor));
    document_forward!(deserialize_byte_buf(; visitor));
    document_forward!(deserialize_option(; visitor));
    document_forward!(deserialize_unit(; visitor));
    document_forward!(deserialize_unit_struct(name: &'static str; visitor));
    document_forward!(deserialize_newtype_struct(name: &'static str; visitor));
    document_forward!(deserialize_seq(; visitor));
    document_forward!(deserialize_tuple(len: usize; visitor));
    document_forward!(deserialize_tuple_struct(name: &'static str, len: usize; visitor));
    document_forward!(deserialize_map(; visitor));
    document_forward!(deserialize_struct(
        name: &'static str,
        fields: &'static [&'static str];
        visitor
    ));
    document_forward!(deserialize_enum(
        name: &'static str,
        variants: &'static [&'static str];
        visitor
    ));
    document_forward!(deserialize_identifier(; visitor));

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_any(self.into_single_document()?, visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_bool(self.into_single_document()?, visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_i64(self.into_single_document()?, visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_u64(self.into_single_document()?, visitor)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_f64(self.into_single_document()?, visitor)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_char(self.into_single_document()?, visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_str(self.into_single_document()?, visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_string(self.into_single_document()?, visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_bytes(self.into_single_document()?, visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_byte_buf(self.into_single_document()?, visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_option(self.into_single_document()?, visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_unit(self.into_single_document()?, visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_unit_struct(self.into_single_document()?, name, visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_newtype_struct(self.into_single_document()?, name, visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.into_single_document()?, visitor)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_tuple(self.into_single_document()?, len, visitor)
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
        de::Deserializer::deserialize_tuple_struct(self.into_single_document()?, name, len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.into_single_document()?, visitor)
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
        de::Deserializer::deserialize_struct(self.into_single_document()?, name, fields, visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_enum(self.into_single_document()?, name, variants, visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_identifier(self.into_single_document()?, visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> IntoDeserializer<'de, Error> for Value {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> de::Deserializer<'de> for Number {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        deserialize_number(self, visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de> de::Deserializer<'de> for &Number {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        deserialize_number(*self, visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

fn deserialize_number<'de, V>(number: Number, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    visit_any_number(number, None, visitor)
}

impl<'de> de::Deserializer<'de> for &'de Value {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(number) => visit_any_number(*number, None, visitor),
            Value::String(value) => visitor.visit_borrowed_str(value),
            Value::Sequence(items) => {
                visitor.visit_seq(ValueRefSeqDeserializer { items, index: 0 })
            }
            Value::Mapping(mapping) => visitor.visit_map(ValueRefMapDeserializer {
                entries: mapping.as_slice(),
                index: 0,
                value: None,
            }),
            Value::Tagged(tagged) => visitor.visit_enum(ValueRefTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::Bool(value) => visitor.visit_bool(*value),
            other => Err(type_error_value("bool", other)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for i64", None))?;
                visitor.visit_i64(value)
            }
            Value::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => visitor.visit_i64(value),
                Err(_) => Err(Error::new("expected integer, found unsigned integer", None)),
            },
            other => Err(type_error_value("integer", other)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            Value::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            other => Err(type_error_value("unsigned integer", other)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => visitor.visit_i128(*value),
            Value::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => visitor.visit_i128(value),
                Err(_) => Err(Error::new("integer scalar is out of range for i128", None)),
            },
            other => Err(type_error_value("integer", other)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                visitor.visit_u128(value)
            }
            Value::Number(Number::Unsigned(value)) => visitor.visit_u128(*value),
            other => Err(type_error_value("unsigned integer", other)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_value(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => visitor.visit_f64(*value as f64),
            Value::Number(Number::Unsigned(value)) => visitor.visit_f64(*value as f64),
            Value::Number(Number::Float(value)) => visitor.visit_f64(*value),
            other => Err(type_error_value("number", other)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", None)),
                }
            }
            other => Err(type_error_value("char", other)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_borrowed_str(value),
            other => Err(type_error_value("string", other)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_string(value.clone()),
            other => Err(type_error_value("string", other)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_borrowed_bytes(value.as_bytes()),
            other => Err(type_error_value("bytes", other)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_byte_buf(value.as_bytes().to_vec()),
            other => Err(type_error_value("bytes", other)),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(other),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_unit(),
            other => Err(type_error_value("unit/null", other)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_value(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_seq(ValueRefSeqDeserializer {
                items: &[],
                index: 0,
            }),
            Value::Sequence(items) => {
                visitor.visit_seq(ValueRefSeqDeserializer { items, index: 0 })
            }
            other => Err(type_error_value("sequence", other)),
        }
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_map(ValueRefMapDeserializer {
                entries: &[],
                index: 0,
                value: None,
            }),
            Value::Mapping(mapping) => visitor.visit_map(ValueRefMapDeserializer {
                entries: mapping.as_slice(),
                index: 0,
                value: None,
            }),
            other => Err(type_error_value("mapping", other)),
        }
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
        let _ = (name, fields);
        self.deserialize_map(visitor)
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = (name, variants);
        match self {
            Value::String(variant) => visitor.visit_enum(variant.as_str().into_deserializer()),
            Value::Mapping(mapping) if mapping.len() == 1 => {
                let entries = mapping.as_slice();
                visitor.visit_enum(ValueRefEnumDeserializer {
                    key: &entries[0].0,
                    value: Some(&entries[0].1),
                })
            }
            Value::Tagged(tagged) => visitor.visit_enum(ValueRefTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
            other => Err(type_error_value("enum string or single-key mapping", other)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct ValueRefSeqDeserializer<'a> {
    items: &'a [Value],
    index: usize,
}

impl<'de> SeqAccess<'de> for ValueRefSeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        seed.deserialize(item).map(Some)
    }
}

struct ValueRefMapDeserializer<'a> {
    entries: &'a [(Value, Value)],
    index: usize,
    value: Option<&'a Value>,
}

impl<'de> MapAccess<'de> for ValueRefMapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        self.value = Some(value);
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
    }
}

struct ValueRefEnumDeserializer<'a> {
    key: &'a Value,
    value: Option<&'a Value>,
}

struct ValueTaggedEnumDeserializer {
    tag: Tag,
    value: Value,
}

impl<'de> EnumAccess<'de> for ValueTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            Value::Null => Ok(()),
            value => Err(type_error_value("unit enum variant", &value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

struct ValueRefTaggedEnumDeserializer<'a> {
    tag: &'a Tag,
    value: &'a Value,
}

impl<'de> EnumAccess<'de> for ValueRefTaggedEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueRefTaggedEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            Value::Null => Ok(()),
            value => Err(type_error_value("unit enum variant", value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de> EnumAccess<'de> for ValueRefEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueRefEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(Value::Null) => Ok(()),
            Some(value) => Err(type_error_value("unit enum variant", value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

struct SeqDeserializer<'a> {
    items: &'a [Node],
    index: usize,
}

struct InputSeqDeserializer<'tree, 'de> {
    items: &'tree [Node],
    input: &'de str,
    index: usize,
}

impl<'de, 'tree> SeqAccess<'de> for InputSeqDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        seed.deserialize(InputNode {
            node: item,
            input: self.input,
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(item.span))
    }
}

impl<'de> SeqAccess<'de> for SeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        seed.deserialize(item)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(item.span))
    }
}

struct MapDeserializer<'a> {
    entries: &'a [(Node, Node)],
    index: usize,
    value: Option<&'a Node>,
}

struct InputMapDeserializer<'tree, 'de> {
    entries: &'tree [(Node, Node)],
    input: &'de str,
    index: usize,
    value: Option<InputNode<'tree, 'de>>,
}

impl<'de, 'tree> MapAccess<'de> for InputMapDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        self.value = Some(InputNode {
            node: value,
            input: self.input,
        });
        seed.deserialize(InputNode {
            node: key,
            input: self.input,
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(value.node.span))
    }
}

impl<'de> MapAccess<'de> for MapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        self.value = Some(value);
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(value.span))
    }
}

struct EnumDeserializer<'a> {
    key: &'a Node,
    value: Option<&'a Node>,
}

struct TaggedEnumDeserializer<'a> {
    tag: &'a Tag,
    value: &'a Node,
}

struct InputEnumDeserializer<'tree, 'de> {
    key: InputNode<'tree, 'de>,
    value: Option<InputNode<'tree, 'de>>,
}

struct InputTaggedEnumDeserializer<'tree, 'de> {
    tag: &'tree Tag,
    value: InputNode<'tree, 'de>,
}

impl<'de, 'tree> EnumAccess<'de> for InputTaggedEnumDeserializer<'tree, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de, 'tree> VariantAccess<'de> for InputTaggedEnumDeserializer<'tree, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.node.value {
            NodeValue::Null => Ok(()),
            _ => Err(type_error("unit enum variant", self.value.node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de, 'tree> EnumAccess<'de> for InputEnumDeserializer<'tree, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de, 'tree> VariantAccess<'de> for InputEnumDeserializer<'tree, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", node.node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

impl<'de> EnumAccess<'de> for TaggedEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for TaggedEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.value {
            NodeValue::Null => Ok(()),
            _ => Err(type_error("unit enum variant", self.value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

struct OwnedTaggedEnumDeserializer {
    tag: Tag,
    value: Node,
}

impl<'de> EnumAccess<'de> for OwnedTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for OwnedTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.value {
            NodeValue::Null => Ok(()),
            other => Err(type_error_owned(
                "unit enum variant",
                &other,
                self.value.span,
            )),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de> EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for EnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

fn untag_node(mut node: &Node) -> &Node {
    while let NodeValue::Tagged(tagged) = &node.value {
        node = &tagged.value;
    }
    node
}

fn untag_node_owned(node: Node) -> Node {
    let Node {
        value,
        span,
        source,
    } = node;
    match value {
        NodeValue::Tagged(tagged) => untag_node_owned(tagged.value),
        value => Node {
            value,
            span,
            source,
        },
    }
}

fn untag_value(mut value: &Value) -> &Value {
    while let Value::Tagged(tagged) = value {
        value = &tagged.value;
    }
    value
}

fn untag_value_owned(value: Value) -> Value {
    match value {
        Value::Tagged(tagged) => untag_value_owned(tagged.value),
        value => value,
    }
}

fn type_error(expected: &'static str, node: &Node) -> Error {
    Error::new(
        format!("expected {expected}, found {}", kind_name(&node.value)),
        Some(node.span),
    )
}

fn type_error_owned(expected: &'static str, value: &NodeValue, span: Span) -> Error {
    Error::new(
        format!("expected {expected}, found {}", kind_name(value)),
        Some(span),
    )
}

fn kind_name(value: &NodeValue) -> &'static str {
    match value {
        NodeValue::Null => "null",
        NodeValue::Bool(_) => "bool",
        NodeValue::Number(Number::Integer(_)) => "integer",
        NodeValue::Number(Number::Unsigned(_)) => "unsigned integer",
        NodeValue::Number(Number::Float(_)) => "float",
        NodeValue::String(_) => "string",
        NodeValue::Sequence(_) => "sequence",
        NodeValue::Mapping(_) => "mapping",
        NodeValue::Tagged(_) => "tagged value",
    }
}

fn type_error_value(expected: &'static str, value: &Value) -> Error {
    Error::new(
        format!("expected {expected}, found {}", kind_name_value(value)),
        None,
    )
}

fn kind_name_value(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(Number::Integer(_)) => "integer",
        Value::Number(Number::Unsigned(_)) => "unsigned integer",
        Value::Number(Number::Float(_)) => "float",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged value",
    }
}
