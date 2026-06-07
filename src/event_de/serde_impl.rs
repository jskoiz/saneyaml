use super::prepared::*;
use super::source::EventSource;
use super::*;

pub(super) struct EventNodeDeserializer<'a, 'de> {
    pub(super) source: &'a mut EventSource<'de>,
}

impl<'de> EventNodeDeserializer<'_, 'de> {
    fn deserialize_prepared_current_node<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_any(PreparedNodeDeserializer { node }, visitor)
    }

    fn deserialize_prepared_current_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node }, visitor)
    }

    fn deserialize_prepared_current_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor)
    }
}

impl<'de> de::Deserializer<'de> for EventNodeDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.resolve_aliases_until_non_alias()?;
        match self.source.peek() {
            Some(Event::Scalar { .. }) => {
                let node = self.source.take_scalar()?;
                visit_scalar_any(&node, self.source.input, visitor)
            }
            Some(Event::SequenceStart { meta, .. }) | Some(Event::MappingStart { meta, .. })
                if meta.tag.is_some() =>
            {
                self.deserialize_prepared_current_node(visitor)
            }
            Some(Event::SequenceStart { .. }) => self.deserialize_seq(visitor),
            Some(Event::MappingStart { .. }) => self.deserialize_map(visitor),
            Some(Event::Alias { anchor }) => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Some(event) => Err(unexpected_event("node", event)),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
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
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
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
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_u64_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_i128_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
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
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_f64_number(number, node.span, visitor),
            _ => Err(type_error("number", &node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
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
        let node = self.source.take_scalar()?;
        let value = string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        if let Some(borrowed) = borrowed_event_str(self.source.input, node.span, value) {
            return visitor.visit_borrowed_str(borrowed);
        }
        visitor.visit_str(value)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
        let value = string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        visitor.visit_string(value.to_string())
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
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
        self.source.resolve_aliases_until_non_alias()?;
        if self.source.peek_is_null_scalar()? {
            self.source.take_scalar()?;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
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
        self.source.resolve_aliases_until_non_alias()?;
        if self
            .source
            .peek_has_yaml_core_tag(&["set", "omap", "pairs"])
        {
            return self.deserialize_prepared_current_seq(visitor);
        }
        match self.source.next()? {
            Event::SequenceStart { span, .. } => {
                self.source.enter_depth(span)?;
                let value = visitor.visit_seq(EventSeqAccess {
                    source: &mut *self.source,
                    index: 0,
                });
                self.source.exit_depth();
                value
            }
            event => Err(unexpected_event("sequence", &event)),
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
        self.source.resolve_aliases_until_non_alias()?;
        if self.source.peek_has_yaml_core_tag(&["omap"]) {
            return self.deserialize_prepared_current_map(visitor);
        }
        if self.source.next_mapping_has_merge_key()? {
            let mut node = self.source.materialize_current_node_for_merge()?;
            node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
            self.source.skip_node_raw()?;
            return de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor);
        }
        self.source.validate_next_mapping_duplicates()?;
        match self.source.next()? {
            Event::MappingStart { span, .. } => {
                self.source.enter_depth(span)?;
                let value = visitor.visit_map(EventMapAccess {
                    source: &mut *self.source,
                    value: None,
                });
                self.source.exit_depth();
                value
            }
            event => Err(unexpected_event("mapping", &event)),
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
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Materialize the current node and reuse the tree-backed enum logic so
        // the event path accepts the same forms as `de.rs`: bare-scalar unit
        // variants, single-key `{Variant: payload}` mappings (newtype/tuple/
        // struct variants), and tag-shorthand variants. The previous
        // scalar-only path rejected every externally-tagged variant that
        // carried a payload.
        self.source.resolve_aliases_until_non_alias()?;
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_enum(
            PreparedNodeDeserializer { node },
            name,
            variants,
            visitor,
        )
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.skip_node()?;
        visitor.visit_unit()
    }
}

impl EventSource<'_> {
    fn peek_has_yaml_core_tag(&self, suffixes: &[&str]) -> bool {
        match self.peek() {
            Some(Event::SequenceStart { meta, .. }) | Some(Event::MappingStart { meta, .. }) => {
                meta.tag
                    .as_ref()
                    .is_some_and(|tag| suffixes.iter().any(|suffix| tag.tag.is_yaml_core(suffix)))
            }
            _ => false,
        }
    }

    fn peek_is_null_scalar(&self) -> Result<bool> {
        let Some(Event::Scalar {
            value,
            style,
            meta,
            span,
        }) = self.peek()
        else {
            return Ok(false);
        };
        let node = self.scalar_from_event(value.clone(), *style, meta, *span)?;
        Ok(prepared_is_null_node(&node))
    }
}

struct EventSeqAccess<'a, 'de> {
    source: &'a mut EventSource<'de>,
    index: usize,
}

impl<'de> SeqAccess<'de> for EventSeqAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        if matches!(self.source.peek(), Some(Event::SequenceEnd { .. })) {
            self.source.next()?;
            return Ok(None);
        }
        let index = self.index;
        self.index += 1;
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map(Some)
        .map_err(|error| error.prepend_path_segment(ErrorPathSegment::Index(index)))
    }
}

struct EventMapAccess<'a, 'de> {
    source: &'a mut EventSource<'de>,
    value: Option<ErrorPathSegment>,
}

impl<'de> MapAccess<'de> for EventMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        if matches!(self.source.peek(), Some(Event::MappingEnd { .. })) {
            self.source.next()?;
            return Ok(None);
        }
        let depth = self.source.depth;
        let (events, pos) = self.source.current_events_and_pos();
        let mut scan_anchors = self.source.anchors.clone();
        let mut replayed_events = 0usize;
        let segment = self
            .source
            .mapping_key_at(events, pos, &mut scan_anchors, &mut replayed_events, depth)?
            .map(|(node, _)| path_segment_for_node(&node))
            .unwrap_or(ErrorPathSegment::ComplexKey);
        self.value = Some(segment.clone());
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map(Some)
        .map_err(|error| error.with_path_segment_if_empty(segment))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        let segment = self
            .value
            .take()
            .ok_or_else(|| Error::data("value requested before key", None))?;
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map_err(|error| error.prepend_path_segment(segment))
    }
}
