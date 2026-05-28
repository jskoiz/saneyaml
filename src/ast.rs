use crate::{Error, Span, key_identity::same_key_identity, yaml11};
use serde::de::{self, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::{Index as StdIndex, IndexMut as StdIndexMut};
use std::str::FromStr;

/// Spanful YAML document tree node produced by the parser.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Parsed node payload.
    pub value: NodeValue,
    /// Source span for this node.
    pub span: Span,
    pub(crate) source: Option<ScalarSource>,
}

impl Node {
    /// Creates a node from a payload and span.
    pub fn new(value: NodeValue, span: Span) -> Self {
        Self {
            value,
            span,
            source: None,
        }
    }

    /// Creates a null node at the given span.
    pub fn null(span: Span) -> Self {
        Self::new(NodeValue::Null, span)
    }

    pub(crate) fn empty_scalar(span: Span) -> Self {
        Self::null(span).with_scalar_source("")
    }

    pub(crate) fn with_scalar_source(mut self, raw: impl Into<String>) -> Self {
        self.source = Some(ScalarSource { raw: raw.into() });
        self
    }

    /// Returns the original scalar spelling when it was retained.
    pub fn scalar_source(&self) -> Option<&ScalarSource> {
        self.source.as_ref()
    }

    /// Returns the scalar string value.
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            NodeValue::String(value) => Some(value),
            NodeValue::Tagged(tagged) => tagged.value.as_str(),
            _ => None,
        }
    }

    /// Returns this node as a YAML 1.1 timestamp, if it carries `!!timestamp`.
    pub fn as_timestamp(&self) -> Option<Timestamp> {
        self.value.as_timestamp()
    }

    /// Compares two nodes by semantic value, ignoring source spans.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.value.equivalent(&other.value)
    }

    /// Converts this spanful node into a spanless [`Value`].
    pub fn into_value(self) -> Value {
        self.value.into()
    }

    pub(crate) fn apply_merge_keys(&mut self) -> crate::Result<()> {
        apply_merge_keys_in_node(self)
    }
}

/// Original scalar spelling retained for string-target deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarSource {
    raw: String,
}

impl ScalarSource {
    /// Returns the raw scalar text as it appeared in the YAML source.
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// YAML 1.1 timestamp value parsed from `!!timestamp` scalars.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timestamp {
    date: Date,
    time: Option<Time>,
}

impl Timestamp {
    /// Creates a timestamp from a date and optional time.
    pub const fn new(date: Date, time: Option<Time>) -> Self {
        Self { date, time }
    }

    /// Parses a YAML 1.1 timestamp scalar.
    pub fn parse_yaml_1_1(text: &str) -> Option<Self> {
        parse_yaml11_timestamp(text)
    }

    /// Returns the date component.
    pub const fn date(&self) -> Date {
        self.date
    }

    /// Returns the optional time component.
    pub const fn time(&self) -> Option<Time> {
        self.time
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.date)?;
        if let Some(time) = self.time {
            write!(formatter, "T{time}")?;
        }
        Ok(())
    }
}

impl FromStr for Timestamp {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_yaml_1_1(text).ok_or(())
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TimestampVisitor;

        impl<'de> Visitor<'de> for TimestampVisitor {
            type Value = Timestamp;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a YAML 1.1 timestamp scalar")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Timestamp::parse_yaml_1_1(value)
                    .ok_or_else(|| E::custom("invalid YAML 1.1 timestamp scalar"))
            }

            fn visit_borrowed_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(value)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_str(TimestampVisitor)
    }
}

/// Date component of a YAML 1.1 [`Timestamp`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// Creates a date if the year, month, and day form a valid Gregorian date.
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Option<Self> {
        (month != 0 && month <= 12 && day != 0 && day <= days_in_month(year, month))
            .then_some(Self { year, month, day })
    }

    /// Returns the four-digit year.
    pub const fn year(&self) -> u16 {
        self.year
    }

    /// Returns the one-based month.
    pub const fn month(&self) -> u8 {
        self.month
    }

    /// Returns the one-based day of month.
    pub const fn day(&self) -> u8 {
        self.day
    }
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

/// Time component of a YAML 1.1 [`Timestamp`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Time {
    hour: u8,
    minute: u8,
    second: u8,
    nanosecond: u32,
    offset: Option<TimeZoneOffset>,
}

impl Time {
    /// Creates a time value if all components are valid.
    pub fn from_hms_nano_offset(
        hour: u8,
        minute: u8,
        second: u8,
        nanosecond: u32,
        offset: Option<TimeZoneOffset>,
    ) -> Option<Self> {
        (hour <= 23 && minute <= 59 && second <= 60 && nanosecond < 1_000_000_000).then_some(Self {
            hour,
            minute,
            second,
            nanosecond,
            offset,
        })
    }

    /// Returns the zero-based hour in the day.
    pub const fn hour(&self) -> u8 {
        self.hour
    }

    /// Returns the minute.
    pub const fn minute(&self) -> u8 {
        self.minute
    }

    /// Returns the second, allowing `60` for YAML 1.1 leap-second spellings.
    pub const fn second(&self) -> u8 {
        self.second
    }

    /// Returns the fractional second in nanoseconds.
    pub const fn nanosecond(&self) -> u32 {
        self.nanosecond
    }

    /// Returns the UTC offset when the source timestamp included one.
    pub const fn offset(&self) -> Option<TimeZoneOffset> {
        self.offset
    }
}

impl fmt::Display for Time {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:02}:{:02}:{:02}",
            self.hour, self.minute, self.second
        )?;
        if self.nanosecond != 0 {
            let mut fraction = format!("{:09}", self.nanosecond);
            while fraction.ends_with('0') {
                fraction.pop();
            }
            write!(formatter, ".{fraction}")?;
        }
        if let Some(offset) = self.offset {
            write!(formatter, "{offset}")?;
        }
        Ok(())
    }
}

/// UTC offset component of a YAML 1.1 [`Timestamp`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimeZoneOffset {
    minutes: i16,
}

impl TimeZoneOffset {
    /// Creates a UTC offset from signed minutes.
    pub fn from_minutes(minutes: i16) -> Option<Self> {
        let max = 23 * 60 + 59;
        (minutes >= -max && minutes <= max).then_some(Self { minutes })
    }

    /// Returns the signed UTC offset in minutes.
    pub const fn minutes(&self) -> i16 {
        self.minutes
    }
}

impl fmt::Display for TimeZoneOffset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minutes == 0 {
            return formatter.write_str("Z");
        }
        let sign = if self.minutes < 0 { '-' } else { '+' };
        let absolute = self.minutes.unsigned_abs();
        write!(formatter, "{sign}{:02}:{:02}", absolute / 60, absolute % 60)
    }
}

fn parse_yaml11_timestamp(text: &str) -> Option<Timestamp> {
    let bytes = text.as_bytes();
    let (date, mut pos) = parse_yaml11_date(bytes, 0)?;
    if pos == bytes.len() {
        return Some(Timestamp::new(date, None));
    }

    match bytes.get(pos) {
        Some(b'T' | b't') => pos += 1,
        Some(byte) if yaml11_timestamp_space(*byte) => {
            while pos < bytes.len() && yaml11_timestamp_space(bytes[pos]) {
                pos += 1;
            }
        }
        _ => return None,
    }

    let (hour, minute, second, mut pos) = parse_yaml11_time(bytes, pos)?;
    let mut nanosecond = 0;
    if bytes.get(pos) == Some(&b'.') {
        pos += 1;
        let fraction_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            if pos - fraction_start < 9 {
                nanosecond = nanosecond * 10 + u32::from(bytes[pos] - b'0');
            } else if bytes[pos] != b'0' {
                return None;
            }
            pos += 1;
        }
        if pos == fraction_start {
            return None;
        }
        for _ in 0..9usize.saturating_sub(pos - fraction_start) {
            nanosecond *= 10;
        }
    }

    while pos < bytes.len() && yaml11_timestamp_space(bytes[pos]) {
        pos += 1;
    }

    let offset = if pos == bytes.len() {
        None
    } else {
        let (offset, after_offset) = parse_yaml11_timezone(bytes, pos)?;
        (after_offset == bytes.len()).then_some(offset)?
    };
    Some(Timestamp::new(
        date,
        Some(Time::from_hms_nano_offset(
            hour, minute, second, nanosecond, offset,
        )?),
    ))
}

fn parse_yaml11_date(bytes: &[u8], mut pos: usize) -> Option<(Date, usize)> {
    if pos + 4 > bytes.len() || !bytes[pos..pos + 4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let year = parse_digits_u16(&bytes[pos..pos + 4])?;
    pos += 4;
    if bytes.get(pos) != Some(&b'-') {
        return None;
    }
    pos += 1;

    let (month, after_month) = parse_one_or_two_digits(bytes, pos)?;
    if bytes.get(after_month) != Some(&b'-') {
        return None;
    }
    let (day, after_day) = parse_one_or_two_digits(bytes, after_month + 1)?;
    Some((Date::from_ymd(year, month, day)?, after_day))
}

fn parse_yaml11_time(bytes: &[u8], mut pos: usize) -> Option<(u8, u8, u8, usize)> {
    let (hour, after_hour) = parse_one_or_two_digits(bytes, pos)?;
    if hour > 23 || bytes.get(after_hour) != Some(&b':') {
        return None;
    }
    pos = after_hour + 1;

    let (minute, after_minute) = parse_exact_two_digits(bytes, pos)?;
    if minute > 59 || bytes.get(after_minute) != Some(&b':') {
        return None;
    }
    pos = after_minute + 1;

    let (second, after_second) = parse_exact_two_digits(bytes, pos)?;
    (second <= 60).then_some((hour, minute, second, after_second))
}

fn parse_yaml11_timezone(bytes: &[u8], mut pos: usize) -> Option<(Option<TimeZoneOffset>, usize)> {
    match bytes.get(pos) {
        Some(b'Z' | b'z') => return Some((Some(TimeZoneOffset::from_minutes(0)?), pos + 1)),
        Some(b'+') => pos += 1,
        Some(b'-') => pos += 1,
        _ => return None,
    }
    let negative = matches!(bytes.get(pos.saturating_sub(1)), Some(b'-'));
    let (hour, after_hour) = parse_one_or_two_digits(bytes, pos)?;
    if hour > 23 {
        return None;
    }
    let (minute, after_minute) = if bytes.get(after_hour) == Some(&b':') {
        let (minute, after_minute) = parse_exact_two_digits(bytes, after_hour + 1)?;
        if minute > 59 {
            return None;
        }
        (minute, after_minute)
    } else {
        (0, after_hour)
    };
    let total = i16::from(hour) * 60 + i16::from(minute);
    let minutes = if negative { -total } else { total };
    Some((Some(TimeZoneOffset::from_minutes(minutes)?), after_minute))
}

fn parse_digits_u16(bytes: &[u8]) -> Option<u16> {
    let mut value = 0u16;
    for byte in bytes {
        value = value
            .checked_mul(10)?
            .checked_add(u16::from(byte.checked_sub(b'0')?))?;
    }
    Some(value)
}

fn parse_one_or_two_digits(bytes: &[u8], pos: usize) -> Option<(u8, usize)> {
    let first = *bytes.get(pos)?;
    if !first.is_ascii_digit() {
        return None;
    }
    let mut value = first - b'0';
    let mut end = pos + 1;
    if let Some(second) = bytes.get(end)
        && second.is_ascii_digit()
    {
        value = value.checked_mul(10)?.checked_add(second - b'0')?;
        end += 1;
    }
    Some((value, end))
}

fn parse_exact_two_digits(bytes: &[u8], pos: usize) -> Option<(u8, usize)> {
    let value = bytes.get(pos)?.checked_sub(b'0')?;
    let second = bytes.get(pos + 1)?.checked_sub(b'0')?;
    if value > 9 || second > 9 {
        return None;
    }
    Some((value * 10 + second, pos + 2))
}

fn yaml11_timestamp_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

impl From<Node> for Value {
    fn from(node: Node) -> Self {
        node.into_value()
    }
}

impl From<&Node> for Value {
    fn from(node: &Node) -> Self {
        (&node.value).into()
    }
}

/// Spanful YAML node payload.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeValue {
    /// YAML null.
    Null,
    /// YAML boolean.
    Bool(bool),
    /// YAML number.
    Number(Number),
    /// YAML string.
    String(String),
    /// YAML sequence.
    Sequence(Vec<Node>),
    /// YAML mapping represented as ordered key/value nodes.
    Mapping(Vec<(Node, Node)>),
    /// Tagged YAML node.
    Tagged(Box<TaggedNode>),
}

/// Parsed YAML tag handle and suffix.
#[derive(Clone, Debug, Eq)]
pub struct Tag {
    /// Tag handle, such as `!`, `!!`, or `!foo!`.
    pub handle: String,
    /// Tag suffix after the handle.
    pub suffix: String,
}

impl Tag {
    /// Parses a raw YAML tag spelling into a tag handle and suffix.
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        assert!(!raw.is_empty(), "empty YAML tag is not allowed");
        if let Some(suffix) = raw.strip_prefix("!!") {
            Self {
                handle: "!!".to_string(),
                suffix: suffix.to_string(),
            }
        } else if let Some(rest) = raw.strip_prefix('!') {
            if rest.starts_with('<') && rest.ends_with('>') && rest.len() >= 2 {
                Self {
                    handle: "!".to_string(),
                    suffix: rest[1..rest.len() - 1].to_string(),
                }
            } else if let Some((handle, suffix)) = rest.split_once('!') {
                Self {
                    handle: format!("!{handle}!"),
                    suffix: suffix.to_string(),
                }
            } else {
                Self {
                    handle: "!".to_string(),
                    suffix: rest.to_string(),
                }
            }
        } else {
            Self {
                handle: "!".to_string(),
                suffix: raw,
            }
        }
    }

    /// Returns the Serde enum variant name represented by this tag.
    pub fn variant(&self) -> &str {
        &self.suffix
    }

    pub(crate) fn serde_variant(&self) -> Cow<'_, str> {
        if self.handle == "!" && self.suffix.is_empty() {
            Cow::Borrowed("!")
        } else if self.handle == "!" && !tag_suffix_needs_verbatim(&self.suffix) {
            Cow::Borrowed(&self.suffix)
        } else {
            Cow::Owned(self.to_string())
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.handle.as_str() {
            "!" if tag_suffix_needs_verbatim(&self.suffix) => {
                write!(formatter, "!<{}>", self.suffix)
            }
            "!" | "!!" => write!(formatter, "{}{}", self.handle, self.suffix),
            _ => write!(formatter, "{}{}", self.handle, self.suffix),
        }
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        tag_compare_key(&self.to_string()) == tag_compare_key(&other.to_string())
    }
}

impl<T> PartialEq<T> for Tag
where
    T: ?Sized + AsRef<str>,
{
    fn eq(&self, other: &T) -> bool {
        tag_compare_key(&self.to_string()) == tag_compare_key(other.as_ref())
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        tag_compare_key(&self.to_string()).cmp(tag_compare_key(&other.to_string()))
    }
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Tag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        tag_compare_key(&self.to_string()).hash(state);
    }
}

fn tag_compare_key(text: &str) -> &str {
    match text.strip_prefix('!') {
        Some("") | None => text,
        Some(unbanged) => unbanged,
    }
}

fn tag_suffix_needs_verbatim(suffix: &str) -> bool {
    !suffix.is_empty()
        && (suffix.starts_with("tag:")
            || suffix.starts_with(':')
            || suffix.starts_with('<')
            || suffix.ends_with(':')
            || suffix.contains('!')
            || suffix
                .chars()
                .any(|ch| ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '{' | '}')))
}

fn is_core_tag(tag: &Tag, suffix: &str) -> bool {
    tag.handle == "!!" && tag.suffix == suffix
}

fn is_timestamp_tag(tag: &Tag) -> bool {
    is_core_tag(tag, "timestamp")
}

fn is_bool_tag(tag: &Tag) -> bool {
    is_core_tag(tag, "bool")
}

fn is_null_tag(tag: &Tag) -> bool {
    is_core_tag(tag, "null")
}

fn is_int_tag(tag: &Tag) -> bool {
    is_core_tag(tag, "int")
}

fn is_float_tag(tag: &Tag) -> bool {
    is_core_tag(tag, "float")
}

fn explicit_core_bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) => yaml11::parse_bool(value),
        Value::Tagged(tagged) => explicit_core_bool_value(&tagged.value),
        _ => None,
    }
}

fn explicit_core_null_value(value: &Value) -> Option<()> {
    match value {
        Value::Null => Some(()),
        Value::String(value) if yaml11::is_null(value) => Some(()),
        Value::Tagged(tagged) => explicit_core_null_value(&tagged.value),
        _ => None,
    }
}

fn explicit_core_int_number_value(value: &Value) -> Option<Number> {
    match value {
        Value::Number(number) => Some(*number),
        Value::String(value) => yaml11::parse_explicit_int_number(value),
        Value::Tagged(tagged) => explicit_core_int_number_value(&tagged.value),
        _ => None,
    }
}

fn explicit_core_float_number_value(value: &Value) -> Option<Number> {
    match value {
        Value::Number(number) => Some(*number),
        Value::String(value) => yaml11::parse_explicit_float_number(value),
        Value::Tagged(tagged) => explicit_core_float_number_value(&tagged.value),
        _ => None,
    }
}

fn explicit_core_numeric_value(tagged: &TaggedValue) -> Option<Number> {
    if is_int_tag(&tagged.tag) {
        explicit_core_int_number_value(&tagged.value)
    } else if is_float_tag(&tagged.tag) {
        explicit_core_float_number_value(&tagged.value)
    } else {
        None
    }
}

/// Spanful YAML tagged node.
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedNode {
    /// YAML tag.
    pub tag: Tag,
    /// Source span of the tag token.
    pub tag_span: Span,
    /// Tagged node value.
    pub value: Node,
}

impl NodeValue {
    /// Returns the scalar string value, following transparent tags.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            NodeValue::String(value) => Some(value),
            NodeValue::Tagged(tagged) => tagged.value.as_str(),
            _ => None,
        }
    }

    /// Returns this payload as a YAML 1.1 timestamp, if it carries `!!timestamp`.
    pub fn as_timestamp(&self) -> Option<Timestamp> {
        match self {
            NodeValue::Tagged(tagged) if is_timestamp_tag(&tagged.tag) => {
                tagged.value.as_str().and_then(Timestamp::parse_yaml_1_1)
            }
            NodeValue::Tagged(tagged) => tagged.value.as_timestamp(),
            _ => None,
        }
    }

    /// Compares two node payloads by semantic value.
    pub fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (NodeValue::Null, NodeValue::Null) => true,
            (NodeValue::Bool(left), NodeValue::Bool(right)) => left == right,
            (NodeValue::Number(left), NodeValue::Number(right)) => left == right,
            (NodeValue::String(left), NodeValue::String(right)) => left == right,
            (NodeValue::Sequence(left), NodeValue::Sequence(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left, right)| left.equivalent(right))
            }
            (NodeValue::Mapping(left), NodeValue::Mapping(right)) => {
                left.len() == right.len()
                    && left.iter().zip(right.iter()).all(
                        |((left_key, left_value), (right_key, right_value))| {
                            left_key.equivalent(right_key) && left_value.equivalent(right_value)
                        },
                    )
            }
            (NodeValue::Tagged(left), NodeValue::Tagged(right)) => {
                left.tag == right.tag && left.value.equivalent(&right.value)
            }
            _ => false,
        }
    }
}

impl From<Value> for NodeValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => NodeValue::Null,
            Value::Bool(value) => NodeValue::Bool(value),
            Value::Number(value) => NodeValue::Number(value),
            Value::String(value) => NodeValue::String(value),
            Value::Sequence(items) => NodeValue::Sequence(
                items
                    .into_iter()
                    .map(|value| Node::new(value.into(), Span::default()))
                    .collect(),
            ),
            Value::Mapping(entries) => NodeValue::Mapping(
                entries
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            Node::new(key.into(), Span::default()),
                            Node::new(value.into(), Span::default()),
                        )
                    })
                    .collect(),
            ),
            Value::Tagged(tagged) => NodeValue::Tagged(Box::new(TaggedNode {
                tag: tagged.tag,
                tag_span: Span::default(),
                value: Node::new(tagged.value.into(), Span::default()),
            })),
        }
    }
}

impl From<NodeValue> for Value {
    fn from(value: NodeValue) -> Self {
        match value {
            NodeValue::Null => Value::Null,
            NodeValue::Bool(value) => Value::Bool(value),
            NodeValue::Number(value) => Value::Number(value),
            NodeValue::String(value) => Value::String(value),
            NodeValue::Sequence(items) => {
                Value::Sequence(items.into_iter().map(Node::into_value).collect())
            }
            NodeValue::Mapping(entries) => Value::Mapping(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.into_value(), value.into_value()))
                    .collect(),
            ),
            NodeValue::Tagged(tagged) => Value::Tagged(Box::new(TaggedValue {
                tag: tagged.tag,
                value: tagged.value.into_value(),
            })),
        }
    }
}

impl From<&NodeValue> for Value {
    fn from(value: &NodeValue) -> Self {
        match value {
            NodeValue::Null => Value::Null,
            NodeValue::Bool(value) => Value::Bool(*value),
            NodeValue::Number(value) => Value::Number(*value),
            NodeValue::String(value) => Value::String(value.clone()),
            NodeValue::Sequence(items) => Value::Sequence(items.iter().map(Value::from).collect()),
            NodeValue::Mapping(entries) => Value::Mapping(
                entries
                    .iter()
                    .map(|(key, value)| (Value::from(key), Value::from(value)))
                    .collect(),
            ),
            NodeValue::Tagged(tagged) => Value::Tagged(Box::new(TaggedValue {
                tag: tagged.tag.clone(),
                value: Value::from(&tagged.value),
            })),
        }
    }
}

/// Spanless YAML value tree used by Serde-facing APIs.
#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Hash)]
pub enum Value {
    /// YAML null.
    #[default]
    Null,
    /// YAML boolean.
    Bool(bool),
    /// YAML number.
    Number(Number),
    /// YAML string.
    String(String),
    /// YAML sequence.
    Sequence(Sequence),
    /// YAML mapping.
    Mapping(Mapping),
    /// Tagged YAML value.
    Tagged(Box<TaggedValue>),
}

impl Eq for Value {}

/// YAML sequence containing spanless [`Value`] elements.
pub type Sequence = Vec<Value>;

/// Spanless tagged YAML value.
#[derive(Clone, Debug, PartialEq, PartialOrd, Hash)]
pub struct TaggedValue {
    /// YAML tag.
    pub tag: Tag,
    /// Tagged value payload.
    pub value: Value,
}

macro_rules! value_from_number {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for Value {
                fn from(value: $ty) -> Self {
                    Value::Number(Number::from(value))
                }
            }
        )*
    };
}

value_from_number! {
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    f32, f64,
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_string())
    }
}

impl<'a> From<Cow<'a, str>> for Value {
    fn from(value: Cow<'a, str>) -> Self {
        Value::String(value.into_owned())
    }
}

impl From<Mapping> for Value {
    fn from(value: Mapping) -> Self {
        Value::Mapping(value)
    }
}

impl<T> From<Vec<T>> for Value
where
    T: Into<Value>,
{
    fn from(value: Vec<T>) -> Self {
        Value::Sequence(value.into_iter().map(Into::into).collect())
    }
}

impl<T> From<&[T]> for Value
where
    T: Clone + Into<Value>,
{
    fn from(value: &[T]) -> Self {
        Value::Sequence(value.iter().cloned().map(Into::into).collect())
    }
}

impl<T> FromIterator<T> for Value
where
    T: Into<Value>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Value::Sequence(iter.into_iter().map(Into::into).collect())
    }
}

/// Ordered YAML mapping with `Value` keys and values.
#[derive(Clone, Debug, Default)]
pub struct Mapping {
    entries: Vec<(Value, Value)>,
}

impl Mapping {
    /// Creates an empty mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty mapping with space for at least `capacity` entries.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Reserves capacity for at least `additional` more entries.
    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    /// Shrinks the backing storage as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.entries.shrink_to_fit();
    }

    /// Returns the current backing storage capacity.
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Inserts a key/value pair, returning the replaced value if the key existed.
    pub fn insert(&mut self, key: Value, value: Value) -> Option<Value> {
        if let Some((_, existing)) = self
            .entries
            .iter_mut()
            .find(|(existing_key, _)| existing_key == &key)
        {
            return Some(mem::replace(existing, value));
        }
        self.entries.push((key, value));
        None
    }

    /// Returns an entry API for inserting or modifying a value by key.
    pub fn entry(&mut self, key: Value) -> Entry<'_> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(existing_key, _)| existing_key == &key)
        {
            Entry::Occupied(OccupiedEntry {
                mapping: self,
                index,
            })
        } else {
            Entry::Vacant(VacantEntry { mapping: self, key })
        }
    }

    /// Removes a key using swap removal.
    pub fn remove<I>(&mut self, index: I) -> Option<Value>
    where
        I: MappingIndex,
    {
        self.swap_remove(index)
    }

    /// Removes a key/value pair using swap removal.
    pub fn remove_entry<I>(&mut self, index: I) -> Option<(Value, Value)>
    where
        I: MappingIndex,
    {
        self.swap_remove_entry(index)
    }

    /// Removes a key using swap removal.
    pub fn swap_remove<I>(&mut self, index: I) -> Option<Value>
    where
        I: MappingIndex,
    {
        index.swap_remove_from(self)
    }

    /// Removes a key/value pair using swap removal.
    pub fn swap_remove_entry<I>(&mut self, index: I) -> Option<(Value, Value)>
    where
        I: MappingIndex,
    {
        index.swap_remove_entry_from(self)
    }

    /// Removes a key while preserving entry order.
    pub fn shift_remove<I>(&mut self, index: I) -> Option<Value>
    where
        I: MappingIndex,
    {
        index.shift_remove_from(self)
    }

    /// Removes a key/value pair while preserving entry order.
    pub fn shift_remove_entry<I>(&mut self, index: I) -> Option<(Value, Value)>
    where
        I: MappingIndex,
    {
        index.shift_remove_entry_from(self)
    }

    /// Retains only entries selected by the predicate.
    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&Value, &mut Value) -> bool,
    {
        self.entries.retain_mut(|(key, value)| keep(&*key, value));
    }

    /// Returns a value for the key, if present.
    pub fn get<I>(&self, index: I) -> Option<&Value>
    where
        I: MappingIndex,
    {
        index.index_into_mapping(self)
    }

    /// Returns a mutable value for the key, if present.
    pub fn get_mut<I>(&mut self, index: I) -> Option<&mut Value>
    where
        I: MappingIndex,
    {
        index.index_into_mapping_mut(self)
    }

    /// Returns whether the mapping contains the key.
    pub fn contains_key<I>(&self, index: I) -> bool
    where
        I: MappingIndex,
    {
        index.is_key_into_mapping(self)
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the mapping contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns an iterator over key/value references.
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            iter: self.entries.iter(),
        }
    }

    /// Returns an iterator over key references and mutable value references.
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut {
            iter: self.entries.iter_mut(),
        }
    }

    /// Returns an iterator over keys.
    pub fn keys(&self) -> Keys<'_> {
        Keys {
            iter: self.entries.iter(),
        }
    }

    /// Returns an iterator over values.
    pub fn values(&self) -> Values<'_> {
        Values {
            iter: self.entries.iter(),
        }
    }

    /// Returns an iterator over mutable values.
    pub fn values_mut(&mut self) -> ValuesMut<'_> {
        ValuesMut {
            iter: self.entries.iter_mut(),
        }
    }

    /// Returns an owning iterator over keys.
    pub fn into_keys(self) -> IntoKeys {
        IntoKeys {
            iter: self.entries.into_iter(),
        }
    }

    /// Returns an owning iterator over values.
    pub fn into_values(self) -> IntoValues {
        IntoValues {
            iter: self.entries.into_iter(),
        }
    }

    /// Returns the ordered key/value entry slice.
    pub fn as_slice(&self) -> &[(Value, Value)] {
        &self.entries
    }
}

impl PartialEq for Mapping {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .entries
                .iter()
                .all(|(key, value)| other.get(key).is_some_and(|other| other == value))
    }
}

impl Eq for Mapping {}

#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for Mapping {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut xor = 0;
        for (key, value) in self {
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            value.hash(&mut hasher);
            xor ^= hasher.finish();
        }
        xor.hash(state);
    }
}

impl PartialOrd for Mapping {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut left = self.iter().collect::<Vec<_>>();
        let mut right = other.iter().collect::<Vec<_>>();
        left.sort_by(|(left_key, left_value), (right_key, right_value)| {
            total_value_cmp(left_key, right_key)
                .then_with(|| total_value_cmp(left_value, right_value))
        });
        right.sort_by(|(left_key, left_value), (right_key, right_value)| {
            total_value_cmp(left_key, right_key)
                .then_with(|| total_value_cmp(left_value, right_value))
        });
        Some(iter_cmp_by(
            left,
            right,
            |(left_key, left_value), (right_key, right_value)| {
                total_value_cmp(left_key, right_key)
                    .then_with(|| total_value_cmp(left_value, right_value))
            },
        ))
    }
}

fn total_value_cmp(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,

        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Bool(_), _) => Ordering::Less,
        (_, Value::Bool(_)) => Ordering::Greater,

        (Value::Number(left), Value::Number(right)) => total_number_cmp(left, right),
        (Value::Number(_), _) => Ordering::Less,
        (_, Value::Number(_)) => Ordering::Greater,

        (Value::String(left), Value::String(right)) => left.cmp(right),
        (Value::String(_), _) => Ordering::Less,
        (_, Value::String(_)) => Ordering::Greater,

        (Value::Sequence(left), Value::Sequence(right)) => {
            iter_cmp_by(left, right, total_value_cmp)
        }
        (Value::Sequence(_), _) => Ordering::Less,
        (_, Value::Sequence(_)) => Ordering::Greater,

        (Value::Mapping(left), Value::Mapping(right)) => {
            left.partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (Value::Mapping(_), _) => Ordering::Less,
        (_, Value::Mapping(_)) => Ordering::Greater,

        (Value::Tagged(left), Value::Tagged(right)) => left
            .tag
            .cmp(&right.tag)
            .then_with(|| total_value_cmp(&left.value, &right.value)),
    }
}

fn iter_cmp_by<I, F>(left: I, right: I, mut cmp: F) -> Ordering
where
    I: IntoIterator,
    F: FnMut(I::Item, I::Item) -> Ordering,
{
    let mut left = left.into_iter();
    let mut right = right.into_iter();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) => match cmp(left, right) {
                Ordering::Equal => {}
                order => return order,
            },
        }
    }
}

/// Iterator over borrowed mapping key/value pairs.
pub struct Iter<'a> {
    iter: std::slice::Iter<'a, (Value, Value)>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a Value, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(key, value)| (key, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(key, value)| (key, value))
    }
}

impl ExactSizeIterator for Iter<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over borrowed mapping keys and mutable values.
pub struct IterMut<'a> {
    iter: std::slice::IterMut<'a, (Value, Value)>,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = (&'a Value, &'a mut Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(key, value)| (&*key, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for IterMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(key, value)| (&*key, value))
    }
}

impl ExactSizeIterator for IterMut<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Owning iterator over mapping key/value pairs.
pub struct IntoIter {
    iter: std::vec::IntoIter<(Value, Value)>,
}

impl Iterator for IntoIter {
    type Item = (Value, Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl ExactSizeIterator for IntoIter {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over borrowed mapping keys.
pub struct Keys<'a> {
    iter: std::slice::Iter<'a, (Value, Value)>,
}

impl<'a> Iterator for Keys<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(key, _)| key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for Keys<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(key, _)| key)
    }
}

impl ExactSizeIterator for Keys<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Owning iterator over mapping keys.
pub struct IntoKeys {
    iter: std::vec::IntoIter<(Value, Value)>,
}

impl Iterator for IntoKeys {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(key, _)| key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for IntoKeys {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(key, _)| key)
    }
}

impl ExactSizeIterator for IntoKeys {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over borrowed mapping values.
pub struct Values<'a> {
    iter: std::slice::Iter<'a, (Value, Value)>,
}

impl<'a> Iterator for Values<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, value)| value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for Values<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(_, value)| value)
    }
}

impl ExactSizeIterator for Values<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over mutable mapping values.
pub struct ValuesMut<'a> {
    iter: std::slice::IterMut<'a, (Value, Value)>,
}

impl<'a> Iterator for ValuesMut<'a> {
    type Item = &'a mut Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, value)| value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for ValuesMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(_, value)| value)
    }
}

impl ExactSizeIterator for ValuesMut<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Owning iterator over mapping values.
pub struct IntoValues {
    iter: std::vec::IntoIter<(Value, Value)>,
}

impl Iterator for IntoValues {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, value)| value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for IntoValues {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|(_, value)| value)
    }
}

impl ExactSizeIterator for IntoValues {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Entry view into a [`Mapping`].
pub enum Entry<'a> {
    /// Existing mapping entry.
    Occupied(OccupiedEntry<'a>),
    /// Vacant mapping entry.
    Vacant(VacantEntry<'a>),
}

/// Occupied entry in a [`Mapping`].
pub struct OccupiedEntry<'a> {
    mapping: &'a mut Mapping,
    index: usize,
}

/// Vacant entry in a [`Mapping`].
pub struct VacantEntry<'a> {
    mapping: &'a mut Mapping,
    key: Value,
}

impl<'a> Entry<'a> {
    /// Returns the entry key.
    pub fn key(&self) -> &Value {
        match self {
            Entry::Occupied(entry) => entry.key(),
            Entry::Vacant(entry) => entry.key(),
        }
    }

    /// Inserts `default` when vacant and returns a mutable value reference.
    pub fn or_insert(self, default: Value) -> &'a mut Value {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default),
        }
    }

    /// Inserts a value produced by `default` when vacant.
    pub fn or_insert_with<F>(self, default: F) -> &'a mut Value
    where
        F: FnOnce() -> Value,
    {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default()),
        }
    }

    /// Modifies an occupied entry in place.
    pub fn and_modify<F>(self, f: F) -> Self
    where
        F: FnOnce(&mut Value),
    {
        match self {
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());
                Entry::Occupied(entry)
            }
            Entry::Vacant(entry) => Entry::Vacant(entry),
        }
    }
}

impl<'a> OccupiedEntry<'a> {
    /// Returns the entry key.
    pub fn key(&self) -> &Value {
        &self.mapping.entries[self.index].0
    }

    /// Returns the entry value.
    pub fn get(&self) -> &Value {
        &self.mapping.entries[self.index].1
    }

    /// Returns a mutable reference to the entry value.
    pub fn get_mut(&mut self) -> &mut Value {
        &mut self.mapping.entries[self.index].1
    }

    /// Converts the entry into a mutable value reference.
    pub fn into_mut(self) -> &'a mut Value {
        &mut self.mapping.entries[self.index].1
    }

    /// Replaces the entry value and returns the old value.
    pub fn insert(&mut self, value: Value) -> Value {
        mem::replace(&mut self.mapping.entries[self.index].1, value)
    }

    /// Removes the entry and returns its value.
    pub fn remove(self) -> Value {
        self.mapping.entries.swap_remove(self.index).1
    }

    /// Removes the entry and returns its key and value.
    pub fn remove_entry(self) -> (Value, Value) {
        self.mapping.entries.swap_remove(self.index)
    }
}

impl<'a> VacantEntry<'a> {
    /// Returns the key that would be inserted.
    pub fn key(&self) -> &Value {
        &self.key
    }

    /// Returns the owned key that would be inserted.
    pub fn into_key(self) -> Value {
        self.key
    }

    /// Inserts a value for this vacant key.
    pub fn insert(self, value: Value) -> &'a mut Value {
        let index = self.mapping.entries.len();
        self.mapping.entries.push((self.key, value));
        &mut self.mapping.entries[index].1
    }
}

impl Extend<(Value, Value)> for Mapping {
    fn extend<T: IntoIterator<Item = (Value, Value)>>(&mut self, iter: T) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl FromIterator<(Value, Value)> for Mapping {
    fn from_iter<T: IntoIterator<Item = (Value, Value)>>(iter: T) -> Self {
        let mut mapping = Mapping::new();
        mapping.extend(iter);
        mapping
    }
}

impl<'a> IntoIterator for &'a Mapping {
    type Item = (&'a Value, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Mapping {
    type Item = (&'a Value, &'a mut Value);
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl IntoIterator for Mapping {
    type Item = (Value, Value);
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            iter: self.entries.into_iter(),
        }
    }
}

impl<I> StdIndex<I> for Mapping
where
    I: MappingIndex,
{
    type Output = Value;

    fn index(&self, index: I) -> &Self::Output {
        index
            .index_into_mapping(self)
            .expect("no entry found for key")
    }
}

impl<I> StdIndexMut<I> for Mapping
where
    I: MappingIndex,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        index
            .index_into_mapping_mut(self)
            .expect("no entry found for key")
    }
}

impl<'de> serde::Deserialize<'de> for Mapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(MappingVisitor)
    }
}

struct MappingVisitor;

impl<'de> Visitor<'de> for MappingVisitor {
    type Value = Mapping;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML mapping")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Mapping::new())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut mapping = Mapping::new();
        while let Some((key, value)) = map.next_entry::<Value, Value>()? {
            if mapping.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "duplicate entry in YAML map for key {key:?}"
                )));
            }
            mapping.insert(key, value);
        }
        Ok(mapping)
    }
}

mod index_private {
    pub trait Sealed {}

    impl Sealed for usize {}
    impl Sealed for str {}
    impl Sealed for String {}
    impl Sealed for super::Value {}

    impl<T> Sealed for &T where T: ?Sized + Sealed {}
}

/// A key type that can index into a YAML [`Value`].
///
/// This trait is sealed; downstream crates can use the built-in implementations
/// for sequence indices (`usize`) and mapping keys (`str`, `String`, `Value`,
/// and references to those types), but cannot implement new index types.
pub trait Index: index_private::Sealed {
    /// Returns the value at this index, or `None` if the current value does not
    /// contain it.
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value>;
    /// Returns the mutable value at this index, or `None` if the current value
    /// does not contain it.
    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value>;
    /// Returns the mutable value at this index, inserting null mapping entries
    /// for missing mapping keys where `serde_yaml` would.
    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value;
}

/// A key type that can index directly into a YAML [`Mapping`].
///
/// This trait is sealed; downstream crates can use string-like keys or `Value`
/// keys, but cannot implement new mapping index types. Unlike [`Index`], this
/// trait does not include `usize` because a `Mapping` has no sequence position
/// indexing surface.
pub trait MappingIndex: index_private::Sealed {
    /// Returns whether the mapping contains this key.
    fn is_key_into_mapping(&self, mapping: &Mapping) -> bool;
    /// Returns the value for this key, if present.
    fn index_into_mapping<'a>(&self, mapping: &'a Mapping) -> Option<&'a Value>;
    /// Returns the mutable value for this key, if present.
    fn index_into_mapping_mut<'a>(&self, mapping: &'a mut Mapping) -> Option<&'a mut Value>;
    /// Removes this key using swap removal.
    fn swap_remove_from(&self, mapping: &mut Mapping) -> Option<Value>;
    /// Removes this key and value using swap removal.
    fn swap_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)>;
    /// Removes this key while preserving order.
    fn shift_remove_from(&self, mapping: &mut Mapping) -> Option<Value>;
    /// Removes this key and value while preserving order.
    fn shift_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)>;
}

impl Index for usize {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        match value {
            Value::Sequence(items) => items.get(*self),
            Value::Mapping(mapping) => mapping.get(numeric_index_key(*self)),
            Value::Tagged(tagged) => self.index_into(&tagged.value),
            _ => None,
        }
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        match value {
            Value::Sequence(items) => items.get_mut(*self),
            Value::Mapping(mapping) => mapping.get_mut(numeric_index_key(*self)),
            Value::Tagged(tagged) => self.index_into_mut(&mut tagged.value),
            _ => None,
        }
    }

    fn index_or_insert<'a>(&self, mut value: &'a mut Value) -> &'a mut Value {
        loop {
            match value {
                Value::Sequence(items) => {
                    let len = items.len();
                    return items.get_mut(*self).unwrap_or_else(|| {
                        panic!("cannot access index {self} of YAML sequence of length {len}")
                    });
                }
                Value::Mapping(mapping) => {
                    return mapping
                        .entry(numeric_index_key(*self))
                        .or_insert(Value::Null);
                }
                Value::Tagged(tagged) => value = &mut tagged.value,
                _ => panic!(
                    "cannot access index {self} of YAML {}",
                    value_type_name(value)
                ),
            }
        }
    }
}

impl Index for str {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        match value {
            Value::Mapping(mapping) => self.index_into_mapping(mapping),
            Value::Tagged(tagged) => self.index_into(&tagged.value),
            _ => None,
        }
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        match value {
            Value::Mapping(mapping) => self.index_into_mapping_mut(mapping),
            Value::Tagged(tagged) => self.index_into_mut(&mut tagged.value),
            _ => None,
        }
    }

    fn index_or_insert<'a>(&self, mut value: &'a mut Value) -> &'a mut Value {
        if let Value::Null = value {
            *value = Value::Mapping(Mapping::new());
        }

        loop {
            match value {
                Value::Mapping(mapping) => {
                    return mapping
                        .entry(Value::String(self.to_string()))
                        .or_insert(Value::Null);
                }
                Value::Tagged(tagged) => value = &mut tagged.value,
                _ => panic!(
                    "cannot access key {self:?} in YAML {}",
                    value_type_name(value)
                ),
            }
        }
    }
}

impl Index for &str {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        str::index_into(self, value)
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        str::index_into_mut(self, value)
    }

    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value {
        str::index_or_insert(self, value)
    }
}

impl Index for String {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        self.as_str().index_into(value)
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        self.as_str().index_into_mut(value)
    }

    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value {
        self.as_str().index_or_insert(value)
    }
}

impl Index for Value {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        match value {
            Value::Mapping(mapping) => mapping.get(self),
            Value::Tagged(tagged) => self.index_into(&tagged.value),
            _ => None,
        }
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        match value {
            Value::Mapping(mapping) => mapping.get_mut(self),
            Value::Tagged(tagged) => self.index_into_mut(&mut tagged.value),
            _ => None,
        }
    }

    fn index_or_insert<'a>(&self, mut value: &'a mut Value) -> &'a mut Value {
        if let Value::Null = value {
            *value = Value::Mapping(Mapping::new());
        }

        loop {
            match value {
                Value::Mapping(mapping) => {
                    return mapping.entry(self.clone()).or_insert(Value::Null);
                }
                Value::Tagged(tagged) => value = &mut tagged.value,
                _ => panic!(
                    "cannot access key {self:?} in YAML {}",
                    value_type_name(value)
                ),
            }
        }
    }
}

impl Index for &Value {
    fn index_into<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        (*self).index_into(value)
    }

    fn index_into_mut<'a>(&self, value: &'a mut Value) -> Option<&'a mut Value> {
        (*self).index_into_mut(value)
    }

    fn index_or_insert<'a>(&self, value: &'a mut Value) -> &'a mut Value {
        (*self).index_or_insert(value)
    }
}

impl MappingIndex for str {
    fn is_key_into_mapping(&self, mapping: &Mapping) -> bool {
        mapping
            .entries
            .iter()
            .any(|(key, _)| matches!(key, Value::String(existing) if existing == self))
    }

    fn index_into_mapping<'a>(&self, mapping: &'a Mapping) -> Option<&'a Value> {
        mapping.entries.iter().find_map(|(key, value)| {
            matches!(key, Value::String(existing) if existing == self).then_some(value)
        })
    }

    fn index_into_mapping_mut<'a>(&self, mapping: &'a mut Mapping) -> Option<&'a mut Value> {
        mapping.entries.iter_mut().find_map(|(key, value)| {
            matches!(key, Value::String(existing) if existing == self).then_some(value)
        })
    }

    fn swap_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        string_index_position_in_mapping(mapping, self)
            .map(|index| mapping.entries.swap_remove(index).1)
    }

    fn swap_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        string_index_position_in_mapping(mapping, self)
            .map(|index| mapping.entries.swap_remove(index))
    }

    fn shift_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        string_index_position_in_mapping(mapping, self).map(|index| mapping.entries.remove(index).1)
    }

    fn shift_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        string_index_position_in_mapping(mapping, self).map(|index| mapping.entries.remove(index))
    }
}

impl MappingIndex for String {
    fn is_key_into_mapping(&self, mapping: &Mapping) -> bool {
        self.as_str().is_key_into_mapping(mapping)
    }

    fn index_into_mapping<'a>(&self, mapping: &'a Mapping) -> Option<&'a Value> {
        self.as_str().index_into_mapping(mapping)
    }

    fn index_into_mapping_mut<'a>(&self, mapping: &'a mut Mapping) -> Option<&'a mut Value> {
        self.as_str().index_into_mapping_mut(mapping)
    }

    fn swap_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        self.as_str().swap_remove_from(mapping)
    }

    fn swap_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        self.as_str().swap_remove_entry_from(mapping)
    }

    fn shift_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        self.as_str().shift_remove_from(mapping)
    }

    fn shift_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        self.as_str().shift_remove_entry_from(mapping)
    }
}

impl MappingIndex for Value {
    fn is_key_into_mapping(&self, mapping: &Mapping) -> bool {
        mapping.entries.iter().any(|(key, _)| key == self)
    }

    fn index_into_mapping<'a>(&self, mapping: &'a Mapping) -> Option<&'a Value> {
        mapping
            .entries
            .iter()
            .find_map(|(key, value)| (key == self).then_some(value))
    }

    fn index_into_mapping_mut<'a>(&self, mapping: &'a mut Mapping) -> Option<&'a mut Value> {
        mapping
            .entries
            .iter_mut()
            .find_map(|(key, value)| (key == self).then_some(value))
    }

    fn swap_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        value_index_position_in_mapping(mapping, self)
            .map(|index| mapping.entries.swap_remove(index).1)
    }

    fn swap_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        value_index_position_in_mapping(mapping, self)
            .map(|index| mapping.entries.swap_remove(index))
    }

    fn shift_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        value_index_position_in_mapping(mapping, self).map(|index| mapping.entries.remove(index).1)
    }

    fn shift_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        value_index_position_in_mapping(mapping, self).map(|index| mapping.entries.remove(index))
    }
}

impl<T> MappingIndex for &T
where
    T: ?Sized + MappingIndex,
{
    fn is_key_into_mapping(&self, mapping: &Mapping) -> bool {
        (**self).is_key_into_mapping(mapping)
    }

    fn index_into_mapping<'a>(&self, mapping: &'a Mapping) -> Option<&'a Value> {
        (**self).index_into_mapping(mapping)
    }

    fn index_into_mapping_mut<'a>(&self, mapping: &'a mut Mapping) -> Option<&'a mut Value> {
        (**self).index_into_mapping_mut(mapping)
    }

    fn swap_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        (**self).swap_remove_from(mapping)
    }

    fn swap_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        (**self).swap_remove_entry_from(mapping)
    }

    fn shift_remove_from(&self, mapping: &mut Mapping) -> Option<Value> {
        (**self).shift_remove_from(mapping)
    }

    fn shift_remove_entry_from(&self, mapping: &mut Mapping) -> Option<(Value, Value)> {
        (**self).shift_remove_entry_from(mapping)
    }
}

static NULL_VALUE: Value = Value::Null;

impl<I> StdIndex<I> for Value
where
    I: Index,
{
    type Output = Value;

    fn index(&self, index: I) -> &Self::Output {
        self.get(index).unwrap_or(&NULL_VALUE)
    }
}

impl<I> StdIndexMut<I> for Value
where
    I: Index,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        index.index_or_insert(self)
    }
}

fn numeric_index_key(index: usize) -> Value {
    Value::Number(Number::Unsigned(index as u128))
}

fn value_index_position_in_mapping(mapping: &Mapping, index: &Value) -> Option<usize> {
    mapping
        .entries
        .iter()
        .position(|(existing, _)| existing == index)
}

fn string_index_position_in_mapping(mapping: &Mapping, index: &str) -> Option<usize> {
    mapping
        .entries
        .iter()
        .position(|(existing, _)| matches!(existing, Value::String(value) if value == index))
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(tagged) => value_type_name(&tagged.value),
    }
}

impl Value {
    /// Expands YAML merge keys in place using `serde_yaml::Value`-style rules.
    pub fn apply_merge(&mut self) -> crate::Result<()> {
        let mut values = vec![self];
        while let Some(value) = values.pop() {
            match value {
                Value::Mapping(mapping) => {
                    match mapping.shift_remove("<<") {
                        Some(Value::Mapping(merge)) => merge_mapping(mapping, merge),
                        Some(Value::Sequence(sequence)) => {
                            for value in sequence {
                                match value {
                                    Value::Mapping(merge) => merge_mapping(mapping, merge),
                                    Value::Sequence(_) => {
                                        return Err(merge_error(
                                            "expected a mapping for merging, but found sequence",
                                        ));
                                    }
                                    Value::Tagged(_) => {
                                        return Err(merge_error(
                                            "unexpected tagged value in merge",
                                        ));
                                    }
                                    _ => {
                                        return Err(merge_error(
                                            "expected a mapping for merging, but found scalar",
                                        ));
                                    }
                                }
                            }
                        }
                        Some(Value::Tagged(_)) => {
                            return Err(merge_error("unexpected tagged value in merge"));
                        }
                        Some(_) => {
                            return Err(merge_error(
                                "expected a mapping or list of mappings for merging, but found scalar",
                            ));
                        }
                        None => {}
                    }
                    values.extend(mapping.values_mut());
                }
                Value::Sequence(sequence) => values.extend(sequence),
                Value::Tagged(tagged) => values.push(&mut tagged.value),
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns a nested value by sequence index or mapping key.
    pub fn get<I>(&self, index: I) -> Option<&Value>
    where
        I: Index,
    {
        index.index_into(self)
    }

    /// Returns a mutable nested value by sequence index or mapping key.
    pub fn get_mut<I>(&mut self, index: I) -> Option<&mut Value>
    where
        I: Index,
    {
        index.index_into_mut(self)
    }

    /// Returns `Some(())` if this value is null.
    pub fn as_null(&self) -> Option<()> {
        match self {
            Value::Null => Some(()),
            Value::Tagged(tagged) if is_null_tag(&tagged.tag) => {
                explicit_core_null_value(&tagged.value)
            }
            Value::Tagged(tagged) => tagged.value.as_null(),
            _ => None,
        }
    }

    /// Returns the boolean value, if this value is a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            Value::Tagged(tagged) if is_bool_tag(&tagged.tag) => {
                explicit_core_bool_value(&tagged.value)
            }
            Value::Tagged(tagged) => tagged.value.as_bool(),
            _ => None,
        }
    }

    /// Returns this value as an `i64`, if it is an in-range integer.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(number) => number.as_i64(),
            Value::Tagged(tagged) if is_int_tag(&tagged.tag) || is_float_tag(&tagged.tag) => {
                explicit_core_numeric_value(tagged).and_then(|number| number.as_i64())
            }
            Value::Tagged(tagged) => tagged.value.as_i64(),
            _ => None,
        }
    }

    /// Returns this value as a `u64`, if it is an in-range nonnegative integer.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Number(number) => number.as_u64(),
            Value::Tagged(tagged) if is_int_tag(&tagged.tag) || is_float_tag(&tagged.tag) => {
                explicit_core_numeric_value(tagged).and_then(|number| number.as_u64())
            }
            Value::Tagged(tagged) => tagged.value.as_u64(),
            _ => None,
        }
    }

    /// Returns this value as an `i128`, if it is an in-range integer.
    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Value::Number(number) => number.as_i128(),
            Value::Tagged(tagged) if is_int_tag(&tagged.tag) || is_float_tag(&tagged.tag) => {
                explicit_core_numeric_value(tagged).and_then(|number| number.as_i128())
            }
            Value::Tagged(tagged) => tagged.value.as_i128(),
            _ => None,
        }
    }

    /// Returns this value as a `u128`, if it is an in-range nonnegative integer.
    pub fn as_u128(&self) -> Option<u128> {
        match self {
            Value::Number(number) => number.as_u128(),
            Value::Tagged(tagged) if is_int_tag(&tagged.tag) || is_float_tag(&tagged.tag) => {
                explicit_core_numeric_value(tagged).and_then(|number| number.as_u128())
            }
            Value::Tagged(tagged) => tagged.value.as_u128(),
            _ => None,
        }
    }

    /// Returns this value as an `f64`, if it is numeric.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(number) => number.as_f64(),
            Value::Tagged(tagged) if is_int_tag(&tagged.tag) || is_float_tag(&tagged.tag) => {
                explicit_core_numeric_value(tagged).and_then(|number| number.as_f64())
            }
            Value::Tagged(tagged) => tagged.value.as_f64(),
            _ => None,
        }
    }

    /// Returns this value as a string slice, if it is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(value) => Some(value),
            Value::Tagged(tagged) => tagged.value.as_str(),
            _ => None,
        }
    }

    /// Returns this value as a YAML 1.1 timestamp, if it carries `!!timestamp`.
    pub fn as_timestamp(&self) -> Option<Timestamp> {
        match self {
            Value::Tagged(tagged) if is_timestamp_tag(&tagged.tag) => {
                tagged.value.as_str().and_then(Timestamp::parse_yaml_1_1)
            }
            Value::Tagged(tagged) => tagged.value.as_timestamp(),
            _ => None,
        }
    }

    /// Returns this value as a sequence, if it is a sequence.
    pub fn as_sequence(&self) -> Option<&Sequence> {
        match self {
            Value::Sequence(items) => Some(items),
            Value::Tagged(tagged) => tagged.value.as_sequence(),
            _ => None,
        }
    }

    /// Returns this value as a mutable sequence, if it is a sequence.
    pub fn as_sequence_mut(&mut self) -> Option<&mut Sequence> {
        match self {
            Value::Sequence(items) => Some(items),
            Value::Tagged(tagged) => tagged.value.as_sequence_mut(),
            _ => None,
        }
    }

    /// Returns this value as a mapping, if it is a mapping.
    pub fn as_mapping(&self) -> Option<&Mapping> {
        match self {
            Value::Mapping(entries) => Some(entries),
            Value::Tagged(tagged) => tagged.value.as_mapping(),
            _ => None,
        }
    }

    /// Returns this value as a mutable mapping, if it is a mapping.
    pub fn as_mapping_mut(&mut self) -> Option<&mut Mapping> {
        match self {
            Value::Mapping(entries) => Some(entries),
            Value::Tagged(tagged) => tagged.value.as_mapping_mut(),
            _ => None,
        }
    }

    /// Returns the tagged value wrapper, if this value is tagged.
    pub fn as_tagged(&self) -> Option<&TaggedValue> {
        match self {
            Value::Tagged(tagged) => Some(tagged),
            _ => None,
        }
    }

    /// Returns whether this value is null.
    pub fn is_null(&self) -> bool {
        self.as_null().is_some()
    }

    /// Returns whether this value is a boolean.
    pub fn is_bool(&self) -> bool {
        self.as_bool().is_some()
    }

    /// Returns whether this value is numeric.
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
            || matches!(self, Value::Tagged(tagged) if explicit_core_numeric_value(tagged).is_some())
            || matches!(self, Value::Tagged(tagged) if tagged.value.is_number())
    }

    /// Returns whether this value can be represented as an `i64`.
    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    /// Returns whether this value can be represented as a `u64`.
    pub fn is_u64(&self) -> bool {
        self.as_u64().is_some()
    }

    /// Returns whether this value is stored as a floating-point number.
    pub fn is_f64(&self) -> bool {
        matches!(self, Value::Number(number) if number.is_f64())
            || matches!(self, Value::Tagged(tagged) if explicit_core_numeric_value(tagged).is_some_and(|number| number.is_f64()))
            || matches!(self, Value::Tagged(tagged) if tagged.value.is_f64())
    }

    /// Returns whether this value is a string.
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
            || matches!(self, Value::Tagged(tagged) if tagged.value.is_string())
    }

    /// Returns whether this value carries a YAML 1.1 timestamp.
    pub fn is_timestamp(&self) -> bool {
        self.as_timestamp().is_some()
    }

    /// Returns whether this value is a sequence.
    pub fn is_sequence(&self) -> bool {
        matches!(self, Value::Sequence(_))
            || matches!(self, Value::Tagged(tagged) if tagged.value.is_sequence())
    }

    /// Returns whether this value is a mapping.
    pub fn is_mapping(&self) -> bool {
        matches!(self, Value::Mapping(_))
            || matches!(self, Value::Tagged(tagged) if tagged.value.is_mapping())
    }

    /// Returns whether this value has an explicit YAML tag.
    pub fn is_tagged(&self) -> bool {
        matches!(self, Value::Tagged(_))
    }

    /// Compares two values by semantic value.
    pub fn equivalent(&self, other: &Self) -> bool {
        self == other
    }
}

impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str().is_some_and(|value| value == other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().is_some_and(|value| value == *other)
    }
}

impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool {
        self.as_str().is_some_and(|value| value == other)
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool().is_some_and(|value| value == *other)
    }
}

macro_rules! value_partial_eq_numeric {
    ($([$($ty:ty)*], $conversion:ident, $base:ty)*) => {
        $($(
            impl PartialEq<$ty> for Value {
                fn eq(&self, other: &$ty) -> bool {
                    self.$conversion().is_some_and(|value| value == (*other as $base))
                }
            }

            impl PartialEq<$ty> for &Value {
                fn eq(&self, other: &$ty) -> bool {
                    self.$conversion().is_some_and(|value| value == (*other as $base))
                }
            }

            impl PartialEq<$ty> for &mut Value {
                fn eq(&self, other: &$ty) -> bool {
                    self.$conversion().is_some_and(|value| value == (*other as $base))
                }
            }
        )*)*
    }
}

value_partial_eq_numeric! {
    [i8 i16 i32 i64 isize], as_i64, i64
    [u8 u16 u32 u64 usize], as_u64, u64
    [f32 f64], as_f64, f64
}

fn merge_mapping(mapping: &mut Mapping, merge: Mapping) {
    for (key, value) in merge {
        if !mapping.contains_key(&key) {
            mapping.insert(key, value);
        }
    }
}

fn merge_error(message: &'static str) -> Error {
    Error::new(message, Span::default())
}

fn apply_merge_keys_in_node(root: &mut Node) -> crate::Result<()> {
    let mut values = vec![root];
    while let Some(node) = values.pop() {
        match &mut node.value {
            NodeValue::Mapping(entries) => {
                apply_merge_entries(entries)?;
                values.extend(entries.iter_mut().map(|(_, value)| value));
            }
            NodeValue::Sequence(items) => values.extend(items),
            NodeValue::Tagged(tagged) => values.push(&mut tagged.value),
            _ => {}
        }
    }
    Ok(())
}

fn apply_merge_entries(entries: &mut Vec<(Node, Node)>) -> crate::Result<()> {
    if let Some(merge) = shift_remove_merge_node(entries) {
        merge_node_mapping(entries, merge)?;
    }
    Ok(())
}

fn shift_remove_merge_node(entries: &mut Vec<(Node, Node)>) -> Option<Node> {
    entries
        .iter()
        .position(|(key, _)| matches!(&key.value, NodeValue::String(value) if value == "<<"))
        .map(|index| entries.remove(index).1)
}

fn merge_node_mapping(entries: &mut Vec<(Node, Node)>, merge: Node) -> crate::Result<()> {
    let span = merge.span;
    match merge.value {
        NodeValue::Mapping(mut merge_entries) => {
            apply_merge_entries(&mut merge_entries)?;
            insert_missing_node_entries(entries, merge_entries)
        }
        NodeValue::Sequence(sequence) => {
            for value in sequence {
                let span = value.span;
                match value.value {
                    NodeValue::Mapping(mut merge_entries) => {
                        apply_merge_entries(&mut merge_entries)?;
                        insert_missing_node_entries(entries, merge_entries)?
                    }
                    NodeValue::Sequence(_) => {
                        return Err(merge_node_error(
                            "expected a mapping for merging, but found sequence",
                            span,
                        ));
                    }
                    NodeValue::Tagged(_) => {
                        return Err(merge_node_error("unexpected tagged value in merge", span));
                    }
                    _ => {
                        return Err(merge_node_error(
                            "expected a mapping for merging, but found scalar",
                            span,
                        ));
                    }
                }
            }
            Ok(())
        }
        NodeValue::Tagged(_) => Err(merge_node_error("unexpected tagged value in merge", span)),
        _ => Err(merge_node_error(
            "expected a mapping or list of mappings for merging, but found scalar",
            span,
        )),
    }
}

fn insert_missing_node_entries(
    entries: &mut Vec<(Node, Node)>,
    merge_entries: Vec<(Node, Node)>,
) -> crate::Result<()> {
    for (key, value) in merge_entries {
        if !node_mapping_contains_key(entries, &key)? {
            entries.push((key, value));
        }
    }
    Ok(())
}

fn node_mapping_contains_key(entries: &[(Node, Node)], key: &Node) -> crate::Result<bool> {
    for (existing, _) in entries {
        if same_key_identity(existing, key)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn merge_node_error(message: &'static str, span: Span) -> Error {
    Error::new(message, span)
}

/// YAML number representation.
#[derive(Clone, Copy, Debug)]
pub enum Number {
    /// Signed integer.
    Integer(i128),
    /// Unsigned integer.
    Unsigned(u128),
    /// Floating-point number.
    Float(f64),
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Number::Float(left), Number::Float(right)) if left.is_nan() && right.is_nan() => true,
            (Number::Float(left), Number::Float(right)) => left == right,
            (Number::Float(_), _) | (_, Number::Float(_)) => false,
            _ => total_number_cmp(self, other) == Ordering::Equal,
        }
    }
}

impl Eq for Number {}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (*self, *other) {
            (Number::Float(left), Number::Float(right)) if left.is_nan() && right.is_nan() => {
                Some(Ordering::Equal)
            }
            (Number::Float(left), Number::Float(right)) => left.partial_cmp(&right),
            _ => Some(total_number_cmp(self, other)),
        }
    }
}

#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for Number {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Number::Integer(value) if *value < 0 => {
                0u8.hash(state);
                value.hash(state);
            }
            Number::Integer(value) => {
                1u8.hash(state);
                (*value as u128).hash(state);
            }
            Number::Unsigned(value) => {
                1u8.hash(state);
                value.hash(state);
            }
            Number::Float(_) => 2u8.hash(state),
        }
    }
}

fn total_number_cmp(left: &Number, right: &Number) -> Ordering {
    match (*left, *right) {
        (Number::Integer(left), Number::Integer(right)) => left.cmp(&right),
        (Number::Unsigned(left), Number::Unsigned(right)) => left.cmp(&right),
        (Number::Integer(left), Number::Unsigned(right)) => {
            if left < 0 {
                Ordering::Less
            } else {
                (left as u128).cmp(&right)
            }
        }
        (Number::Unsigned(left), Number::Integer(right)) => {
            if right < 0 {
                Ordering::Greater
            } else {
                left.cmp(&(right as u128))
            }
        }
        (Number::Float(left), Number::Float(right)) => {
            left.partial_cmp(&right).unwrap_or_else(|| {
                if !left.is_nan() {
                    Ordering::Less
                } else if !right.is_nan() {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
        }
        (Number::Integer(_) | Number::Unsigned(_), Number::Float(_)) => Ordering::Less,
        (Number::Float(_), Number::Integer(_) | Number::Unsigned(_)) => Ordering::Greater,
    }
}

impl Number {
    /// Returns whether this number fits in `i64`.
    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    /// Returns whether this number fits in `u64`.
    pub fn is_u64(&self) -> bool {
        self.as_u64().is_some()
    }

    /// Returns whether this number fits in `i128`.
    pub fn is_i128(&self) -> bool {
        matches!(self, Number::Integer(_))
            || matches!(self, Number::Unsigned(value) if i128::try_from(*value).is_ok())
    }

    /// Returns whether this number fits in `u128`.
    pub fn is_u128(&self) -> bool {
        matches!(self, Number::Unsigned(_)) || matches!(self, Number::Integer(value) if *value >= 0)
    }

    /// Returns whether this number is stored as `f64`.
    pub fn is_f64(&self) -> bool {
        matches!(self, Number::Float(_))
    }

    /// Returns whether this number is NaN.
    pub fn is_nan(&self) -> bool {
        matches!(self, Number::Float(value) if value.is_nan())
    }

    /// Returns whether this number is infinite.
    pub fn is_infinite(&self) -> bool {
        matches!(self, Number::Float(value) if value.is_infinite())
    }

    /// Returns whether this number is finite.
    pub fn is_finite(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Unsigned(_) => true,
            Number::Float(value) => value.is_finite(),
        }
    }

    /// Returns this number as `i64`, if possible.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Number::Integer(value) => i64::try_from(*value).ok(),
            Number::Unsigned(value) => i64::try_from(*value).ok(),
            Number::Float(_) => None,
        }
    }

    /// Returns this number as `u64`, if possible.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Number::Integer(value) => u64::try_from(*value).ok(),
            Number::Unsigned(value) => u64::try_from(*value).ok(),
            Number::Float(_) => None,
        }
    }

    /// Returns this number as `i128`, if possible.
    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Number::Integer(value) => Some(*value),
            Number::Unsigned(value) => i128::try_from(*value).ok(),
            Number::Float(_) => None,
        }
    }

    /// Returns this number as `u128`, if possible.
    pub fn as_u128(&self) -> Option<u128> {
        match self {
            Number::Integer(value) => u128::try_from(*value).ok(),
            Number::Unsigned(value) => Some(*value),
            Number::Float(_) => None,
        }
    }

    /// Returns this number as `f64`, if possible.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Number::Integer(value) => Some(*value as f64),
            Number::Unsigned(value) => Some(*value as f64),
            Number::Float(value) => Some(*value),
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(value) => write!(formatter, "{value}"),
            Number::Unsigned(value) => write!(formatter, "{value}"),
            Number::Float(value) if value.is_nan() => formatter.write_str(".nan"),
            Number::Float(value) if value.is_infinite() => {
                if value.is_sign_negative() {
                    formatter.write_str("-.inf")
                } else {
                    formatter.write_str(".inf")
                }
            }
            Number::Float(value) => formatter.write_str(ryu::Buffer::new().format_finite(*value)),
        }
    }
}

impl FromStr for Number {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_number_text(text).ok_or_else(|| Error::new("failed to parse number", Span::default()))
    }
}

macro_rules! number_from_signed {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for Number {
                fn from(value: $ty) -> Self {
                    Number::Integer(value as i128)
                }
            }
        )*
    };
}

macro_rules! number_from_unsigned {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for Number {
                fn from(value: $ty) -> Self {
                    Number::Unsigned(value as u128)
                }
            }
        )*
    };
}

number_from_signed!(i8, i16, i32, i64, i128, isize);
number_from_unsigned!(u8, u16, u32, u64, u128, usize);

impl From<f32> for Number {
    fn from(value: f32) -> Self {
        Number::from(f64::from(value))
    }
}

impl From<f64> for Number {
    fn from(mut value: f64) -> Self {
        if value.is_nan() {
            value = f64::NAN.copysign(1.0);
        }
        Number::Float(value)
    }
}

fn parse_number_text(text: &str) -> Option<Number> {
    let compact = text.replace('_', "");
    if is_number_int_like(&compact) {
        return if compact.starts_with('-') {
            compact.parse::<i128>().ok().map(Number::Integer)
        } else {
            parse_positive_number_text(compact.strip_prefix('+').unwrap_or(&compact))
        };
    }
    parse_special_float_text(&compact).or_else(|| {
        is_number_float_like(&compact)
            .then(|| compact.parse::<f64>().ok().map(Number::from))
            .flatten()
    })
}

fn parse_positive_number_text(text: &str) -> Option<Number> {
    text.parse::<i64>()
        .ok()
        .map(|value| Number::Integer(i128::from(value)))
        .or_else(|| {
            text.parse::<u64>()
                .ok()
                .map(|value| Number::Unsigned(u128::from(value)))
        })
        .or_else(|| text.parse::<i128>().ok().map(Number::Integer))
        .or_else(|| text.parse::<u128>().ok().map(Number::Unsigned))
}

fn parse_special_float_text(text: &str) -> Option<Number> {
    if text.eq_ignore_ascii_case(".nan") {
        return Some(Number::from(f64::NAN));
    }
    if text.eq_ignore_ascii_case(".inf") || text.eq_ignore_ascii_case("+.inf") {
        return Some(Number::from(f64::INFINITY));
    }
    if text.eq_ignore_ascii_case("-.inf") {
        return Some(Number::from(f64::NEG_INFINITY));
    }
    None
}

fn is_number_int_like(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut idx = usize::from(matches!(bytes[0], b'+' | b'-'));
    let mut digits = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'0'..=b'9' => digits += 1,
            _ => return false,
        }
        idx += 1;
    }
    digits > 0
}

fn is_number_float_like(text: &str) -> bool {
    if !(text.contains('.') || text.contains('e') || text.contains('E')) {
        return false;
    }
    let bytes = text.as_bytes();
    let mut digits = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'+' | b'-' if idx == 0 => {}
            b'+' | b'-' if matches!(bytes.get(idx.wrapping_sub(1)), Some(b'e' | b'E')) => {}
            b'.' | b'e' | b'E' => {}
            _ => return false,
        }
    }
    digits > 0
}

struct NumberVisitor;

impl<'de> Visitor<'de> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML number")
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Number::from(value))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E> {
        Ok(Number::from(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Number::from(value))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E> {
        Ok(Number::from(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(Number::from(value))
    }
}

impl<'de> serde::Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NumberVisitor)
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}

impl<'de> serde::Deserialize<'de> for TaggedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TaggedValueVisitor;

        impl<'de> Visitor<'de> for TaggedValueVisitor {
            type Value = TaggedValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a YAML tagged value")
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (tag, contents) = data.variant_seed(TagVisitor)?;
                let value = contents.newtype_variant()?;
                Ok(TaggedValue { tag, value })
            }
        }

        deserializer.deserialize_any(TaggedValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::Integer(i128::from(value))))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::Integer(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::Unsigned(u128::from(value))))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::Unsigned(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::Float(value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_string()))
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(value) = seq.next_element::<Value>()? {
            items.push(value);
        }
        Ok(Value::Sequence(items))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = Mapping::new();
        while let Some((key, value)) = map.next_entry::<Value, Value>()? {
            entries.insert(key, value);
        }
        Ok(Value::Mapping(entries))
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: EnumAccess<'de>,
    {
        let (tag, contents) = data.variant_seed(TagVisitor)?;
        let value = contents.newtype_variant()?;
        Ok(Value::Tagged(Box::new(TaggedValue { tag, value })))
    }
}

struct TagVisitor;

impl<'de> de::DeserializeSeed<'de> for TagVisitor {
    type Value = Tag;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for TagVisitor {
    type Value = Tag;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML tag")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Tag::new(value))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Tag::new(value))
    }
}
