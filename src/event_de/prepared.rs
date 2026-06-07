use super::*;

pub(super) struct PreparedNodeDeserializer {
    pub(super) node: Node,
}

struct PreparedSeqAccess {
    items: std::vec::IntoIter<Node>,
    index: usize,
}

impl<'de> SeqAccess<'de> for PreparedSeqAccess {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        let Some(node) = self.items.next() else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        seed.deserialize(PreparedNodeDeserializer { node })
            .map(Some)
            .map_err(|error| error.prepend_path_segment(ErrorPathSegment::Index(index)))
    }
}

struct PreparedMapAccess {
    entries: std::vec::IntoIter<(Node, Node)>,
    value: Option<(Node, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for PreparedMapAccess {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        let segment = path_segment_for_node(&key);
        self.value = Some((value, segment.clone()));
        seed.deserialize(PreparedNodeDeserializer { node: key })
            .map(Some)
            .map_err(|error| error.with_path_segment_if_empty(segment))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        let (node, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::data("value requested before key", None))?;
        seed.deserialize(PreparedNodeDeserializer { node })
            .map_err(|error| error.prepend_path_segment(segment))
    }
}

impl<'de> de::Deserializer<'de> for PreparedNodeDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let span = self.node.span;
        match self.node.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(value),
            NodeValue::Number(number) => visit_any_number(number, span, visitor),
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Sequence(items) => visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Bool(value) => with_span(visitor.visit_bool(value), node.span),
            _ => Err(type_error("bool", &node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_i64_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_u64_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_i128_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_u128_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_f64_number(number, node.span, visitor),
            _ => Err(type_error("number", &node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        let value = prepared_string_target_text(&node).ok_or_else(|| type_error("char", &node))?;
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) => with_span(visitor.visit_char(ch), node.span),
            _ => Err(type_error("char", &node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        let value =
            prepared_string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        visitor.visit_string(value.to_string())
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        Err(type_error("bytes", &node))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if prepared_is_null_node(&self.node) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", &node)),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if yaml11_set_entries_node(&self.node)?.is_some() {
            let entries = take_yaml11_set_entries_node(self.node).expect("checked explicit !!set");
            let items = yaml11_set_key_nodes(entries)?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        if yaml11_pair_items_node(&self.node, "omap")?.is_some() {
            let items =
                take_yaml11_pair_items_node(self.node, "omap").expect("checked explicit !!omap");
            let items = yaml11_pair_sequence_nodes(items, "omap")?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        if yaml11_pair_items_node(&self.node, "pairs")?.is_some() {
            let items =
                take_yaml11_pair_items_node(self.node, "pairs").expect("checked explicit !!pairs");
            let items = yaml11_pair_sequence_nodes(items, "pairs")?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            }),
            _ => Err(type_error("sequence", &node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
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
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if let Some(items) = yaml11_pair_items_node(&self.node, "omap")? {
            validate_yaml11_omap_node_keys(items)?;
            let items =
                take_yaml11_pair_items_node(self.node, "omap").expect("checked explicit !!omap");
            let entries = yaml11_pair_entries(items, "omap")?;
            return visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            });
        }
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            }),
            _ => Err(type_error("mapping", &node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
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
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.node.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(PreparedEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            _ => Err(type_error("enum string or single-key mapping", &self.node)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

pub(super) struct PreparedEnumDeserializer {
    key: Node,
    value: Option<Node>,
}

impl<'de> EnumAccess<'de> for PreparedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(PreparedNodeDeserializer {
            node: self.key.clone(),
        })?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for PreparedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", &node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("newtype variant requires a value", None))?;
        seed.deserialize(PreparedNodeDeserializer { node })
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node }, visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor)
    }
}

pub(super) struct PreparedTaggedEnumDeserializer {
    tag: Tag,
    value: Node,
}

impl<'de> EnumAccess<'de> for PreparedTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for PreparedTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        if prepared_is_null_node(&self.value) {
            Ok(())
        } else {
            Err(type_error("unit enum variant", &self.value))
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(PreparedNodeDeserializer { node: self.value })
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node: self.value }, visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node: self.value }, visitor)
    }
}

pub(super) fn explicit_core_tagged_node<'a>(mut node: &'a Node, suffix: &str) -> Option<&'a Node> {
    while let NodeValue::Tagged(tagged) = &node.value {
        if tagged.tag.is_yaml_core(suffix) {
            return Some(&tagged.value);
        }
        node = &tagged.value;
    }
    None
}

pub(super) fn take_explicit_core_tagged_node(mut node: Node, suffix: &str) -> Option<Node> {
    loop {
        match node.value {
            NodeValue::Tagged(tagged) if tagged.tag.is_yaml_core(suffix) => {
                return Some(tagged.value);
            }
            NodeValue::Tagged(tagged) => node = tagged.value,
            _ => return None,
        }
    }
}

pub(super) fn yaml11_set_entries_node(node: &Node) -> Result<Option<&[(Node, Node)]>> {
    let Some(value) = explicit_core_tagged_node(node, "set") else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Mapping(entries) => Ok(Some(entries)),
        _ => Err(type_error("mapping for explicit !!set", value)),
    }
}

pub(super) fn take_yaml11_set_entries_node(node: Node) -> Option<Vec<(Node, Node)>> {
    let value = take_explicit_core_tagged_node(node, "set")?;
    match value.value {
        NodeValue::Mapping(entries) => Some(entries),
        _ => None,
    }
}

pub(super) fn yaml11_pair_items_node<'a>(
    node: &'a Node,
    suffix: &'static str,
) -> Result<Option<&'a [Node]>> {
    let Some(value) = explicit_core_tagged_node(node, suffix) else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Sequence(items) => Ok(Some(items)),
        _ => Err(Error::data(
            format!("expected sequence for explicit !!{suffix}"),
            Some(value.span),
        )),
    }
}

pub(super) fn take_yaml11_pair_items_node(node: Node, suffix: &'static str) -> Option<Vec<Node>> {
    let value = take_explicit_core_tagged_node(node, suffix)?;
    match value.value {
        NodeValue::Sequence(items) => Some(items),
        _ => None,
    }
}

pub(super) fn validate_yaml11_omap_node_keys(items: &[Node]) -> Result<()> {
    let mut seen = DuplicateKeyTracker::new();
    for item in items {
        let (key, _) = yaml11_singleton_pair_node(item, "omap")?;
        check_duplicate_with_tracker_at_depth_limit(
            &mut seen,
            key,
            1,
            Some(crate::schema::DEFAULT_MAX_NESTING_DEPTH),
        )?;
    }
    Ok(())
}

pub(super) fn yaml11_set_key_nodes(entries: Vec<(Node, Node)>) -> Result<Vec<Node>> {
    entries
        .into_iter()
        .map(|(key, value)| {
            ensure_yaml11_set_null_node(&value)?;
            Ok(key)
        })
        .collect()
}

pub(super) fn ensure_yaml11_set_null_node(value: &Node) -> Result<()> {
    if prepared_is_null_node(value) {
        Ok(())
    } else {
        Err(Error::data(
            "expected explicit !!set entry value to be null",
            Some(value.span),
        ))
    }
}

pub(super) fn yaml11_pair_sequence_nodes(
    items: Vec<Node>,
    suffix: &'static str,
) -> Result<Vec<Node>> {
    items
        .into_iter()
        .map(|item| {
            let span = item.span;
            let (key, value) = take_yaml11_singleton_pair_node(item, suffix)?;
            Ok(Node::new(NodeValue::Sequence(vec![key, value]), span))
        })
        .collect()
}

pub(super) fn yaml11_pair_entries(
    items: Vec<Node>,
    suffix: &'static str,
) -> Result<Vec<(Node, Node)>> {
    items
        .into_iter()
        .map(|item| take_yaml11_singleton_pair_node(item, suffix))
        .collect()
}

pub(super) fn yaml11_singleton_pair_node<'a>(
    node: &'a Node,
    suffix: &'static str,
) -> Result<(&'a Node, &'a Node)> {
    let node = prepared_untag_node(node);
    match &node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => Ok((&entries[0].0, &entries[0].1)),
        NodeValue::Mapping(_) => Err(Error::data(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::data(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

pub(super) fn take_yaml11_singleton_pair_node(
    node: Node,
    suffix: &'static str,
) -> Result<(Node, Node)> {
    let node = prepared_untag_node_owned(node);
    match node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => {
            let mut entries = entries.into_iter();
            entries.next().ok_or_else(|| {
                Error::data(
                    "internal: singleton mapping lost its entry",
                    Some(node.span),
                )
            })
        }
        NodeValue::Mapping(_) => Err(Error::data(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::data(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

pub(super) fn prepared_untag_node(mut node: &Node) -> &Node {
    while let NodeValue::Tagged(tagged) = &node.value {
        node = &tagged.value;
    }
    node
}

pub(super) fn prepared_untag_node_owned(node: Node) -> Node {
    let Node {
        value,
        span,
        source,
    } = node;
    match value {
        NodeValue::Tagged(tagged) => prepared_untag_node_owned(tagged.value),
        value => Node {
            value,
            span,
            source,
        },
    }
}

pub(super) fn prepared_is_null_node(node: &Node) -> bool {
    match &node.value {
        NodeValue::Null => true,
        NodeValue::Tagged(tagged) => prepared_is_null_node(&tagged.value),
        _ => false,
    }
}

pub(super) fn prepared_string_target_text(node: &Node) -> Option<&str> {
    match &node.value {
        NodeValue::Tagged(tagged) => prepared_string_target_text(&tagged.value),
        _ => string_target_text(node),
    }
}

pub(super) fn visit_scalar_any<'de, V>(node: &Node, input: &'de str, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match &node.value {
        NodeValue::Null => visitor.visit_unit(),
        NodeValue::Bool(value) => visitor.visit_bool(*value),
        NodeValue::Number(number) => visit_any_number(*number, node.span, visitor),
        NodeValue::String(value) => {
            if let Some(borrowed) = borrowed_event_str(input, node.span, value) {
                visitor.visit_borrowed_str(borrowed)
            } else {
                visitor.visit_str(value)
            }
        }
        NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
            tag: tagged.tag.clone(),
            value: tagged.value.clone(),
        }),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) => Err(type_error("scalar", node)),
    }
}

pub(super) fn string_target_text(node: &Node) -> Option<&str> {
    match &node.value {
        NodeValue::String(value) => Some(value),
        NodeValue::Null => Some("null"),
        NodeValue::Bool(value) => Some(if *value { "true" } else { "false" }),
        NodeValue::Number(_) => node.scalar_source().map(|source| source.raw()),
        NodeValue::Tagged(tagged) => string_target_text(&tagged.value),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) => None,
    }
}

pub(super) fn borrowed_event_str<'de>(
    input: &'de str,
    span: Span,
    value: &str,
) -> Option<&'de str> {
    let raw = input.get(span.start..span.end)?;
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

pub(super) fn path_segment_for_node(node: &Node) -> ErrorPathSegment {
    match &node.value {
        NodeValue::String(value) => ErrorPathSegment::Key(value.clone()),
        NodeValue::Bool(value) => ErrorPathSegment::ScalarKey(value.to_string()),
        NodeValue::Number(number) => ErrorPathSegment::ScalarKey(number.to_string()),
        NodeValue::Null => ErrorPathSegment::ScalarKey("null".to_string()),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) | NodeValue::Tagged(_) => {
            ErrorPathSegment::ComplexKey
        }
    }
}

pub(super) fn with_span<T>(result: Result<T>, span: Span) -> Result<T> {
    result.map_err(|error| error.with_span_if_missing(span))
}

pub(super) fn visit_i64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i64",
                Some(span),
            )),
        },
        Number::Unsigned(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i64",
                Some(span),
            )),
        },
        Number::Float(_) => Err(Error::data("expected integer, found float", Some(span))),
    }
}

pub(super) fn visit_u64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for u64",
                Some(span),
            )),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for u64",
                Some(span),
            )),
        },
        Number::Integer(_) => Err(Error::data(
            "expected unsigned integer, found integer",
            Some(span),
        )),
        Number::Float(_) => Err(Error::data(
            "expected unsigned integer, found float",
            Some(span),
        )),
    }
}

pub(super) fn visit_i128_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_span(visitor.visit_i128(value), span),
        Number::Unsigned(value) => match i128::try_from(value) {
            Ok(value) => with_span(visitor.visit_i128(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i128",
                Some(span),
            )),
        },
        Number::Float(_) => Err(Error::data("expected integer, found float", Some(span))),
    }
}

pub(super) fn visit_u128_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => {
            let value = u128::try_from(value).expect("non-negative i128 fits u128");
            with_span(visitor.visit_u128(value), span)
        }
        Number::Unsigned(value) => with_span(visitor.visit_u128(value), span),
        Number::Integer(_) => Err(Error::data(
            "expected unsigned integer, found integer",
            Some(span),
        )),
        Number::Float(_) => Err(Error::data(
            "expected unsigned integer, found float",
            Some(span),
        )),
    }
}

pub(super) fn visit_f64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_span(visitor.visit_f64(value as f64), span),
        Number::Unsigned(value) => with_span(visitor.visit_f64(value as f64), span),
        Number::Float(value) => with_span(visitor.visit_f64(value), span),
    }
}

pub(super) fn visit_any_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => with_span(visitor.visit_i128(value), span),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => with_span(visitor.visit_u128(value), span),
        },
        Number::Float(value) => with_span(visitor.visit_f64(value), span),
    }
}

pub(super) fn type_error(expected: &'static str, node: &Node) -> Error {
    Error::data(
        format!("expected {expected}, found {}", kind_name(&node.value)),
        Some(node.span),
    )
}

pub(super) fn kind_name(value: &NodeValue) -> &'static str {
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

pub(super) fn unexpected_event(expected: &'static str, event: &Event) -> Error {
    Error::data(
        format!("expected {expected}, found {}", event_kind(event)),
        event_span(event),
    )
}

pub(super) fn event_kind(event: &Event) -> &'static str {
    match event {
        Event::StreamStart => "stream start",
        Event::StreamEnd => "stream end",
        Event::DocumentStart { .. } => "document start",
        Event::DocumentEnd { .. } => "document end",
        Event::SequenceStart { .. } => "sequence start",
        Event::SequenceEnd { .. } => "sequence end",
        Event::MappingStart { .. } => "mapping start",
        Event::MappingEnd { .. } => "mapping end",
        Event::Alias { .. } => "alias",
        Event::Scalar { .. } => "scalar",
    }
}

pub(super) fn event_span(event: &Event) -> Option<Span> {
    match event {
        Event::DocumentStart { span, .. }
        | Event::DocumentEnd { span, .. }
        | Event::SequenceStart { span, .. }
        | Event::SequenceEnd { span }
        | Event::MappingStart { span, .. }
        | Event::MappingEnd { span }
        | Event::Scalar { span, .. } => Some(*span),
        Event::Alias { anchor } => Some(anchor.span),
        Event::StreamStart | Event::StreamEnd => None,
    }
}
