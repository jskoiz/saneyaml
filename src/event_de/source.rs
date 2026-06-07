use super::*;

pub(super) struct EventDocumentFrames {
    events: crate::parse::EventStream,
    started: bool,
    finished: bool,
    index: usize,
}

impl EventDocumentFrames {
    pub(super) fn from_str_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        Ok(Self {
            events: crate::parse::EventStream::from_str_with_options(input, options)?,
            started: false,
            finished: false,
            index: 0,
        })
    }

    pub(super) fn next_frame(&mut self) -> Option<(usize, Result<Vec<Event>>)> {
        if self.finished {
            return None;
        }
        let index = self.index;
        if let Err(error) = self.enter_stream() {
            self.finished = true;
            return Some((index, Err(error)));
        }

        match self.events.next() {
            Some(Ok(Event::StreamEnd)) => {
                self.finished = true;
                None
            }
            Some(Ok(start @ Event::DocumentStart { .. })) => {
                Some((index, self.collect_document_frame(start)))
            }
            Some(Ok(event)) => {
                self.finished = true;
                Some((
                    index,
                    Err(unexpected_event("document start or stream end", &event)),
                ))
            }
            Some(Err(error)) => {
                self.finished = true;
                Some((index, Err(error)))
            }
            None => {
                self.finished = true;
                None
            }
        }
    }

    pub(super) fn enter_stream(&mut self) -> Result<()> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        match self.events.next() {
            Some(Ok(Event::StreamStart)) => Ok(()),
            Some(Ok(event)) => Err(unexpected_event("stream start", &event)),
            Some(Err(error)) => Err(error),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    fn collect_document_frame(&mut self, start: Event) -> Result<Vec<Event>> {
        let mut frame = Vec::new();
        frame.push(Event::StreamStart);
        frame.push(start);
        loop {
            match self.events.next() {
                Some(Ok(event)) => {
                    let end = matches!(event, Event::DocumentEnd { .. });
                    frame.push(event);
                    if end {
                        frame.push(Event::StreamEnd);
                        self.index += 1;
                        return Ok(frame);
                    }
                }
                Some(Err(error)) => {
                    self.finished = true;
                    return Err(error);
                }
                None => {
                    self.finished = true;
                    return Err(Error::data("unexpected end of YAML event stream", None));
                }
            }
        }
    }
}

pub(super) fn deserialize_document_frame<'de, T>(
    input: &'de str,
    events: Vec<Event>,
    configured_schema: Schema,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
) -> Result<T>
where
    T: serde::Deserialize<'de>,
{
    let mut source = EventSource::new(
        input,
        events,
        configured_schema,
        replay_budget,
        max_nesting_depth,
    );
    source.enter_stream()?;
    source.enter_document()?;
    let value = T::deserialize(EventNodeDeserializer {
        source: &mut source,
    })?;
    source.finish_document()?;
    match source.peek() {
        Some(Event::StreamEnd) => Ok(value),
        Some(event) => Err(unexpected_event("stream end", event)),
        None => Err(Error::data("unexpected end of YAML event stream", None)),
    }
}

pub(super) struct EventSource<'de> {
    pub(super) input: &'de str,
    events: Vec<Event>,
    pos: usize,
    configured_schema: Schema,
    pub(super) schema: Schema,
    pub(super) anchors: HashMap<String, Vec<Event>>,
    inject: Vec<InjectedEvents>,
    replayed_events: usize,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
    pub(super) depth: usize,
}

struct InjectedEvents {
    anchor: String,
    events: Vec<Event>,
    pos: usize,
}

impl<'de> EventSource<'de> {
    pub(super) fn new(
        input: &'de str,
        events: Vec<Event>,
        configured_schema: Schema,
        replay_budget: usize,
        max_nesting_depth: Option<usize>,
    ) -> Self {
        Self {
            input,
            events,
            pos: 0,
            configured_schema,
            schema: configured_schema,
            anchors: HashMap::new(),
            inject: Vec::new(),
            replayed_events: 0,
            replay_budget,
            max_nesting_depth,
            depth: 0,
        }
    }

    /// Records descent into a nested collection and enforces the configured
    /// nesting-depth ceiling. The event-backed path expands aliases lazily as
    /// it walks, so — unlike the tree-backed path's `AnchorTable::resolve` — the
    /// parser's literal-depth check does not bound the *expanded* depth. Without
    /// this guard a literally shallow document with a long alias chain recurses
    /// until the stack overflows. Mirrors the tree-backed `depth > max` check.
    pub(super) fn enter_depth(&mut self, span: Span) -> Result<()> {
        self.depth = self.depth.saturating_add(1);
        if self.max_nesting_depth.is_some_and(|max| self.depth > max) {
            return Err(Error::limit(
                "maximum YAML nesting depth exceeded while expanding alias",
                span,
            ));
        }
        Ok(())
    }

    pub(super) fn exit_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Same ceiling as [`enter_depth`], but for the read-only key/merge
    /// materialization walk in [`node_at_for_key`], which threads an explicit
    /// `depth` because it borrows `self` immutably.
    fn check_depth(&self, depth: usize, span: impl Into<Option<Span>>) -> Result<()> {
        if self.max_nesting_depth.is_some_and(|max| depth > max) {
            return Err(Error::limit(
                "maximum YAML nesting depth exceeded while expanding alias",
                span,
            ));
        }
        Ok(())
    }

    pub(super) fn peek(&self) -> Option<&Event> {
        if let Some(frame) = self.inject.last()
            && frame.pos < frame.events.len()
        {
            return frame.events.get(frame.pos);
        }
        self.events.get(self.pos)
    }

    pub(super) fn next(&mut self) -> Result<Event> {
        loop {
            let event = self.next_raw()?;
            if let Event::Alias { anchor } = event {
                self.inject_alias(anchor.name, anchor.span)?;
                continue;
            }
            return Ok(event);
        }
    }

    fn next_raw(&mut self) -> Result<Event> {
        if let Some(event) = self.next_injected_event() {
            return Ok(event);
        }

        let pos = self.pos;
        let event = self
            .events
            .get(pos)
            .cloned()
            .ok_or_else(|| Error::data("unexpected end of YAML event stream", None))?;
        self.record_anchor_at(pos, &event)?;
        self.pos += 1;
        Ok(event)
    }

    pub(super) fn resolve_aliases_until_non_alias(&mut self) -> Result<()> {
        while matches!(self.peek(), Some(Event::Alias { .. })) {
            let Event::Alias { anchor } = self.next_raw()? else {
                unreachable!("peek observed an alias");
            };
            self.inject_alias(anchor.name, anchor.span)?;
        }
        Ok(())
    }

    fn next_injected_event(&mut self) -> Option<Event> {
        loop {
            let frame = self.inject.last_mut()?;
            if frame.pos < frame.events.len() {
                let event = frame.events[frame.pos].clone();
                frame.pos += 1;
                if frame.pos == frame.events.len() {
                    self.inject.pop();
                }
                return Some(event);
            }
            self.inject.pop();
        }
    }

    fn record_anchor_at(&mut self, pos: usize, event: &Event) -> Result<()> {
        let Some(name) = event_anchor_name(event) else {
            return Ok(());
        };
        let end = skip_node_in(&self.events, pos)?;
        self.anchors
            .insert(name.to_string(), self.events[pos..end].to_vec());
        Ok(())
    }

    fn inject_alias(&mut self, name: String, span: Span) -> Result<()> {
        if self.inject.iter().any(|frame| frame.anchor == name) {
            return Err(Error::reference(
                format!("recursive alias `{name}` is not supported"),
                span,
            ));
        }
        let events = self
            .anchors
            .get(&name)
            .cloned()
            .ok_or_else(|| Error::reference(format!("unknown anchor `{name}`"), span))?;
        self.replayed_events = self.replayed_events.saturating_add(events.len());
        if self.replayed_events > self.replay_budget {
            return Err(Error::limit("alias event replay limit exceeded", span));
        }
        self.inject.push(InjectedEvents {
            anchor: name,
            events,
            pos: 0,
        });
        Ok(())
    }

    pub(super) fn enter_stream(&mut self) -> Result<()> {
        match self.next()? {
            Event::StreamStart => Ok(()),
            event => Err(unexpected_event("stream start", &event)),
        }
    }

    pub(super) fn enter_document(&mut self) -> Result<()> {
        match self.next()? {
            Event::DocumentStart { directives, .. } => {
                self.anchors.clear();
                self.inject.clear();
                self.replayed_events = 0;
                self.depth = 0;
                self.schema = schema_for_directives(self.configured_schema, &directives);
                Ok(())
            }
            event => Err(unexpected_event("document start", &event)),
        }
    }

    pub(super) fn finish_document(&mut self) -> Result<()> {
        match self.next()? {
            Event::DocumentEnd { .. } => Ok(()),
            event => Err(unexpected_event("document end", &event)),
        }
    }

    pub(super) fn scalar_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        meta: &EventMeta,
        span: Span,
    ) -> Result<Node> {
        if let Some(tag) = &meta.tag {
            let tag = &tag.tag;
            let tag_span = meta.tag.as_ref().expect("tag checked").span;
            if tag.is_yaml_core("str") {
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::String(value), span),
                ));
            }
            if tag.is_yaml_core("int") {
                let number = crate::de::parse_explicit_core_int_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Number(number), span).with_scalar_source(value),
                ));
            }
            if tag.is_yaml_core("float") {
                let number = crate::de::parse_explicit_core_float_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Number(number), span).with_scalar_source(value),
                ));
            }
            if tag.is_yaml_core("bool") {
                let value = crate::de::parse_explicit_core_bool_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Bool(value), span),
                ));
            }
            if tag.is_yaml_core("null") {
                crate::de::parse_explicit_core_null_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Null, span),
                ));
            }
            let inner = self.untagged_scalar_from_event(value, style, span)?;
            if tag.is_non_specific() {
                return Ok(non_specific_event_node(span_union(tag_span, span), inner));
            }
            return Ok(Node::new(
                NodeValue::Tagged(Box::new(TaggedNode {
                    tag: tag.clone(),
                    tag_span,
                    value: inner,
                })),
                span_union(tag_span, span),
            ));
        }
        self.untagged_scalar_from_event(value, style, span)
    }

    fn untagged_scalar_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        span: Span,
    ) -> Result<Node> {
        match style {
            ScalarStyle::Plain => parse_scalar_with_schema(&value, span, self.schema),
            ScalarStyle::SingleQuoted
            | ScalarStyle::DoubleQuoted
            | ScalarStyle::Literal
            | ScalarStyle::Folded => Ok(Node::new(NodeValue::String(value), span)),
        }
    }

    pub(super) fn take_scalar(&mut self) -> Result<Node> {
        match self.next()? {
            Event::Scalar {
                value,
                style,
                meta,
                span,
            } => self.scalar_from_event(value, style, &meta, span),
            Event::Alias { anchor } => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            event => Err(unexpected_event("scalar", &event)),
        }
    }

    pub(super) fn skip_node(&mut self) -> Result<()> {
        self.resolve_aliases_until_non_alias()?;
        match self.peek().cloned() {
            Some(Event::Scalar { .. }) => {
                self.next()?;
                Ok(())
            }
            Some(Event::Alias { anchor }) => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Some(Event::SequenceStart { span, .. }) => {
                self.enter_depth(span)?;
                self.next()?;
                loop {
                    if matches!(self.peek(), Some(Event::SequenceEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node()?;
                }
            }
            Some(Event::MappingStart { span, .. }) => {
                if self.next_mapping_has_merge_key()? {
                    let mut node = self.materialize_current_node_for_merge()?;
                    node.apply_merge_keys_with_policy(merge_policy_for_schema(self.schema))?;
                    self.skip_node_raw()?;
                    return Ok(());
                }
                self.validate_next_mapping_duplicates()?;
                self.enter_depth(span)?;
                self.next()?;
                loop {
                    if matches!(self.peek(), Some(Event::MappingEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node()?;
                    self.skip_node()?;
                }
            }
            Some(event) => Err(unexpected_event("node", &event)),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    pub(super) fn skip_node_raw(&mut self) -> Result<()> {
        match self.next()? {
            Event::Scalar { .. } => Ok(()),
            Event::Alias { anchor } => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Event::SequenceStart { span, .. } => {
                self.enter_depth(span)?;
                loop {
                    if matches!(self.peek(), Some(Event::SequenceEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node_raw()?;
                }
            }
            Event::MappingStart { span, .. } => {
                self.enter_depth(span)?;
                loop {
                    if matches!(self.peek(), Some(Event::MappingEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node_raw()?;
                    self.skip_node_raw()?;
                }
            }
            event => Err(unexpected_event("node", &event)),
        }
    }

    pub(super) fn materialize_current_node_for_merge(&self) -> Result<Node> {
        let (events, pos) = self.current_events_and_pos();
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        let (node, next) = self.node_at_for_key(
            events,
            pos,
            &mut scan_anchors,
            &mut Vec::new(),
            &mut replayed_events,
            true,
            self.depth,
        )?;
        let expected = skip_node_in(events, pos)?;
        if next != expected {
            return Err(Error::data(
                "unterminated merge materialization event stream",
                None,
            ));
        }
        Ok(node)
    }

    pub(super) fn next_mapping_has_merge_key(&self) -> Result<bool> {
        let (events, start) = self.current_events_and_pos();
        let Some(Event::MappingStart { .. }) = events.get(start) else {
            return Ok(false);
        };
        let mut pos = start + 1;
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        while let Some(event) = events.get(pos) {
            if matches!(event, Event::MappingEnd { .. }) {
                return Ok(false);
            }
            let (key, next_pos) = self.node_at_for_key(
                events,
                pos,
                &mut scan_anchors,
                &mut Vec::new(),
                &mut replayed_events,
                true,
                self.depth,
            )?;
            if node_is_merge_key(&key) {
                return Ok(true);
            }
            pos = next_pos;
            pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
        }
        Err(Error::data("unterminated mapping event stream", None))
    }

    pub(super) fn validate_next_mapping_duplicates(&self) -> Result<()> {
        let (events, start) = self.current_events_and_pos();
        let Some(Event::MappingStart { .. }) = events.get(start) else {
            return Ok(());
        };
        let mut pos = start + 1;
        let mut seen = DuplicateKeyTracker::new();
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        while let Some(event) = events.get(pos) {
            if matches!(event, Event::MappingEnd { .. }) {
                return Ok(());
            }
            if let Some((key, next_pos)) = self.mapping_key_at(
                events,
                pos,
                &mut scan_anchors,
                &mut replayed_events,
                self.depth,
            )? {
                if node_is_merge_key(&key) {
                    return Err(Error::data(
                        "event-backed merge-key expansion is not implemented",
                        Some(key.span),
                    ));
                }
                check_duplicate_with_tracker_at_depth_limit(&mut seen, &key, 1, None)?;
                pos = next_pos;
            } else {
                pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
            }
            pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
        }
        Err(Error::data("unterminated mapping event stream", None))
    }

    pub(super) fn current_events_and_pos(&self) -> (&[Event], usize) {
        if let Some(frame) = self.inject.last()
            && frame.pos < frame.events.len()
        {
            return (&frame.events, frame.pos);
        }
        (&self.events, self.pos)
    }

    pub(super) fn mapping_key_at(
        &self,
        events: &[Event],
        pos: usize,
        scan_anchors: &mut HashMap<String, Vec<Event>>,
        replayed_events: &mut usize,
        depth: usize,
    ) -> Result<Option<(Node, usize)>> {
        if let Some(name) = events.get(pos).and_then(event_anchor_name) {
            let end = skip_node_in(events, pos)?;
            scan_anchors.insert(name.to_string(), events[pos..end].to_vec());
        }
        match events.get(pos) {
            Some(Event::Scalar { .. })
            | Some(Event::Alias { .. })
            | Some(Event::SequenceStart { .. })
            | Some(Event::MappingStart { .. }) => self
                .node_at_for_key(
                    events,
                    pos,
                    scan_anchors,
                    &mut Vec::new(),
                    replayed_events,
                    false,
                    depth,
                )
                .map(|(node, next)| Some((node, next))),
            Some(_) | None => Ok(None),
        }
    }

    fn scalar_key_at(&self, pos: usize) -> Result<Option<(Node, usize)>> {
        self.scalar_key_at_in(&self.events, pos)
    }

    fn scalar_key_at_in(&self, events: &[Event], pos: usize) -> Result<Option<(Node, usize)>> {
        let Some(Event::Scalar {
            value,
            style,
            meta,
            span,
        }) = events.get(pos)
        else {
            return Ok(None);
        };
        self.scalar_from_event(value.clone(), *style, meta, *span)
            .map(|node| Some((node, pos + 1)))
    }

    fn scalar_key_node_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        meta: &EventMeta,
        span: Span,
    ) -> Result<Node> {
        let Some(tag) = &meta.tag else {
            return self.scalar_from_event(value, style, meta, span);
        };
        let inner = if tag.tag.is_yaml_core("int") {
            Node::new(
                NodeValue::Number(crate::de::parse_explicit_core_int_text(&value, Some(span))?),
                span,
            )
        } else if tag.tag.is_yaml_core("float") {
            Node::new(
                NodeValue::Number(crate::de::parse_explicit_core_float_text(
                    &value,
                    Some(span),
                )?),
                span,
            )
        } else if tag.tag.is_yaml_core("bool") {
            Node::new(
                NodeValue::Bool(crate::de::parse_explicit_core_bool_text(
                    &value,
                    Some(span),
                )?),
                span,
            )
        } else if tag.tag.is_yaml_core("null") {
            crate::de::parse_explicit_core_null_text(&value, Some(span))?;
            Node::new(NodeValue::Null, span)
        } else {
            let _ = style;
            Node::new(NodeValue::String(value), span)
        };
        Ok(tagged_key_node(tag.tag.clone(), tag.span, inner))
    }

    #[allow(clippy::too_many_arguments)]
    fn node_at_for_key(
        &self,
        events: &[Event],
        pos: usize,
        scan_anchors: &mut HashMap<String, Vec<Event>>,
        active_aliases: &mut Vec<String>,
        replayed_events: &mut usize,
        allow_merge_key: bool,
        depth: usize,
    ) -> Result<(Node, usize)> {
        let Some(event) = events.get(pos) else {
            return Err(Error::data("unexpected end of YAML event stream", None));
        };
        self.check_depth(depth, event_span(event))?;
        if let Some(name) = event_anchor_name(event) {
            let end = skip_node_in(events, pos)?;
            scan_anchors.insert(name.to_string(), events[pos..end].to_vec());
        }

        match event {
            Event::Scalar {
                value,
                style,
                meta,
                span,
            } => self
                .scalar_key_node_from_event(value.clone(), *style, meta, *span)
                .map(|node| (node, pos + 1)),
            Event::Alias { anchor } => {
                let name = &anchor.name;
                if active_aliases.iter().any(|active| active == name) {
                    return Err(Error::reference(
                        format!("recursive alias `{name}` is not supported"),
                        anchor.span,
                    ));
                }
                let target = scan_anchors.get(name).cloned().ok_or_else(|| {
                    Error::reference(format!("unknown anchor `{name}`"), anchor.span)
                })?;
                *replayed_events = replayed_events.saturating_add(target.len());
                if *replayed_events > self.replay_budget {
                    return Err(Error::limit(
                        "alias event replay limit exceeded",
                        anchor.span,
                    ));
                }
                active_aliases.push(name.clone());
                let (mut node, end) = self.node_at_for_key(
                    &target,
                    0,
                    scan_anchors,
                    active_aliases,
                    replayed_events,
                    allow_merge_key,
                    depth,
                )?;
                active_aliases.pop();
                if end != target.len() {
                    return Err(Error::data("unterminated alias key event subtree", None));
                }
                node.span = anchor.span;
                Ok((node, pos + 1))
            }
            Event::SequenceStart { meta, span, .. } => {
                let mut items = Vec::new();
                let mut next = pos + 1;
                loop {
                    match events.get(next) {
                        Some(Event::SequenceEnd { span: end_span }) => {
                            let node =
                                Node::new(NodeValue::Sequence(items), span_union(*span, *end_span));
                            return Ok((apply_event_tag(meta, node), next + 1));
                        }
                        Some(_) => {
                            let (item, after_item) = self.node_at_for_key(
                                events,
                                next,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            items.push(item);
                            next = after_item;
                        }
                        None => {
                            return Err(Error::data("unterminated sequence event stream", None));
                        }
                    }
                }
            }
            Event::MappingStart { meta, span, .. } => {
                let mut entries = Vec::new();
                let mut seen = DuplicateKeyTracker::new();
                let mut next = pos + 1;
                loop {
                    match events.get(next) {
                        Some(Event::MappingEnd { span: end_span }) => {
                            let node = Node::new(
                                NodeValue::Mapping(entries),
                                span_union(*span, *end_span),
                            );
                            return Ok((apply_event_tag(meta, node), next + 1));
                        }
                        Some(_) => {
                            let (key, after_key) = self.node_at_for_key(
                                events,
                                next,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            if !allow_merge_key && node_is_merge_key(&key) {
                                return Err(Error::data(
                                    "event-backed merge-key expansion is not implemented",
                                    Some(key.span),
                                ));
                            }
                            if !(allow_merge_key
                                && self.schema.is_legacy_compatible()
                                && node_is_merge_key(&key))
                            {
                                check_duplicate_with_tracker_at_depth_limit(
                                    &mut seen, &key, 1, None,
                                )?;
                            }
                            let (value, after_value) = self.node_at_for_key(
                                events,
                                after_key,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            entries.push((key, value));
                            next = after_value;
                        }
                        None => return Err(Error::data("unterminated mapping event stream", None)),
                    }
                }
            }
            event => Err(unexpected_event("node", event)),
        }
    }
}

pub(super) fn skip_node_in(events: &[Event], pos: usize) -> Result<usize> {
    match events
        .get(pos)
        .ok_or_else(|| Error::data("unexpected end of YAML event stream", None))?
    {
        Event::Scalar { .. } | Event::Alias { .. } => Ok(pos + 1),
        Event::SequenceStart { .. } => {
            let mut next = pos + 1;
            loop {
                match events.get(next) {
                    Some(Event::SequenceEnd { .. }) => return Ok(next + 1),
                    Some(_) => next = skip_node_in(events, next)?,
                    None => return Err(Error::data("unterminated sequence event stream", None)),
                }
            }
        }
        Event::MappingStart { .. } => {
            let mut next = pos + 1;
            loop {
                match events.get(next) {
                    Some(Event::MappingEnd { .. }) => return Ok(next + 1),
                    Some(_) => {
                        next = skip_node_in(events, next)?;
                        next = skip_node_in(events, next)?;
                    }
                    None => return Err(Error::data("unterminated mapping event stream", None)),
                }
            }
        }
        event => Err(unexpected_event("node", event)),
    }
}
