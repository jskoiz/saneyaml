use serde::{
    Deserialize, Serialize,
    de::{self, IgnoredAny, IntoDeserializer, Visitor},
    ser::SerializeMap,
    ser::SerializeSeq,
    ser::SerializeStruct,
    ser::SerializeStructVariant,
    ser::SerializeTuple,
    ser::SerializeTupleStruct,
    ser::SerializeTupleVariant,
};
use std::borrow::Cow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet, hash_map::DefaultHasher};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use yaml::{LoadOptions, Mapping, Number, Sequence, Tag, TaggedValue, Value};

#[derive(Debug, Deserialize, PartialEq)]
struct Config {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct NameOnly {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct FlattenedYamlExtras {
    name: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct FlattenedReferenceExtras {
    name: String,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type")]
enum TaggedService {
    Http { port: u16 },
}

#[derive(Debug, Deserialize, PartialEq)]
struct UntaggedNumericPort {
    port: NumericOrString,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum NumericOrString {
    Number(u16),
    Text(String),
}

#[derive(Debug, Default, Deserialize, PartialEq)]
struct DefaultedCollections {
    #[serde(default)]
    ports: Vec<u16>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SerializableConfig {
    name: String,
    ports: Vec<u16>,
    enabled: bool,
    env: BTreeMap<String, String>,
    optional: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedConfig {
    name: String,
    ports: Vec<u16>,
    limits: BTreeMap<String, String>,
    enabled: bool,
    optional: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorScalarRead {
    first: String,
    second: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorSequenceRead {
    first: Vec<String>,
    second: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorMappingRead {
    first: BTreeMap<String, String>,
    second: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorUnsignedRead {
    first: u64,
    second: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorFloatRead {
    first: f64,
    second: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TaggedAnchorKeyRead {
    root: BTreeMap<String, String>,
    alias_value: String,
}

#[derive(Clone, Debug, PartialEq)]
enum TaggedAnchorPayload {
    Text(String),
    List(Vec<String>),
    Map(BTreeMap<String, String>),
    CoreUnsigned { text: &'static str, value: u64 },
    CoreFloat { text: &'static str, value: f64 },
}

#[derive(Clone, Debug)]
enum TaggedAnchorShape {
    ValuePair(TaggedAnchorPayload),
    KeyPair,
}

#[derive(Debug, Deserialize, PartialEq)]
struct BorrowedConfig<'a> {
    name: &'a str,
    path: &'a str,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(bound(deserialize = "'de: 'a"))]
struct BorrowedScalarTargets<'a> {
    responses: BTreeMap<&'a str, &'a str>,
    vars: BTreeMap<&'a str, &'a str>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct BorrowedValue<'a> {
    value: &'a str,
}

#[derive(Debug, Deserialize, PartialEq)]
struct CowValue<'a> {
    value: Cow<'a, str>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedBorrowedConfig<'a> {
    name: &'a str,
    path: &'a str,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct StrictConfig {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AliasService {
    image: String,
    environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AliasCompose {
    services: BTreeMap<String, AliasService>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct StreamPortConfig {
    port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
struct StreamAliasConfig {
    service: StreamPortConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
struct FlowStringKeyRoot {
    root: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct FlowSequenceKeyRoot {
    root: BTreeMap<Vec<String>, String>,
}

fn assert_borrowed_from(source: &str, borrowed: &str) {
    let source_start = source.as_ptr() as usize;
    let source_end = source_start + source.len();
    let borrowed_start = borrowed.as_ptr() as usize;
    let borrowed_end = borrowed_start + borrowed.len();
    assert!(
        borrowed_start >= source_start && borrowed_end <= source_end,
        "`{borrowed}` should borrow from input range {source_start:#x}..{source_end:#x}, got {borrowed_start:#x}..{borrowed_end:#x}"
    );
    let offset = borrowed_start - source_start;
    assert_eq!(
        &source.as_bytes()[offset..offset + borrowed.len()],
        borrowed.as_bytes()
    );
}

fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn assert_value_traits<T: Eq + Hash + PartialOrd>() {}

fn assert_tag_traits<T: Eq + Hash + Ord>() {}

fn assert_tagged_traits<T: Hash + PartialOrd>() {}

#[derive(Debug, Deserialize, PartialEq)]
enum TaggedEnum {
    Unit,
    Newtype(u32),
    Tuple(u8, u8, u8),
    Struct { x: f64, y: f64 },
    String(String),
}

#[derive(Debug, Deserialize, PartialEq)]
enum SingletonAction {
    Unit,
    Newtype(String),
    Tuple(u8, u8),
    Shell { run: String },
}

#[derive(Debug, Serialize)]
enum SerializableAction {
    Unit,
    Newtype(String),
    Tuple(u8, u8),
    Shell { run: String },
}

#[derive(Debug, Serialize)]
struct SerializableSingletonMapConfig {
    #[serde(with = "yaml::with::singleton_map")]
    action: SerializableAction,
}

#[derive(Debug, Serialize)]
struct ReferenceSerializableSingletonMapConfig {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: SerializableAction,
}

#[derive(Debug, Serialize)]
struct SerializableRecursiveSingletonMapConfig {
    #[serde(with = "yaml::with::singleton_map_recursive")]
    actions: SerializableActions,
}

#[derive(Debug, Serialize)]
struct ReferenceSerializableRecursiveSingletonMapConfig {
    #[serde(with = "serde_yaml::with::singleton_map_recursive")]
    actions: SerializableActions,
}

#[derive(Debug, Serialize)]
struct SerializableActions {
    primary: SerializableAction,
    steps: Vec<SerializableAction>,
    by_name: BTreeMap<String, SerializableAction>,
}

#[derive(Clone, Copy)]
struct CollectStrTagKey(&'static str);

impl std::fmt::Display for CollectStrTagKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Serialize for CollectStrTagKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

struct SerializableCollectStrTagMap;

impl Serialize for SerializableCollectStrTagMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&CollectStrTagKey("!Thing"), &"x")?;
        map.end()
    }
}

#[derive(Clone, Copy)]
struct SerializableCollectStrLoneBangMap;

impl Serialize for SerializableCollectStrLoneBangMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&CollectStrTagKey("!"), &"x")?;
        map.end()
    }
}

#[derive(Clone, Copy)]
struct EmptyNewtypeVariant;

impl Serialize for EmptyNewtypeVariant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_newtype_variant("EmptyVariant", 0, "", &"x")
    }
}

#[derive(Clone, Copy)]
struct EmptyTupleVariant;

impl Serialize for EmptyTupleVariant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tuple = serializer.serialize_tuple_variant("EmptyVariant", 0, "", 1)?;
        tuple.serialize_field(&"x")?;
        tuple.end()
    }
}

#[derive(Clone, Copy)]
struct EmptyStructVariant;

impl Serialize for EmptyStructVariant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut fields = serializer.serialize_struct_variant("EmptyVariant", 0, "", 1)?;
        fields.serialize_field("value", &"x")?;
        fields.end()
    }
}

#[derive(Clone, Copy)]
struct SerializableBytes(&'static [u8]);

impl Serialize for SerializableBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.0)
    }
}

#[derive(Debug, PartialEq)]
struct BytesFromDeserializeBytes(Vec<u8>);

#[derive(Debug, PartialEq)]
struct BytesFromDeserializeByteBuf(Vec<u8>);

struct ByteVisitor;

impl<'de> Visitor<'de> for ByteVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bytes")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value.to_vec())
    }

    fn visit_borrowed_bytes<E>(self, value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value.to_vec())
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for BytesFromDeserializeBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer
            .deserialize_bytes(ByteVisitor)
            .map(BytesFromDeserializeBytes)
    }
}

impl<'de> Deserialize<'de> for BytesFromDeserializeByteBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer
            .deserialize_byte_buf(ByteVisitor)
            .map(BytesFromDeserializeByteBuf)
    }
}

#[derive(Clone, Copy)]
struct OneShotScalar<'a> {
    calls: &'a Cell<usize>,
    first: &'static str,
    later: &'static str,
}

impl<'a> OneShotScalar<'a> {
    fn new(calls: &'a Cell<usize>, first: &'static str, later: &'static str) -> Self {
        Self {
            calls,
            first,
            later,
        }
    }
}

impl Serialize for OneShotScalar<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let calls = self.calls.get();
        self.calls.set(calls + 1);
        if calls == 0 {
            serializer.serialize_str(self.first)
        } else {
            serializer.serialize_str(self.later)
        }
    }
}

struct OneShotMap<'a> {
    key_calls: &'a Cell<usize>,
    value_calls: &'a Cell<usize>,
}

impl Serialize for OneShotMap<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let key = OneShotScalar::new(self.key_calls, "first_key", "later_key");
        let value = OneShotScalar::new(self.value_calls, "first_value", "later_value");
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&key, &value)?;
        map.end()
    }
}

struct OneShotCollectStrKey<'a> {
    calls: &'a Cell<usize>,
}

impl std::fmt::Display for OneShotCollectStrKey<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let calls = self.calls.get();
        self.calls.set(calls + 1);
        formatter.write_str("!")?;
        if calls == 0 {
            formatter.write_str("First")
        } else {
            formatter.write_str("Second")
        }
    }
}

impl Serialize for OneShotCollectStrKey<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

struct OneShotCollectStrMap<'a> {
    key_calls: &'a Cell<usize>,
}

impl Serialize for OneShotCollectStrMap<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let key = OneShotCollectStrKey {
            calls: self.key_calls,
        };
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&key, &"value")?;
        map.end()
    }
}

struct OneShotStruct<'a> {
    value_calls: &'a Cell<usize>,
}

impl Serialize for OneShotStruct<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = OneShotScalar::new(self.value_calls, "first_field", "later_field");
        let mut fields = serializer.serialize_struct("OneShotStruct", 1)?;
        fields.serialize_field("field", &value)?;
        fields.end()
    }
}

struct OneShotNewtypeVariant<'a> {
    value_calls: &'a Cell<usize>,
}

impl Serialize for OneShotNewtypeVariant<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = OneShotScalar::new(self.value_calls, "first_newtype", "later_newtype");
        serializer.serialize_newtype_variant("OneShotEnum", 0, "Newtype", &value)
    }
}

struct OneShotTupleVariant<'a> {
    value_calls: &'a Cell<usize>,
}

impl Serialize for OneShotTupleVariant<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = OneShotScalar::new(self.value_calls, "first_tuple", "later_tuple");
        let mut tuple = serializer.serialize_tuple_variant("OneShotEnum", 0, "Tuple", 1)?;
        tuple.serialize_field(&value)?;
        tuple.end()
    }
}

struct OneShotStructVariant<'a> {
    value_calls: &'a Cell<usize>,
}

impl Serialize for OneShotStructVariant<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = OneShotScalar::new(
            self.value_calls,
            "first_struct_variant",
            "later_struct_variant",
        );
        let mut fields = serializer.serialize_struct_variant("OneShotEnum", 0, "Struct", 1)?;
        fields.serialize_field("field", &value)?;
        fields.end()
    }
}

#[derive(Clone, Copy)]
struct HostileSequenceLengthHint;

impl Serialize for HostileSequenceLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(usize::MAX))?;
        sequence.serialize_element(&"x")?;
        sequence.end()
    }
}

#[derive(Clone, Copy)]
struct HostileTupleLengthHint;

impl Serialize for HostileTupleLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tuple = serializer.serialize_tuple(usize::MAX)?;
        tuple.serialize_element(&"x")?;
        tuple.end()
    }
}

#[derive(Clone, Copy)]
struct HostileTupleStructLengthHint;

impl Serialize for HostileTupleStructLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tuple = serializer.serialize_tuple_struct("HostileTuple", usize::MAX)?;
        tuple.serialize_field(&"x")?;
        tuple.end()
    }
}

#[derive(Clone, Copy)]
struct HostileTupleVariantLengthHint;

impl Serialize for HostileTupleVariantLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tuple =
            serializer.serialize_tuple_variant("HostileVariant", 0, "Tuple", usize::MAX)?;
        tuple.serialize_field(&"x")?;
        tuple.end()
    }
}

#[derive(Clone, Copy)]
struct HostileMapLengthHint;

impl Serialize for HostileMapLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(usize::MAX))?;
        map.serialize_entry("k", &"v")?;
        map.end()
    }
}

#[derive(Clone, Copy)]
struct HostileStructLengthHint;

impl Serialize for HostileStructLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut fields = serializer.serialize_struct("HostileStruct", usize::MAX)?;
        fields.serialize_field("k", &"v")?;
        fields.end()
    }
}

#[derive(Clone, Copy)]
struct HostileStructVariantLengthHint;

impl Serialize for HostileStructVariantLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut fields =
            serializer.serialize_struct_variant("HostileVariant", 0, "Struct", usize::MAX)?;
        fields.serialize_field("k", &"v")?;
        fields.end()
    }
}

#[derive(Clone, Copy)]
struct HostileMappingKeyLengthHint;

impl Serialize for HostileMappingKeyLengthHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&HostileSequenceLengthHint, &"v")?;
        map.end()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct RecursiveActions {
    primary: SingletonAction,
    steps: Vec<SingletonAction>,
    by_name: BTreeMap<String, SingletonAction>,
}

#[test]
fn serde_api_from_slice_reader_and_value_roundtrip() {
    let input = b"name: app\nports: [80, 443]\nenabled: true\n";
    let from_slice: Config = yaml::from_slice(input).expect("from_slice");
    let from_reader: Config = yaml::from_reader(Cursor::new(input)).expect("from_reader");
    let from_reader_turbofish: Config =
        yaml::from_reader::<_, Config>(Cursor::new(input)).expect("from_reader generic order");
    let reference_from_reader: Config =
        serde_yaml::from_reader::<_, Config>(Cursor::new(input)).expect("serde_yaml from_reader");
    assert_eq!(from_slice, from_reader);
    assert_eq!(from_slice, from_reader_turbofish);
    assert_eq!(from_slice, reference_from_reader);

    let value: Value = yaml::from_slice(input).expect("deserialize into Value");
    assert_eq!(value["name"].as_str(), Some("app"));
    assert_eq!(value["ports"][0].as_u64(), Some(80));
    assert_eq!(value["ports"][1].as_i64(), Some(443));
    assert_eq!(value["enabled"].as_bool(), Some(true));

    let from_value: Config = yaml::from_value(value).expect("from_value");
    assert_eq!(from_value, from_slice);
}

#[test]
fn serde_api_leading_utf8_bom_matches_serde_yaml_for_read_entrypoints() {
    let input = "\u{feff}name: app\n";
    let expected = NameOnly {
        name: "app".to_string(),
    };

    let from_str: NameOnly = yaml::from_str(input).expect("from_str leading BOM");
    let from_slice: NameOnly = yaml::from_slice(input.as_bytes()).expect("from_slice leading BOM");
    let from_reader: NameOnly =
        yaml::from_reader(Cursor::new(input.as_bytes())).expect("from_reader leading BOM");
    let direct_str: NameOnly = NameOnly::deserialize(yaml::Deserializer::from_str(input))
        .expect("direct deserializer from_str leading BOM");
    let direct_slice: NameOnly =
        NameOnly::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("direct deserializer from_slice leading BOM");
    let direct_reader: NameOnly = NameOnly::deserialize(yaml::Deserializer::from_reader(
        Cursor::new(input.as_bytes()),
    ))
    .expect("direct deserializer from_reader leading BOM");
    let reference: NameOnly = serde_yaml::from_str(input).expect("serde_yaml leading BOM");

    assert_eq!(from_str, expected);
    assert_eq!(from_slice, expected);
    assert_eq!(from_reader, expected);
    assert_eq!(direct_str, expected);
    assert_eq!(direct_slice, expected);
    assert_eq!(direct_reader, expected);
    assert_eq!(from_str, reference);

    let map: BTreeMap<String, String> = yaml::from_str(input).expect("BOM mapping");
    assert_eq!(map.get("name").map(String::as_str), Some("app"));
    assert!(!map.contains_key("\u{feff}name"));

    let borrowed_input = "\u{feff}value: app\n";
    let borrowed: BorrowedValue<'_> = yaml::from_str(borrowed_input).expect("borrowed BOM value");
    assert_eq!(borrowed.value, "app");
    assert_borrowed_from(borrowed_input, borrowed.value);

    let sequence_input = "\u{feff}- app\n";
    let sequence: Vec<String> = yaml::from_str(sequence_input).expect("BOM sequence");
    let reference_sequence: Vec<String> =
        serde_yaml::from_str(sequence_input).expect("serde_yaml BOM sequence");
    assert_eq!(sequence, reference_sequence);

    let flow_input = "\u{feff}{name: app}\n";
    let flow: BTreeMap<String, String> = yaml::from_str(flow_input).expect("BOM flow mapping");
    let reference_flow: BTreeMap<String, String> =
        serde_yaml::from_str(flow_input).expect("serde_yaml BOM flow mapping");
    assert_eq!(flow, reference_flow);
}

#[test]
fn serde_api_deserialize_any_integer_dispatch_matches_serde_yaml_for_buffered_paths() {
    let flatten_input = "name: app\nimage: nginx\nreplicas: 3\n";
    let ours: FlattenedYamlExtras = yaml::from_str(flatten_input).expect("yaml flatten extras");
    let reference: FlattenedReferenceExtras =
        serde_yaml::from_str(flatten_input).expect("serde_yaml flatten extras");
    assert_eq!(ours.name, reference.name);
    assert_eq!(
        ours.extra["image"].as_str(),
        reference.extra["image"].as_str()
    );
    assert_eq!(
        ours.extra["replicas"].as_u64(),
        reference.extra["replicas"].as_u64()
    );

    let tagged_input = "type: Http\nport: 8080\n";
    let tagged: TaggedService = yaml::from_str(tagged_input).expect("yaml tagged service");
    let tagged_reference: TaggedService =
        serde_yaml::from_str(tagged_input).expect("serde_yaml tagged service");
    assert_eq!(tagged, tagged_reference);

    let node = yaml::parse_str(tagged_input).expect("parse tagged service");
    let from_node: TaggedService = yaml::from_node(&node).expect("from_node tagged service");
    let from_owned_node: TaggedService =
        TaggedService::deserialize(node).expect("owned node tagged service");
    assert_eq!(from_node, tagged_reference);
    assert_eq!(from_owned_node, tagged_reference);

    let untagged_input = "port: 8080\n";
    let untagged: UntaggedNumericPort =
        yaml::from_str(untagged_input).expect("yaml untagged numeric port");
    let untagged_reference: UntaggedNumericPort =
        serde_yaml::from_str(untagged_input).expect("serde_yaml untagged numeric port");
    assert_eq!(untagged, untagged_reference);

    let value: Value = yaml::from_str(untagged_input).expect("deserialize into Value");
    let from_value: UntaggedNumericPort =
        yaml::from_value(value.clone()).expect("from_value untagged numeric port");
    let from_value_ref: UntaggedNumericPort =
        UntaggedNumericPort::deserialize(&value).expect("value ref untagged numeric port");
    assert_eq!(from_value, untagged_reference);
    assert_eq!(from_value_ref, untagged_reference);
}

#[test]
fn serde_api_to_value_serializes_common_config_shapes_like_serde_yaml() {
    let config = SerializableConfig {
        name: "app".to_string(),
        ports: vec![80, 443],
        enabled: true,
        env: BTreeMap::from([
            ("CARGO_TERM_COLOR".to_string(), "always".to_string()),
            ("RUST_LOG".to_string(), "info".to_string()),
        ]),
        optional: None,
    };

    let value = yaml::to_value(&config).expect("yaml to_value");
    let reference = serde_yaml::to_value(&config).expect("serde_yaml to_value");

    assert_eq!(value["name"].as_str(), reference["name"].as_str());
    assert_eq!(value["ports"][0].as_u64(), reference["ports"][0].as_u64());
    assert_eq!(value["ports"][1].as_u64(), reference["ports"][1].as_u64());
    assert_eq!(value["enabled"].as_bool(), reference["enabled"].as_bool());
    assert_eq!(
        value["env"]["CARGO_TERM_COLOR"].as_str(),
        reference["env"]["CARGO_TERM_COLOR"].as_str()
    );
    assert!(value["optional"].is_null());

    let from_value: Config = yaml::from_value(value).expect("deserialize to_value output");
    assert_eq!(
        from_value,
        Config {
            name: "app".to_string(),
            ports: vec![80, 443],
            enabled: true,
        }
    );
}

#[test]
fn serde_api_to_value_serializes_enum_shapes_like_serde_yaml() {
    let cases = [
        SerializableAction::Unit,
        SerializableAction::Newtype("deploy".to_string()),
        SerializableAction::Tuple(4, 2),
        SerializableAction::Shell {
            run: "cargo test".to_string(),
        },
    ];

    for action in cases {
        let value = yaml::to_value(&action).expect("yaml enum to_value");
        let reference = serde_yaml::to_value(&action).expect("serde_yaml enum to_value");

        match action {
            SerializableAction::Unit => {
                assert_eq!(value.as_str(), reference.as_str());
                assert_eq!(value.as_str(), Some("Unit"));
            }
            SerializableAction::Newtype(_) => {
                let tagged = value.as_tagged().expect("newtype variant tag");
                let serde_yaml::Value::Tagged(reference) = reference else {
                    panic!("serde_yaml newtype variant should be tagged");
                };
                assert_eq!(tagged.tag.to_string(), reference.tag.to_string());
                assert_eq!(tagged.value.as_str(), Some("deploy"));
            }
            SerializableAction::Tuple(_, _) => {
                let tagged = value.as_tagged().expect("tuple variant tag");
                let serde_yaml::Value::Tagged(reference) = reference else {
                    panic!("serde_yaml tuple variant should be tagged");
                };
                assert_eq!(tagged.tag.to_string(), reference.tag.to_string());
                assert_eq!(tagged.value[0].as_u64(), Some(4));
                assert_eq!(tagged.value[1].as_u64(), Some(2));
            }
            SerializableAction::Shell { .. } => {
                let tagged = value.as_tagged().expect("struct variant tag");
                let serde_yaml::Value::Tagged(reference) = reference else {
                    panic!("serde_yaml struct variant should be tagged");
                };
                assert_eq!(tagged.tag.to_string(), reference.tag.to_string());
                assert_eq!(tagged.value["run"].as_str(), Some("cargo test"));
            }
        }
    }
}

#[test]
fn serde_api_to_value_preserves_yaml_value_tags() {
    let tagged = Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new("Thing"),
        value: Value::String("tagged".to_string()),
    }));

    let value = yaml::to_value(tagged).expect("serialize tagged yaml Value");
    let tagged = value.as_tagged().expect("tag survives to_value");
    assert_eq!(tagged.tag, Tag::new("Thing"));
    assert_eq!(tagged.value.as_str(), Some("tagged"));
}

#[test]
fn serde_api_to_value_keeps_ordinary_collect_str_tag_like_keys_as_mappings() {
    let value = yaml::to_value(SerializableCollectStrTagMap).expect("yaml collect_str tag map");
    let reference =
        serde_yaml::to_value(SerializableCollectStrTagMap).expect("serde_yaml collect_str tag map");

    assert!(value.as_tagged().is_none());
    assert_eq!(value["!Thing"].as_str(), reference["!Thing"].as_str());
    let emitted =
        yaml::to_string(&SerializableCollectStrTagMap).expect("yaml collect_str tag output");
    let reparsed: Value = yaml::from_str(&emitted).expect("reparse collect_str tag output");
    assert_eq!(reparsed["!Thing"].as_str(), Some("x"));

    let ordinary = BTreeMap::from([("!Thing".to_string(), "x".to_string())]);
    let ordinary_value = yaml::to_value(&ordinary).expect("ordinary string key");
    assert!(ordinary_value.as_tagged().is_none());
    let ordinary_emitted = yaml::to_string(&ordinary).expect("yaml ordinary string tag-like key");
    let ordinary_reparsed: Value =
        yaml::from_str(&ordinary_emitted).expect("reparse ordinary string tag-like key");
    assert_eq!(ordinary_reparsed["!Thing"].as_str(), Some("x"));
}

#[test]
fn serde_api_to_value_keeps_lone_bang_collect_str_keys_as_mappings_like_serde_yaml() {
    let value =
        yaml::to_value(SerializableCollectStrLoneBangMap).expect("yaml collect_str lone bang map");
    let reference = serde_yaml::to_value(SerializableCollectStrLoneBangMap)
        .expect("serde_yaml collect_str lone bang map");

    assert!(value.as_tagged().is_none());
    assert_eq!(value["!"].as_str(), reference["!"].as_str());
    let emitted =
        yaml::to_string(&SerializableCollectStrLoneBangMap).expect("yaml lone bang output");
    let reparsed: Value = yaml::from_str(&emitted).expect("reparse lone bang output");
    assert_eq!(reparsed["!"].as_str(), Some("x"));
}

#[test]
fn serde_api_tagged_value_serializes_like_serde_yaml_singleton_tag_map() {
    let tagged = TaggedValue {
        tag: Tag::new("Thing"),
        value: Value::String("x".to_string()),
    };
    let reference = serde_yaml::value::TaggedValue {
        tag: serde_yaml::value::Tag::new("Thing"),
        value: serde_yaml::Value::String("x".to_string()),
    };

    let value = yaml::to_value(&tagged).expect("yaml tagged value to_value");
    let tagged_value = value.as_tagged().expect("yaml TaggedValue stays tagged");
    let reference_value = serde_yaml::to_value(&reference).expect("serde_yaml tagged value");
    let serde_yaml::Value::Tagged(reference_tagged) = reference_value else {
        panic!("serde_yaml TaggedValue should serialize as tagged");
    };
    assert_eq!(
        tagged_value.tag.to_string(),
        reference_tagged.tag.to_string()
    );
    assert_eq!(tagged_value.value.as_str(), reference_tagged.value.as_str());
    assert_eq!(
        yaml::to_string(&tagged).expect("yaml tagged value output"),
        serde_yaml::to_string(&reference).expect("serde_yaml tagged value output")
    );

    let external = yaml::to_value(&reference).expect("serde_yaml TaggedValue through yaml");
    assert!(external.as_tagged().is_some());
}

#[test]
fn serde_api_non_specific_tagged_value_round_trips_even_though_lone_bang_keys_are_mappings() {
    let tagged = TaggedValue {
        tag: Tag::new("!"),
        value: Value::Null,
    };

    let value = yaml::to_value(&tagged).expect("yaml non-specific tagged value");
    let tagged_value = value.as_tagged().expect("non-specific tag survives");
    assert_eq!(tagged_value.tag, Tag::new("!"));
    assert!(tagged_value.value.is_null());

    let emitted = yaml::to_string(&tagged).expect("emit non-specific tag");
    let reparsed: Value = yaml::from_str(&emitted).expect("reparse non-specific tag");
    let reparsed = reparsed.as_tagged().expect("reparsed non-specific tag");
    assert_eq!(reparsed.tag, Tag::new("!"));
    assert!(reparsed.value.is_null());
}

fn assert_empty_variant_rejected_like_serde_yaml<T>(value: T)
where
    T: Copy + Serialize,
{
    let error = yaml::to_value(value).expect_err("yaml rejects empty variant tag");
    let reference = serde_yaml::to_value(value).expect_err("serde_yaml rejects empty variant tag");
    assert_eq!(error.to_string(), reference.to_string());

    let direct = value
        .serialize(yaml::value::Serializer)
        .expect_err("yaml value serializer rejects empty variant tag");
    assert_eq!(direct.to_string(), reference.to_string());

    let mut writer = Vec::new();
    let mut serializer = yaml::Serializer::new(&mut writer);
    let writer_error = value
        .serialize(&mut serializer)
        .expect_err("yaml writer serializer rejects empty variant tag");
    assert_eq!(writer_error.to_string(), reference.to_string());
}

#[test]
fn serde_api_empty_variant_tags_are_rejected_like_serde_yaml_value_serializer() {
    assert_empty_variant_rejected_like_serde_yaml(EmptyNewtypeVariant);
    assert_empty_variant_rejected_like_serde_yaml(EmptyTupleVariant);
    assert_empty_variant_rejected_like_serde_yaml(EmptyStructVariant);
}

#[test]
fn serde_api_empty_tag_constructor_matches_serde_yaml() {
    assert!(std::panic::catch_unwind(|| Tag::new("")).is_err());
    assert!(std::panic::catch_unwind(|| serde_yaml::value::Tag::new("")).is_err());

    assert_eq!(Tag::new("!").to_string(), "!");
    assert_eq!(Tag::new("Thing").to_string(), "!Thing");
}

#[test]
fn serde_api_value_module_serializer_matches_to_value_path() {
    let config = SerializableConfig {
        name: "app".to_string(),
        ports: vec![80, 443],
        enabled: true,
        env: BTreeMap::from([("RUST_LOG".to_string(), "info".to_string())]),
        optional: None,
    };

    let direct = config
        .serialize(yaml::value::Serializer)
        .expect("yaml value serializer");
    let helper = yaml::value::to_value(&config).expect("yaml value::to_value");

    assert_eq!(direct, helper);
    assert_eq!(direct["name"].as_str(), Some("app"));
    assert_eq!(direct["ports"][1].as_u64(), Some(443));
    assert!(direct["optional"].is_null());
}

#[test]
fn serde_api_to_value_serializes_bytes_like_serde_yaml_value_serializer() {
    let bytes = SerializableBytes(b"\0A\xff");
    let value = yaml::to_value(bytes).expect("yaml to_value bytes");
    let direct = bytes
        .serialize(yaml::value::Serializer)
        .expect("yaml value serializer bytes");
    let reference = serde_yaml::to_value(bytes).expect("serde_yaml to_value bytes");

    assert_eq!(value, direct);
    let sequence = value.as_sequence().expect("yaml bytes become sequence");
    let serde_yaml::Value::Sequence(reference_sequence) = reference else {
        panic!("serde_yaml bytes become sequence");
    };
    assert_eq!(sequence.len(), reference_sequence.len());
    for (item, reference_item) in sequence.iter().zip(reference_sequence.iter()) {
        assert_eq!(item.as_u64(), reference_item.as_u64());
    }
}

#[test]
fn serde_api_value_byte_deserialization_matches_serde_yaml_value_policy() {
    let expected = vec![0, 65, 255];
    let bytes = SerializableBytes(b"\0A\xff");
    let value = yaml::to_value(bytes).expect("yaml to_value bytes");
    let reference = serde_yaml::to_value(bytes).expect("serde_yaml to_value bytes");

    let ours_vec: Vec<u8> = yaml::from_value(value.clone()).expect("yaml byte sequence to Vec<u8>");
    let reference_vec: Vec<u8> =
        serde_yaml::from_value(reference.clone()).expect("serde_yaml byte sequence to Vec<u8>");
    assert_eq!(ours_vec, expected);
    assert_eq!(ours_vec, reference_vec);

    assert!(
        serde_yaml::from_value::<BytesFromDeserializeBytes>(reference.clone()).is_err(),
        "serde_yaml rejects byte visitors for value sequences"
    );
    assert!(
        yaml::from_value::<BytesFromDeserializeBytes>(value.clone()).is_err(),
        "yaml rejects byte visitors for value sequences"
    );
    assert!(
        BytesFromDeserializeBytes::deserialize(&value).is_err(),
        "borrowed yaml Value rejects byte visitors for value sequences"
    );

    assert!(
        serde_yaml::from_value::<BytesFromDeserializeByteBuf>(reference).is_err(),
        "serde_yaml rejects byte_buf visitors for value sequences"
    );
    assert!(
        yaml::from_value::<BytesFromDeserializeByteBuf>(value.clone()).is_err(),
        "yaml rejects byte_buf visitors for value sequences"
    );
    assert!(
        BytesFromDeserializeByteBuf::deserialize(&value).is_err(),
        "borrowed yaml Value rejects byte_buf visitors for value sequences"
    );
}

#[test]
fn serde_api_value_byte_deserialization_rejects_strings_like_serde_yaml() {
    let value = Value::String("abc".to_string());
    let reference = serde_yaml::Value::String("abc".to_string());

    assert!(
        serde_yaml::from_value::<BytesFromDeserializeBytes>(reference.clone()).is_err(),
        "serde_yaml rejects string values for deserialize_bytes"
    );
    assert!(
        yaml::from_value::<BytesFromDeserializeBytes>(value.clone()).is_err(),
        "yaml rejects string values for deserialize_bytes"
    );
    assert!(
        BytesFromDeserializeBytes::deserialize(&value).is_err(),
        "borrowed yaml Value rejects string values for deserialize_bytes"
    );

    assert!(
        serde_yaml::from_value::<BytesFromDeserializeByteBuf>(reference).is_err(),
        "serde_yaml rejects string values for deserialize_byte_buf"
    );
    assert!(
        yaml::from_value::<BytesFromDeserializeByteBuf>(value.clone()).is_err(),
        "yaml rejects string values for deserialize_byte_buf"
    );
    assert!(
        BytesFromDeserializeByteBuf::deserialize(&value).is_err(),
        "borrowed yaml Value rejects string values for deserialize_byte_buf"
    );
}

#[test]
fn serde_api_parser_backed_byte_deserialization_rejects_like_serde_yaml() {
    for input in ["abc\n", "\"abc\"\n"] {
        assert!(
            serde_yaml::from_str::<BytesFromDeserializeBytes>(input).is_err(),
            "serde_yaml rejects parser-backed deserialize_bytes for {input:?}"
        );
        assert!(
            serde_yaml::from_str::<BytesFromDeserializeByteBuf>(input).is_err(),
            "serde_yaml rejects parser-backed deserialize_byte_buf for {input:?}"
        );

        assert_parser_backed_deserialize_bytes_rejected(input);
        assert_parser_backed_deserialize_byte_buf_rejected(input);
    }

    let ours_vec: Vec<u8> = yaml::from_str("[0, 65, 255]\n").expect("yaml sequence to Vec<u8>");
    let reference_vec: Vec<u8> =
        serde_yaml::from_str("[0, 65, 255]\n").expect("serde_yaml sequence to Vec<u8>");
    assert_eq!(ours_vec, reference_vec);
}

#[test]
fn serde_api_explicit_binary_deserializes_to_byte_targets() {
    let input = "!!binary SGVsbG8=\n";
    let expected = b"Hello".to_vec();

    assert_eq!(
        yaml::from_str::<Vec<u8>>(input).expect("from_str explicit binary Vec<u8>"),
        expected
    );
    assert_eq!(
        yaml::from_slice::<Vec<u8>>(input.as_bytes()).expect("from_slice explicit binary Vec<u8>"),
        expected
    );
    assert_eq!(
        yaml::from_reader::<_, Vec<u8>>(Cursor::new(input.as_bytes()))
            .expect("from_reader explicit binary Vec<u8>"),
        expected
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_str(input))
            .expect("direct deserialize_bytes from str"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("direct deserialize_bytes from slice"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_reader(Cursor::new(
            input.as_bytes()
        )))
        .expect("direct deserialize_bytes from reader"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_str(input))
            .expect("direct deserialize_byte_buf from str"),
        BytesFromDeserializeByteBuf(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("direct deserialize_byte_buf from slice"),
        BytesFromDeserializeByteBuf(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_reader(Cursor::new(
            input.as_bytes()
        )))
        .expect("direct deserialize_byte_buf from reader"),
        BytesFromDeserializeByteBuf(expected.clone())
    );

    let node = yaml::parse_str(input).expect("explicit binary node");
    assert_eq!(
        yaml::from_node::<Vec<u8>>(&node).expect("node explicit binary Vec<u8>"),
        expected
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(&node).expect("node ref deserialize_bytes"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(node.clone())
            .expect("node owned deserialize_byte_buf"),
        BytesFromDeserializeByteBuf(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(node.clone()).expect("node owned deserialize_bytes"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(&node).expect("node ref deserialize_byte_buf"),
        BytesFromDeserializeByteBuf(expected.clone())
    );

    let value: Value = yaml::from_str(input).expect("explicit binary value");
    let tagged = value.as_tagged().expect("retained binary tag");
    assert_eq!(tagged.tag, Tag::new("!!binary"));
    assert_eq!(tagged.value.as_str(), Some("SGVsbG8="));
    assert_eq!(
        yaml::from_value::<Vec<u8>>(value.clone()).expect("value explicit binary Vec<u8>"),
        expected
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(&value).expect("value ref deserialize_bytes"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(value.clone())
            .expect("value owned deserialize_byte_buf"),
        BytesFromDeserializeByteBuf(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeBytes::deserialize(value.clone())
            .expect("value owned deserialize_bytes"),
        BytesFromDeserializeBytes(expected.clone())
    );
    assert_eq!(
        BytesFromDeserializeByteBuf::deserialize(&value).expect("value ref deserialize_byte_buf"),
        BytesFromDeserializeByteBuf(expected)
    );
}

#[test]
fn serde_api_invalid_explicit_binary_reports_scalar_span_for_byte_targets() {
    let input = "!!binary SGVsbG8?\n";
    let error = yaml::from_str::<Vec<u8>>(input).expect_err("invalid explicit binary");

    assert!(
        error
            .to_string()
            .contains("invalid explicit !!binary scalar")
    );
    assert_eq!(error.line(), Some(1));
    assert_eq!(error.column(), Some(10));

    let value: Value =
        yaml::from_str(input).expect("invalid binary remains loadable as tagged value");
    let tagged = value.as_tagged().expect("retained binary tag");
    assert_eq!(tagged.tag, Tag::new("!!binary"));
    assert_eq!(tagged.value.as_str(), Some("SGVsbG8?"));
    assert!(
        yaml::from_value::<Vec<u8>>(value).is_err(),
        "typed byte targets validate retained !!binary payloads"
    );
}

fn assert_parser_backed_deserialize_bytes_rejected(input: &str) {
    let from_reader: yaml::Result<BytesFromDeserializeBytes> =
        yaml::from_reader(Cursor::new(input.as_bytes()));
    let node = yaml::parse_str(input).expect("parse byte rejection input");

    for (label, result) in [
        (
            "from_str",
            yaml::from_str::<BytesFromDeserializeBytes>(input),
        ),
        (
            "from_slice",
            yaml::from_slice::<BytesFromDeserializeBytes>(input.as_bytes()),
        ),
        ("from_reader", from_reader),
        (
            "direct from_str",
            BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_str(input)),
        ),
        (
            "direct from_slice",
            BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_slice(
                input.as_bytes(),
            )),
        ),
        (
            "direct from_reader",
            BytesFromDeserializeBytes::deserialize(yaml::Deserializer::from_reader(Cursor::new(
                input.as_bytes(),
            ))),
        ),
        (
            "from_node",
            yaml::from_node::<BytesFromDeserializeBytes>(&node),
        ),
        ("node ref", BytesFromDeserializeBytes::deserialize(&node)),
        (
            "node owned",
            BytesFromDeserializeBytes::deserialize(node.clone()),
        ),
    ] {
        assert!(result.is_err(), "{label} rejects bytes for {input:?}");
    }
}

fn assert_parser_backed_deserialize_byte_buf_rejected(input: &str) {
    let from_reader: yaml::Result<BytesFromDeserializeByteBuf> =
        yaml::from_reader(Cursor::new(input.as_bytes()));
    let node = yaml::parse_str(input).expect("parse byte buffer rejection input");

    for (label, result) in [
        (
            "from_str",
            yaml::from_str::<BytesFromDeserializeByteBuf>(input),
        ),
        (
            "from_slice",
            yaml::from_slice::<BytesFromDeserializeByteBuf>(input.as_bytes()),
        ),
        ("from_reader", from_reader),
        (
            "direct from_str",
            BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_str(input)),
        ),
        (
            "direct from_slice",
            BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_slice(
                input.as_bytes(),
            )),
        ),
        (
            "direct from_reader",
            BytesFromDeserializeByteBuf::deserialize(yaml::Deserializer::from_reader(Cursor::new(
                input.as_bytes(),
            ))),
        ),
        (
            "from_node",
            yaml::from_node::<BytesFromDeserializeByteBuf>(&node),
        ),
        ("node ref", BytesFromDeserializeByteBuf::deserialize(&node)),
        (
            "node owned",
            BytesFromDeserializeByteBuf::deserialize(node.clone()),
        ),
    ] {
        assert!(result.is_err(), "{label} rejects byte_buf for {input:?}");
    }
}

#[test]
fn serde_api_to_value_bounds_hostile_sequence_and_tuple_length_hints() {
    for value in [
        yaml::to_value(HostileSequenceLengthHint).expect("hostile sequence hint"),
        yaml::to_value(HostileTupleLengthHint).expect("hostile tuple hint"),
        yaml::to_value(HostileTupleStructLengthHint).expect("hostile tuple struct hint"),
        HostileSequenceLengthHint
            .serialize(yaml::value::Serializer)
            .expect("direct value serializer hostile sequence hint"),
    ] {
        let sequence = value.as_sequence().expect("sequence value");
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str(), Some("x"));
    }
}

#[test]
fn serde_api_to_string_bounds_hostile_sequence_length_hint() {
    let emitted = yaml::to_string(&HostileSequenceLengthHint).expect("hostile sequence output");
    let value: Value = yaml::from_str(&emitted).expect("parse hostile sequence output");
    let sequence = value.as_sequence().expect("sequence output");
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str(), Some("x"));
}

#[test]
fn serde_api_writer_serializer_bounds_hostile_map_length_hint() {
    let mut buffer = Vec::new();
    let mut serializer = yaml::Serializer::new(&mut buffer);
    HostileMapLengthHint
        .serialize(&mut serializer)
        .expect("hostile map writer");
    let emitted = String::from_utf8(buffer).expect("utf8 writer output");
    let value: Value = yaml::from_str(&emitted).expect("parse hostile map output");
    assert_eq!(value["k"].as_str(), Some("v"));
}

#[test]
fn serde_api_value_serializer_bounds_tuple_variant_length_hint() {
    let value = HostileTupleVariantLengthHint
        .serialize(yaml::value::Serializer)
        .expect("hostile tuple variant hint");
    let tagged = value.as_tagged().expect("tuple variant tag");
    assert_eq!(tagged.tag, Tag::new("Tuple"));
    let items = tagged.value.as_sequence().expect("tuple variant sequence");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_str(), Some("x"));
}

#[test]
fn serde_api_mapping_key_serializer_bounds_nested_key_length_hint() {
    let value = yaml::to_value(HostileMappingKeyLengthHint).expect("hostile mapping key hint");
    let mapping = value.as_mapping().expect("mapping output");
    let [(key, value)] = mapping.as_slice() else {
        panic!("expected one mapping entry");
    };
    let sequence = key.as_sequence().expect("sequence key");
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str(), Some("x"));
    assert_eq!(value.as_str(), Some("v"));
}

#[test]
fn serde_api_struct_and_variant_serializers_bound_length_hints() {
    let fields = yaml::to_value(HostileStructLengthHint).expect("hostile struct hint");
    assert_eq!(fields["k"].as_str(), Some("v"));

    let variant = yaml::to_value(HostileStructVariantLengthHint).expect("hostile variant hint");
    let tagged = variant.as_tagged().expect("struct variant tag");
    assert_eq!(tagged.tag, Tag::new("Struct"));
    assert_eq!(tagged.value["k"].as_str(), Some("v"));
}

#[test]
fn serde_api_to_string_and_to_writer_serialize_common_config_shapes() {
    let config = SerializableConfig {
        name: "app".to_string(),
        ports: vec![80, 443],
        enabled: true,
        env: BTreeMap::from([("RUST_LOG".to_string(), "info".to_string())]),
        optional: None,
    };

    let emitted = yaml::to_string(&config).expect("yaml to_string");
    assert!(!emitted.starts_with("---\n"));
    let mut written = Vec::new();
    yaml::to_writer(&mut written, &config).expect("yaml to_writer");
    assert_eq!(String::from_utf8(written).expect("utf8 output"), emitted);

    let value: Value = yaml::from_str(&emitted).expect("parse emitted config");
    let reference = serde_yaml::to_value(&config).expect("serde_yaml to_value");
    assert_eq!(value["name"].as_str(), reference["name"].as_str());
    assert_eq!(value["ports"][0].as_u64(), reference["ports"][0].as_u64());
    assert_eq!(value["enabled"].as_bool(), reference["enabled"].as_bool());
    assert_eq!(value["env"]["RUST_LOG"].as_str(), Some("info"));
    assert!(value["optional"].is_null());
}

#[test]
fn serde_api_document_writers_reject_bytes_like_serde_yaml() {
    let bytes = SerializableBytes(b"\0A\xff");
    let reference = serde_yaml::to_string(&bytes).expect_err("serde_yaml rejects bytes");

    let error = yaml::to_string(&bytes).expect_err("yaml to_string rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());

    let mut written = Vec::new();
    let error = yaml::to_writer(&mut written, &bytes).expect_err("yaml to_writer rejects bytes");
    assert_eq!(error.to_string(), reference.to_string());
    assert!(written.is_empty());

    let nested = BTreeMap::from([("payload", bytes)]);
    let nested_error = yaml::to_string(&nested).expect_err("yaml rejects nested bytes");
    let nested_reference =
        serde_yaml::to_string(&nested).expect_err("serde_yaml rejects nested bytes");
    assert_eq!(nested_error.to_string(), nested_reference.to_string());

    let mut buffer = Vec::new();
    let mut serializer = yaml::Serializer::new(&mut buffer);
    let streaming_error = bytes
        .serialize(&mut serializer)
        .expect_err("yaml streaming serializer rejects bytes");
    assert_eq!(streaming_error.to_string(), reference.to_string());
    assert!(buffer.is_empty());

    let mut nested_buffer = Vec::new();
    let mut nested_serializer = yaml::Serializer::new(&mut nested_buffer);
    let nested_streaming_error = nested
        .serialize(&mut nested_serializer)
        .expect_err("yaml streaming serializer rejects nested bytes");
    assert_eq!(nested_streaming_error.to_string(), reference.to_string());
    assert!(nested_buffer.is_empty());
}

#[test]
fn serde_api_document_writers_serialize_top_level_values_once() {
    let calls = Cell::new(0);
    let value = OneShotScalar::new(&calls, "first", "later");
    let emitted = yaml::to_string(&value).expect("yaml to_string");
    assert_eq!(calls.get(), 1);

    let reference_calls = Cell::new(0);
    let reference = OneShotScalar::new(&reference_calls, "first", "later");
    assert_eq!(
        emitted,
        serde_yaml::to_string(&reference).expect("serde_yaml to_string")
    );
    assert_eq!(reference_calls.get(), 1);

    let writer_calls = Cell::new(0);
    let writer_value = OneShotScalar::new(&writer_calls, "first", "later");
    let mut written = Vec::new();
    yaml::to_writer(&mut written, &writer_value).expect("yaml to_writer");
    assert_eq!(writer_calls.get(), 1);
    assert_eq!(
        String::from_utf8(written).expect("utf8 writer output"),
        emitted
    );

    let streaming_calls = Cell::new(0);
    let streaming_value = OneShotScalar::new(&streaming_calls, "first", "later");
    let mut buffer = Vec::new();
    let mut serializer = yaml::Serializer::new(&mut buffer);
    streaming_value
        .serialize(&mut serializer)
        .expect("yaml streaming serializer");
    assert_eq!(streaming_calls.get(), 1);
    assert_eq!(
        String::from_utf8(buffer).expect("utf8 streaming output"),
        emitted
    );
}

#[test]
fn serde_api_document_writers_serialize_nested_map_entries_once() {
    let key_calls = Cell::new(0);
    let value_calls = Cell::new(0);
    let value = OneShotMap {
        key_calls: &key_calls,
        value_calls: &value_calls,
    };
    let emitted = yaml::to_string(&value).expect("yaml map");
    assert_eq!(key_calls.get(), 1);
    assert_eq!(value_calls.get(), 1);

    let reference_key_calls = Cell::new(0);
    let reference_value_calls = Cell::new(0);
    let reference = OneShotMap {
        key_calls: &reference_key_calls,
        value_calls: &reference_value_calls,
    };
    assert_eq!(
        emitted,
        serde_yaml::to_string(&reference).expect("serde_yaml map")
    );
    assert_eq!(reference_key_calls.get(), 1);
    assert_eq!(reference_value_calls.get(), 1);
}

#[test]
fn serde_api_collect_str_mapping_keys_format_display_once() {
    let value_key_calls = Cell::new(0);
    let value = yaml::to_value(OneShotCollectStrMap {
        key_calls: &value_key_calls,
    })
    .expect("yaml collect_str value");
    assert_eq!(value_key_calls.get(), 1);
    let tagged = value.as_tagged().expect("collect_str key becomes tag");
    assert_eq!(tagged.tag, Tag::new("!First"));
    assert_eq!(tagged.value.as_str(), Some("value"));

    let reference_key_calls = Cell::new(0);
    let reference = serde_yaml::to_value(OneShotCollectStrMap {
        key_calls: &reference_key_calls,
    })
    .expect("serde_yaml collect_str value");
    assert_eq!(reference_key_calls.get(), 1);
    let serde_yaml::Value::Tagged(reference_tagged) = reference else {
        panic!("serde_yaml collect_str key should become tag");
    };
    assert_eq!(reference_tagged.tag.to_string(), "!First");

    let string_key_calls = Cell::new(0);
    let emitted = yaml::to_string(&OneShotCollectStrMap {
        key_calls: &string_key_calls,
    })
    .expect("yaml collect_str string");
    assert_eq!(string_key_calls.get(), 1);
    assert!(emitted.contains("First"), "{emitted}");
    assert!(!emitted.contains("Second"), "{emitted}");

    let mut buffer = Vec::new();
    let streaming_key_calls = Cell::new(0);
    let mut serializer = yaml::Serializer::new(&mut buffer);
    OneShotCollectStrMap {
        key_calls: &streaming_key_calls,
    }
    .serialize(&mut serializer)
    .expect("yaml collect_str streaming writer");
    assert_eq!(streaming_key_calls.get(), 1);
    let streaming_output = String::from_utf8(buffer).expect("utf8 streaming output");
    assert!(streaming_output.contains("First"), "{streaming_output}");
    assert!(!streaming_output.contains("Second"), "{streaming_output}");
}

#[test]
fn serde_api_document_writers_serialize_struct_fields_once() {
    let value_calls = Cell::new(0);
    let value = OneShotStruct {
        value_calls: &value_calls,
    };
    let emitted = yaml::to_string(&value).expect("yaml struct");
    assert_eq!(value_calls.get(), 1);

    let reference_value_calls = Cell::new(0);
    let reference = OneShotStruct {
        value_calls: &reference_value_calls,
    };
    assert_eq!(
        emitted,
        serde_yaml::to_string(&reference).expect("serde_yaml struct")
    );
    assert_eq!(reference_value_calls.get(), 1);
}

#[test]
fn serde_api_document_writers_serialize_enum_variant_payloads_once() {
    let newtype_calls = Cell::new(0);
    let newtype = OneShotNewtypeVariant {
        value_calls: &newtype_calls,
    };
    let emitted_newtype = yaml::to_string(&newtype).expect("yaml newtype variant");
    assert_eq!(newtype_calls.get(), 1);
    let reference_newtype_calls = Cell::new(0);
    let reference_newtype = OneShotNewtypeVariant {
        value_calls: &reference_newtype_calls,
    };
    assert_eq!(
        emitted_newtype,
        serde_yaml::to_string(&reference_newtype).expect("serde_yaml newtype variant")
    );
    assert_eq!(reference_newtype_calls.get(), 1);

    let tuple_calls = Cell::new(0);
    let tuple = OneShotTupleVariant {
        value_calls: &tuple_calls,
    };
    let emitted_tuple = yaml::to_string(&tuple).expect("yaml tuple variant");
    assert_eq!(tuple_calls.get(), 1);
    assert!(emitted_tuple.contains("first_tuple"));
    assert!(!emitted_tuple.contains("later_tuple"));
    let reference_tuple_calls = Cell::new(0);
    let reference_tuple = OneShotTupleVariant {
        value_calls: &reference_tuple_calls,
    };
    let reference_tuple_output =
        serde_yaml::to_string(&reference_tuple).expect("serde_yaml tuple variant");
    assert!(reference_tuple_output.contains("first_tuple"));
    assert!(!reference_tuple_output.contains("later_tuple"));
    assert_eq!(reference_tuple_calls.get(), 1);

    let struct_calls = Cell::new(0);
    let struct_variant = OneShotStructVariant {
        value_calls: &struct_calls,
    };
    let emitted_struct = yaml::to_string(&struct_variant).expect("yaml struct variant");
    assert_eq!(struct_calls.get(), 1);
    assert!(emitted_struct.contains("first_struct_variant"));
    assert!(!emitted_struct.contains("later_struct_variant"));
    let reference_struct_calls = Cell::new(0);
    let reference_struct = OneShotStructVariant {
        value_calls: &reference_struct_calls,
    };
    let reference_struct_output =
        serde_yaml::to_string(&reference_struct).expect("serde_yaml struct variant");
    assert!(reference_struct_output.contains("first_struct_variant"));
    assert!(!reference_struct_output.contains("later_struct_variant"));
    assert_eq!(reference_struct_calls.get(), 1);
}

#[test]
fn serde_api_writer_document_markers_match_serde_yaml() {
    let scalar = "x";
    assert_eq!(
        yaml::to_string(&scalar).expect("yaml scalar"),
        serde_yaml::to_string(&scalar).expect("serde_yaml scalar")
    );

    let sequence = vec![1, 2];
    assert_eq!(
        yaml::to_string(&sequence).expect("yaml sequence"),
        serde_yaml::to_string(&sequence).expect("serde_yaml sequence")
    );

    let mapping = BTreeMap::from([("k".to_string(), 107_u16)]);
    assert_eq!(
        yaml::to_string(&mapping).expect("yaml mapping"),
        serde_yaml::to_string(&mapping).expect("serde_yaml mapping")
    );

    let tagged = SerializableAction::Newtype("deploy".to_string());
    assert_eq!(
        yaml::to_string(&tagged).expect("yaml tagged enum"),
        serde_yaml::to_string(&tagged).expect("serde_yaml tagged enum")
    );

    let mut written = Vec::new();
    yaml::to_writer(&mut written, &mapping).expect("yaml writer");
    assert_eq!(
        String::from_utf8(written).expect("utf8 output"),
        serde_yaml::to_string(&mapping).expect("serde_yaml mapping")
    );
}

#[test]
fn serde_api_writer_serializer_streams_multiple_documents() {
    let first = BTreeMap::from([("k".to_string(), 107_u16)]);
    let second = BTreeMap::from([("J".to_string(), 74_u16), ("k".to_string(), 107_u16)]);

    let mut buffer = Vec::new();
    {
        let mut serializer = yaml::Serializer::new(&mut buffer);
        first
            .serialize(&mut serializer)
            .expect("serialize first document");
        second
            .serialize(&mut serializer)
            .expect("serialize second document");
        serializer.flush().expect("flush serializer");
    }

    let output = String::from_utf8(buffer).expect("utf8 output");
    let mut reference_buffer = Vec::new();
    {
        let mut serializer = serde_yaml::Serializer::new(&mut reference_buffer);
        first
            .serialize(&mut serializer)
            .expect("serialize reference first document");
        second
            .serialize(&mut serializer)
            .expect("serialize reference second document");
    }
    assert_eq!(
        output,
        String::from_utf8(reference_buffer).expect("serde_yaml utf8 output")
    );
    assert_eq!(output, "k: 107\n---\nJ: 74\nk: 107\n");

    let docs: Vec<Value> = yaml::from_documents_str(&output).expect("parse serialized stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["k"].as_u64(), Some(107));
    assert_eq!(docs[1]["J"].as_u64(), Some(74));
    assert_eq!(docs[1]["k"].as_u64(), Some(107));
}

#[test]
fn serde_api_writer_serializer_into_inner_returns_writer() {
    let config = SerializableConfig {
        name: "app".to_string(),
        ports: vec![8080],
        enabled: false,
        env: BTreeMap::new(),
        optional: Some("canary".to_string()),
    };

    let mut serializer = yaml::Serializer::new(Cursor::new(Vec::new()));
    config
        .serialize(&mut serializer)
        .expect("serialize document into cursor");
    let cursor = serializer.into_inner().expect("into_inner");
    let output = String::from_utf8(cursor.into_inner()).expect("utf8 output");
    let value: Value = yaml::from_str(&output).expect("parse serialized document");

    assert_eq!(value["name"].as_str(), Some("app"));
    assert_eq!(value["ports"][0].as_u64(), Some(8080));
    assert_eq!(value["enabled"].as_bool(), Some(false));
    assert_eq!(value["optional"].as_str(), Some("canary"));
}

#[test]
fn serde_api_with_singleton_map_serializes_enum_fields_like_serde_yaml() {
    let config = SerializableSingletonMapConfig {
        action: SerializableAction::Shell {
            run: "cargo test".to_string(),
        },
    };
    let reference_config = ReferenceSerializableSingletonMapConfig {
        action: SerializableAction::Shell {
            run: "cargo test".to_string(),
        },
    };

    let value = yaml::to_value(&config).expect("yaml singleton serialize");
    let reference =
        serde_yaml::to_value(&reference_config).expect("serde_yaml singleton serialize");
    assert_eq!(
        value["action"]["Shell"]["run"].as_str(),
        reference["action"]["Shell"]["run"].as_str()
    );
    assert_eq!(value["action"]["Shell"]["run"].as_str(), Some("cargo test"));
    assert!(value["action"].as_tagged().is_none());

    let emitted = yaml::to_string(&config).expect("emit singleton map");
    let reparsed: Value = yaml::from_str(&emitted).expect("parse singleton map");
    assert_eq!(
        reparsed["action"]["Shell"]["run"].as_str(),
        Some("cargo test")
    );
}

#[test]
fn serde_api_with_singleton_map_recursive_serializes_nested_enum_fields_like_serde_yaml() {
    let actions = SerializableActions {
        primary: SerializableAction::Shell {
            run: "cargo test".to_string(),
        },
        steps: vec![
            SerializableAction::Unit,
            SerializableAction::Newtype("deploy".to_string()),
            SerializableAction::Tuple(4, 2),
        ],
        by_name: BTreeMap::from([(
            "release".to_string(),
            SerializableAction::Newtype("ship".to_string()),
        )]),
    };
    let config = SerializableRecursiveSingletonMapConfig { actions };
    let reference_config = ReferenceSerializableRecursiveSingletonMapConfig {
        actions: SerializableActions {
            primary: SerializableAction::Shell {
                run: "cargo test".to_string(),
            },
            steps: vec![
                SerializableAction::Unit,
                SerializableAction::Newtype("deploy".to_string()),
                SerializableAction::Tuple(4, 2),
            ],
            by_name: BTreeMap::from([(
                "release".to_string(),
                SerializableAction::Newtype("ship".to_string()),
            )]),
        },
    };

    let value = yaml::to_value(&config).expect("yaml recursive singleton serialize");
    let reference =
        serde_yaml::to_value(&reference_config).expect("serde_yaml recursive singleton serialize");
    assert_eq!(
        value["actions"]["primary"]["Shell"]["run"].as_str(),
        reference["actions"]["primary"]["Shell"]["run"].as_str()
    );
    assert_eq!(
        value["actions"]["steps"][1]["Newtype"].as_str(),
        reference["actions"]["steps"][1]["Newtype"].as_str()
    );
    assert_eq!(value["actions"]["steps"][2]["Tuple"][0].as_u64(), Some(4));
    assert_eq!(
        value["actions"]["by_name"]["release"]["Newtype"].as_str(),
        Some("ship")
    );
}

#[test]
fn serde_api_with_singleton_map_deserializes_enum_fields() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct SingletonMapConfig {
        #[serde(with = "yaml::with::singleton_map")]
        unit: SingletonAction,
        #[serde(with = "yaml::with::singleton_map")]
        newtype: SingletonAction,
        #[serde(with = "yaml::with::singleton_map")]
        tuple: SingletonAction,
        #[serde(with = "yaml::with::singleton_map")]
        shell: SingletonAction,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReferenceSingletonMapConfig {
        #[serde(with = "serde_yaml::with::singleton_map")]
        unit: SingletonAction,
        #[serde(with = "serde_yaml::with::singleton_map")]
        newtype: SingletonAction,
        #[serde(with = "serde_yaml::with::singleton_map")]
        tuple: SingletonAction,
        #[serde(with = "serde_yaml::with::singleton_map")]
        shell: SingletonAction,
    }

    let input = "\
unit: Unit
newtype:
  Newtype: deploy
tuple:
  Tuple: [4, 2]
shell:
  Shell:
    run: cargo test
";

    let parsed: SingletonMapConfig = yaml::from_str(input).expect("yaml singleton_map helper");
    let reference: ReferenceSingletonMapConfig =
        serde_yaml::from_str(input).expect("serde_yaml singleton_map helper");

    assert_eq!(parsed.unit, reference.unit);
    assert_eq!(parsed.newtype, reference.newtype);
    assert_eq!(parsed.tuple, reference.tuple);
    assert_eq!(parsed.shell, reference.shell);
    assert_eq!(parsed.unit, SingletonAction::Unit);
    assert_eq!(
        parsed.shell,
        SingletonAction::Shell {
            run: "cargo test".to_string()
        }
    );
}

#[test]
fn serde_api_with_singleton_map_recursive_deserializes_nested_enum_fields() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct RecursiveSingletonMapConfig {
        #[serde(with = "yaml::with::singleton_map_recursive")]
        actions: RecursiveActions,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ReferenceRecursiveSingletonMapConfig {
        #[serde(with = "serde_yaml::with::singleton_map_recursive")]
        actions: RecursiveActions,
    }

    let input = "\
actions:
  primary:
    Shell:
      run: cargo test
  steps:
    - Unit
    - Newtype: deploy
    - Tuple: [4, 2]
  by_name:
    smoke:
      Shell:
        run: cargo test --test serde_value_api
    release:
      Newtype: ship
";

    let parsed: RecursiveSingletonMapConfig =
        yaml::from_str(input).expect("yaml recursive singleton_map helper");
    let reference: ReferenceRecursiveSingletonMapConfig =
        serde_yaml::from_str(input).expect("serde_yaml recursive singleton_map helper");
    let direct: RecursiveActions = yaml::with::singleton_map_recursive::deserialize(
        yaml::Deserializer::from_str(&input["actions:\n".len()..]),
    )
    .expect("direct recursive singleton_map helper");

    assert_eq!(parsed.actions, reference.actions);
    assert_eq!(parsed.actions, direct);
    assert_eq!(
        parsed.actions.primary,
        SingletonAction::Shell {
            run: "cargo test".to_string()
        }
    );
    assert_eq!(
        parsed.actions.steps[1],
        SingletonAction::Newtype("deploy".to_string())
    );
    assert_eq!(
        parsed.actions.by_name["release"],
        SingletonAction::Newtype("ship".to_string())
    );
}

#[test]
fn serde_api_typed_config_reads_alias_expanded_values() {
    let input = "x-service-defaults: &service_defaults\n  image: nginx\n  environment:\n    RUST_LOG: info\nservices:\n  web: *service_defaults\n";
    let compose: AliasCompose = yaml::from_str(input).expect("deserialize alias compose");
    assert_eq!(compose.services["web"].image, "nginx");
    assert_eq!(compose.services["web"].environment["RUST_LOG"], "info");
}

#[test]
fn serde_api_value_collections_are_spanless() {
    let value: Value = yaml::from_str("name: app\nports: [80, 443]\n").expect("value");
    let Value::Mapping(mapping) = &value else {
        panic!("expected mapping");
    };
    assert_eq!(mapping.get("name").and_then(Value::as_str), Some("app"));
    assert_eq!(
        mapping
            .get("ports")
            .and_then(Value::as_sequence)
            .and_then(|items| items.first())
            .and_then(Value::as_u64),
        Some(80)
    );

    let constructed_sequence: Sequence = vec![Value::String("one".to_string()), Value::Bool(true)];
    let constructed = Value::Sequence(constructed_sequence);
    assert_eq!(constructed[0].as_str(), Some("one"));
    assert_eq!(constructed[1].as_bool(), Some(true));

    let mut constructed_mapping = Mapping::new();
    constructed_mapping.insert(
        Value::String("name".to_string()),
        Value::String("manual".to_string()),
    );
    let constructed = Value::Mapping(constructed_mapping);
    assert_eq!(
        constructed.get("name").and_then(Value::as_str),
        Some("manual")
    );
    assert_eq!(
        constructed[Value::String("name".to_string())].as_str(),
        Some("manual")
    );
}

#[test]
fn serde_api_value_public_traits_match_serde_yaml_adoption_surface() {
    assert_value_traits::<Value>();
    assert_value_traits::<Mapping>();
    assert_tag_traits::<Tag>();
    assert_tagged_traits::<TaggedValue>();

    assert!(Value::String("thing".to_string()) == "thing");
    let owned_thing = "thing".to_string();
    assert!(Value::String("thing".to_string()) == owned_thing);
    assert!(Value::Bool(true) == true);
    assert!(Value::Number(Number::Integer(-7)) == -7i64);
    assert!(Value::Number(Number::Unsigned(7)) == 7u64);
    assert!(Value::Number(Number::Float(2.5)) == 2.5f64);

    let first_nan = Value::Number(Number::Float(f64::from_bits(0x7ff8_0000_0000_0001)));
    let second_nan = Value::Number(Number::Float(f64::from_bits(0x7ff8_0000_0000_0002)));
    assert_eq!(first_nan, second_nan);
    assert_eq!(hash_of(&first_nan), hash_of(&second_nan));

    let positive_zero = Value::Number(Number::Float(0.0));
    let negative_zero = Value::Number(Number::Float(-0.0));
    assert_eq!(positive_zero, negative_zero);
    assert_eq!(hash_of(&positive_zero), hash_of(&negative_zero));
    assert_eq!(
        positive_zero.partial_cmp(&negative_zero),
        Some(Ordering::Equal)
    );

    let mut left = Mapping::new();
    left.insert("a".into(), 1u64.into());
    left.insert("b".into(), "two".into());
    let mut right = Mapping::new();
    right.insert("b".into(), "two".into());
    right.insert("a".into(), 1u64.into());
    assert_eq!(left, right);
    assert_eq!(hash_of(&left), hash_of(&right));
    assert_eq!(left.partial_cmp(&right), Some(Ordering::Equal));

    let left_value = Value::Mapping(left.clone());
    let right_value = Value::Mapping(right.clone());
    assert_eq!(left_value, right_value);
    assert_eq!(hash_of(&left_value), hash_of(&right_value));
    assert_eq!(left_value.partial_cmp(&right_value), Some(Ordering::Equal));

    let mut values = HashSet::new();
    values.insert(left_value);
    assert!(values.contains(&right_value));

    let tag = Tag::new("Thing");
    assert_eq!(tag, "Thing");
    assert_eq!(tag, "!Thing");
    assert_eq!(hash_of(&tag), hash_of(&Tag::new("!Thing")));

    let verbatim = Tag::new("!<tag:example.com,2026:Thing>");
    assert_eq!(verbatim, "!<tag:example.com,2026:Thing>");
    assert_eq!(verbatim, "<tag:example.com,2026:Thing>");

    let mut tags = [Tag::new("beta"), Tag::new("alpha")];
    tags.sort();
    assert_eq!(tags[0], "alpha");
    assert_eq!(tags[1], "beta");

    let tagged = TaggedValue {
        tag: Tag::new("Thing"),
        value: Value::String("value".to_string()),
    };
    assert_eq!(tagged.partial_cmp(&tagged), Some(Ordering::Equal));
    let mut tagged_values = HashSet::new();
    tagged_values.insert(Value::Tagged(Box::new(tagged.clone())));
    assert!(tagged_values.contains(&Value::Tagged(Box::new(tagged))));

    let mut reference_left = serde_yaml::Mapping::new();
    reference_left.insert("a".into(), 1u64.into());
    reference_left.insert("b".into(), "two".into());
    let mut reference_right = serde_yaml::Mapping::new();
    reference_right.insert("b".into(), "two".into());
    reference_right.insert("a".into(), 1u64.into());
    assert_eq!(reference_left, reference_right);
    assert_eq!(
        reference_left.partial_cmp(&reference_right),
        Some(Ordering::Equal)
    );
}

#[test]
fn serde_api_nonnegative_signed_unsigned_number_identity_matches_serde_yaml() {
    let signed = Number::from(1i64);
    let unsigned = Number::from(1u64);
    assert_eq!(signed, unsigned);
    assert_eq!(hash_of(&signed), hash_of(&unsigned));
    assert_eq!(signed.partial_cmp(&unsigned), Some(Ordering::Equal));

    let signed_value = Value::from(1i64);
    let unsigned_value = Value::from(1u64);
    assert_eq!(signed_value, unsigned_value);
    assert_eq!(hash_of(&signed_value), hash_of(&unsigned_value));
    assert_eq!(
        signed_value.partial_cmp(&unsigned_value),
        Some(Ordering::Equal)
    );
    assert_ne!(Value::from(-1i64), Value::from(1u64));
    assert_ne!(Value::from("1"), signed_value);

    let mut mapping = Mapping::new();
    assert_eq!(
        mapping.insert(signed_value.clone(), Value::from("signed")),
        None
    );
    assert_eq!(
        mapping.insert(unsigned_value.clone(), Value::from("unsigned")),
        Some(Value::from("signed"))
    );
    assert_eq!(mapping.len(), 1);
    assert_eq!(
        mapping.get(signed_value).and_then(Value::as_str),
        Some("unsigned")
    );
    assert_eq!(
        mapping
            .entry(unsigned_value)
            .or_insert(Value::from("unreachable"))
            .as_str(),
        Some("unsigned")
    );

    let mut reference = serde_yaml::Mapping::new();
    assert_eq!(reference.insert(1i64.into(), "signed".into()), None);
    assert_eq!(
        reference.insert(1u64.into(), "unsigned".into()),
        Some("signed".into())
    );
    assert_eq!(reference.len(), mapping.len());
}

#[test]
fn serde_api_value_mutation_and_defaults_match_read_side_expectations() {
    let mut value: Value = yaml::from_str("items: [one]\n").expect("value");
    value
        .get_mut("items")
        .and_then(Value::as_sequence_mut)
        .expect("items sequence")
        .push(Value::String("two".to_string()));
    assert_eq!(value["items"][1].as_str(), Some("two"));

    let mut mapping = Mapping::new();
    assert!(mapping.is_empty());
    assert_eq!(
        mapping.insert(Value::String("enabled".into()), Value::Bool(true)),
        None
    );
    assert!(mapping.contains_key("enabled"));
    assert_eq!(
        mapping.get_mut("enabled").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(mapping.len(), 1);

    let defaulted = Value::default();
    assert_eq!(defaulted.as_null(), Some(()));
}

#[test]
fn serde_api_value_index_mut_matches_serde_yaml_for_config_patching() {
    let mut value: Value = yaml::from_str("{x: 0}\n").expect("value");
    let mut reference: serde_yaml::Value = serde_yaml::from_str("{x: 0}\n").expect("reference");

    value["x"] = Value::Number(Number::Unsigned(1));
    reference["x"] = serde_yaml::from_str("1").expect("reference number");

    value["y"] = Value::Sequence(vec![Value::Bool(false), Value::Bool(false)]);
    reference["y"] = serde_yaml::from_str("[false, false]").expect("reference sequence");
    value["y"][0] = Value::Bool(true);
    reference["y"][0] = serde_yaml::from_str("true").expect("reference bool");

    value["a"]["b"]["c"]["d"] = Value::String("inserted".to_string());
    reference["a"]["b"]["c"]["d"] = serde_yaml::Value::String("inserted".to_string());

    assert_eq!(value["x"].as_u64(), reference["x"].as_u64());
    assert_eq!(value["y"][0].as_bool(), reference["y"][0].as_bool());
    assert_eq!(value["y"][1].as_bool(), reference["y"][1].as_bool());
    assert_eq!(
        value["a"]["b"]["c"]["d"].as_str(),
        reference["a"]["b"]["c"]["d"].as_str()
    );
    assert!(value["missing"]["nested"].is_null());
}

#[test]
fn serde_api_mapping_entry_matches_serde_yaml_normalization_helpers() {
    let mut mapping = Mapping::new();
    let mut reference = serde_yaml::Mapping::new();

    *mapping
        .entry(Value::String("image".to_string()))
        .or_insert(Value::String("nginx".to_string())) =
        Value::String("ghcr.io/example/app:latest".to_string());
    *reference
        .entry(serde_yaml::Value::String("image".to_string()))
        .or_insert(serde_yaml::Value::String("nginx".to_string())) =
        serde_yaml::Value::String("ghcr.io/example/app:latest".to_string());

    mapping
        .entry(Value::String("replicas".to_string()))
        .and_modify(|value| *value = Value::Number(Number::Unsigned(3)))
        .or_insert_with(|| Value::Number(Number::Unsigned(1)));
    reference
        .entry(serde_yaml::Value::String("replicas".to_string()))
        .and_modify(|value| *value = serde_yaml::from_str("3").expect("reference replicas"))
        .or_insert_with(|| serde_yaml::from_str("1").expect("reference default"));

    match mapping.entry(Value::String("ports".to_string())) {
        yaml::mapping::Entry::Vacant(entry) => {
            assert_eq!(entry.key().as_str(), Some("ports"));
            entry.insert(Value::Sequence(vec![Value::Number(Number::Unsigned(8080))]));
        }
        yaml::mapping::Entry::Occupied(_) => panic!("ports should start vacant"),
    }

    match mapping.entry(Value::String("image".to_string())) {
        yaml::mapping::Entry::Occupied(mut entry) => {
            assert_eq!(entry.key().as_str(), Some("image"));
            assert_eq!(entry.get().as_str(), Some("ghcr.io/example/app:latest"));
            assert_eq!(
                entry
                    .insert(Value::String("ghcr.io/example/app:v2".to_string()))
                    .as_str(),
                Some("ghcr.io/example/app:latest")
            );
        }
        yaml::mapping::Entry::Vacant(_) => panic!("image should be occupied"),
    }

    assert_eq!(mapping["image"].as_str(), Some("ghcr.io/example/app:v2"));
    assert_eq!(
        reference["image"].as_str(),
        Some("ghcr.io/example/app:latest")
    );
    assert_eq!(mapping["replicas"].as_u64(), reference["replicas"].as_u64());
    assert_eq!(mapping["ports"][0].as_u64(), Some(8080));
}

#[test]
fn serde_api_value_usize_index_reads_numeric_mapping_keys() {
    let input = "0: zero\n42: answer\nitems: [first]\n";
    let mut value: Value = yaml::from_str(input).expect("value");
    let mut reference: serde_yaml::Value = serde_yaml::from_str(input).expect("reference");

    assert_eq!(value[0].as_str(), reference[0].as_str());
    assert_eq!(value[42].as_str(), reference[42].as_str());
    assert_eq!(value["items"][0].as_str(), reference["items"][0].as_str());

    value[42] = Value::String("patched".to_string());
    reference[42] = serde_yaml::Value::String("patched".to_string());
    assert_eq!(value[42].as_str(), reference[42].as_str());

    let mut mapping: Mapping = yaml::from_str("42: answer\n").expect("mapping");
    let key = Value::from(42);
    assert_eq!(mapping.get(&key).and_then(Value::as_str), Some("answer"));
    mapping.insert(key.clone(), Value::String("patched".to_string()));
    assert_eq!(mapping.get(&key).and_then(Value::as_str), Some("patched"));
}

#[test]
fn serde_api_mapping_matches_common_serde_yaml_read_helpers() {
    let input = "a: 1\nb: two\nc: true\n";
    let mut mapping: Mapping = yaml::from_str(input).expect("deserialize yaml::Mapping");
    let reference: serde_yaml::Mapping =
        serde_yaml::from_str(input).expect("deserialize serde_yaml::Mapping");
    assert_eq!(mapping.len(), reference.len());
    assert_eq!(mapping["a"].as_u64(), Some(1));
    assert_eq!(mapping["b"].as_str(), Some("two"));

    mapping["b"] = Value::String("changed".to_string());
    assert_eq!(mapping.get("b").and_then(Value::as_str), Some("changed"));

    mapping.reserve(4);
    assert!(mapping.capacity() >= mapping.len());
    mapping.shrink_to_fit();
    assert!(mapping.capacity() >= mapping.len());

    let borrowed_pairs = (&mapping).into_iter().count();
    assert_eq!(borrowed_pairs, 3);

    for (_, value) in &mut mapping {
        if value.as_bool() == Some(true) {
            *value = Value::String("retained".to_string());
        }
    }
    assert_eq!(mapping["c"].as_str(), Some("retained"));

    mapping.extend([
        (
            Value::String("d".to_string()),
            Value::Number(Number::Unsigned(4)),
        ),
        (
            Value::String("a".to_string()),
            Value::String("replaced".to_string()),
        ),
    ]);
    assert_eq!(mapping["a"].as_str(), Some("replaced"));
    assert_eq!(mapping["d"].as_u64(), Some(4));

    mapping.retain(|key, _| key.as_str() != Some("c"));
    assert!(!mapping.contains_key("c"));

    assert_eq!(
        mapping.remove("d").and_then(|value| value.as_u64()),
        Some(4)
    );
    let removed = mapping
        .remove_entry("a")
        .expect("remove existing entry by string key");
    assert_eq!(removed.0.as_str(), Some("a"));
    assert_eq!(removed.1.as_str(), Some("replaced"));

    mapping.clear();
    mapping.insert(
        Value::String("x".to_string()),
        Value::String("ex".to_string()),
    );
    mapping.insert(
        Value::String("y".to_string()),
        Value::String("why".to_string()),
    );
    let keys = mapping
        .clone()
        .into_keys()
        .filter_map(|key| key.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let values = mapping
        .into_values()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    assert_eq!(keys, ["x", "y"]);
    assert_eq!(values, ["ex", "why"]);
}

#[test]
fn serde_api_mapping_iterators_match_public_serde_yaml_surface() {
    fn assert_value_index<T: ?Sized + yaml::Index>() {}
    fn assert_mapping_index<T: ?Sized + yaml::mapping::Index>() {}

    assert_value_index::<usize>();
    assert_value_index::<str>();
    assert_value_index::<String>();
    assert_value_index::<Value>();
    assert_mapping_index::<str>();
    assert_mapping_index::<String>();
    assert_mapping_index::<Value>();

    let input = "a: 1\nb: two\nc: true\n";
    let mut mapping: Mapping = yaml::from_str(input).expect("deserialize mapping");

    let iter: yaml::mapping::Iter<'_> = mapping.iter();
    assert_eq!(iter.len(), 3);
    assert_eq!(iter.size_hint(), (3, Some(3)));

    let mut iter = mapping.iter();
    assert_eq!(iter.next().and_then(|(key, _)| key.as_str()), Some("a"));
    assert_eq!(
        iter.next_back().and_then(|(key, _)| key.as_str()),
        Some("c")
    );
    assert_eq!(iter.len(), 1);

    let keys: yaml::mapping::Keys<'_> = mapping.keys();
    assert_eq!(keys.len(), 3);
    assert_eq!(
        keys.collect::<Vec<_>>()
            .into_iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );

    let values: yaml::mapping::Values<'_> = mapping.values();
    assert_eq!(values.len(), 3);
    assert_eq!(values.filter(|value| value.is_number()).count(), 1);

    {
        let by_ref: yaml::mapping::Iter<'_> = (&mapping).into_iter();
        assert_eq!(by_ref.len(), 3);
    }

    {
        let mut iter_mut: yaml::mapping::IterMut<'_> = mapping.iter_mut();
        assert_eq!(iter_mut.len(), 3);
        let (_, value) = iter_mut.next_back().expect("last mutable pair");
        *value = Value::String("changed".to_string());
        assert_eq!(iter_mut.len(), 2);
    }
    assert_eq!(mapping["c"].as_str(), Some("changed"));

    {
        let mut values_mut: yaml::mapping::ValuesMut<'_> = mapping.values_mut();
        assert_eq!(values_mut.len(), 3);
        *values_mut.next().expect("first mutable value") = Value::Number(Number::Unsigned(10));
        assert_eq!(values_mut.len(), 2);
    }
    assert_eq!(mapping["a"].as_u64(), Some(10));

    {
        let by_mut: yaml::mapping::IterMut<'_> = (&mut mapping).into_iter();
        assert_eq!(by_mut.len(), 3);
    }

    let into_iter: yaml::mapping::IntoIter = mapping.clone().into_iter();
    assert_eq!(into_iter.len(), 3);
    assert_eq!(
        into_iter
            .map(|(key, _)| key.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .expect("string keys"),
        ["a", "b", "c"]
    );

    let mut into_keys: yaml::mapping::IntoKeys = mapping.clone().into_keys();
    assert_eq!(into_keys.len(), 3);
    assert_eq!(
        into_keys
            .next()
            .and_then(|key| key.as_str().map(str::to_string)),
        Some("a".to_string())
    );
    assert_eq!(
        into_keys
            .next_back()
            .and_then(|key| key.as_str().map(str::to_string)),
        Some("c".to_string())
    );
    assert_eq!(into_keys.len(), 1);

    let mut into_values: yaml::mapping::IntoValues = mapping.into_values();
    assert_eq!(into_values.len(), 3);
    assert_eq!(
        into_values.next().and_then(|value| value.as_u64()),
        Some(10)
    );
    assert_eq!(
        into_values
            .next_back()
            .and_then(|value| value.as_str().map(str::to_string)),
        Some("changed".to_string())
    );
    assert_eq!(into_values.len(), 1);
}

#[test]
fn serde_api_mapping_rejects_duplicate_keys_and_non_mappings() {
    let duplicate = yaml::from_str::<Mapping>("a: 1\na: 2\n")
        .expect_err("duplicate mapping keys should be rejected");
    assert!(duplicate.to_string().contains("duplicate"));

    let null = yaml::from_str::<Mapping>("null\n").expect_err("null is not a mapping");
    assert!(null.to_string().contains("expected"));
}

#[test]
fn serde_api_value_can_drive_deserialize_directly() {
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::String("name".to_string()),
        Value::String("direct".to_string()),
    );
    mapping.insert(
        Value::String("ports".to_string()),
        Value::Sequence(vec![Value::Number(Number::Unsigned(8080))]),
    );
    mapping.insert(Value::String("enabled".to_string()), Value::Bool(true));

    let config = Config::deserialize(Value::Mapping(mapping)).expect("deserialize from Value");
    assert_eq!(
        config,
        Config {
            name: "direct".to_string(),
            ports: vec![8080],
            enabled: true,
        }
    );
}

#[test]
fn serde_api_value_null_deserializes_as_empty_collections_like_serde_yaml() {
    let reference_vec: Vec<String> =
        serde_yaml::from_value(serde_yaml::Value::Null).expect("serde_yaml null to vec");
    let ours_vec: Vec<String> = yaml::from_value(Value::Null).expect("null to vec");
    assert_eq!(ours_vec, reference_vec);

    let reference_map: BTreeMap<String, String> =
        serde_yaml::from_value(serde_yaml::Value::Null).expect("serde_yaml null to map");
    let ours_map: BTreeMap<String, String> = yaml::from_value(Value::Null).expect("null to map");
    assert_eq!(ours_map, reference_map);

    let reference_struct: DefaultedCollections = serde_yaml::from_value(serde_yaml::Value::Null)
        .expect("serde_yaml null to defaulted struct");
    let ours_struct: DefaultedCollections =
        yaml::from_value(Value::Null).expect("null to defaulted struct");
    assert_eq!(ours_struct, reference_struct);

    let owned_vec = Vec::<String>::deserialize(Value::Null).expect("owned value null to vec");
    assert_eq!(owned_vec, reference_vec);
    let owned_map =
        BTreeMap::<String, String>::deserialize(Value::Null).expect("owned value null to map");
    assert_eq!(owned_map, reference_map);
    let owned_struct =
        DefaultedCollections::deserialize(Value::Null).expect("owned value null to struct");
    assert_eq!(owned_struct, reference_struct);

    let value = Value::Null;
    let borrowed_vec = Vec::<String>::deserialize(&value).expect("value ref null to vec");
    assert_eq!(borrowed_vec, reference_vec);
    let borrowed_map =
        BTreeMap::<String, String>::deserialize(&value).expect("value ref null to map");
    assert_eq!(borrowed_map, reference_map);
    let borrowed_struct =
        DefaultedCollections::deserialize(&value).expect("value ref null to struct");
    assert_eq!(borrowed_struct, reference_struct);

    let direct_null = yaml::from_str::<Vec<String>>("null\n")
        .expect_err("parser-backed null is still not a sequence");
    assert!(direct_null.to_string().contains("expected"));
}

#[test]
fn serde_api_parser_backed_empty_nodes_deserialize_as_empty_collections_like_serde_yaml() {
    for input in ["", "---\n"] {
        let reference_vec: Vec<String> = serde_yaml::from_str(input).expect("serde_yaml vec");
        let ours_vec: Vec<String> = yaml::from_str(input).expect("from_str vec");
        let ours_slice: Vec<String> = yaml::from_slice(input.as_bytes()).expect("from_slice vec");
        let node = yaml::parse_str(input).expect("node");
        let ours_node: Vec<String> = yaml::from_node(&node).expect("from_node vec");
        let ours_document: Vec<String> =
            Vec::deserialize(yaml::Deserializer::from_str(input)).expect("document vec");
        assert_eq!(ours_vec, reference_vec);
        assert_eq!(ours_slice, reference_vec);
        assert_eq!(ours_node, reference_vec);
        assert_eq!(ours_document, reference_vec);

        let reference_map: BTreeMap<String, String> =
            serde_yaml::from_str(input).expect("serde_yaml map");
        let ours_map: BTreeMap<String, String> = yaml::from_str(input).expect("from_str map");
        let ours_node_map: BTreeMap<String, String> =
            yaml::from_node(&node).expect("from_node map");
        assert_eq!(ours_map, reference_map);
        assert_eq!(ours_node_map, reference_map);

        let reference_struct: DefaultedCollections =
            serde_yaml::from_str(input).expect("serde_yaml struct");
        let ours_struct: DefaultedCollections = yaml::from_str(input).expect("from_str struct");
        let ours_node_struct: DefaultedCollections =
            yaml::from_node(&node).expect("from_node struct");
        assert_eq!(ours_struct, reference_struct);
        assert_eq!(ours_node_struct, reference_struct);
    }

    let empty_fields = "ports:\nlabels:\n";
    let reference: DefaultedCollections =
        serde_yaml::from_str(empty_fields).expect("serde_yaml empty field collections");
    let ours: DefaultedCollections = yaml::from_str(empty_fields).expect("empty field collections");
    assert_eq!(ours, reference);

    let stream = "---\n---\nports: [80]\nlabels:\n  env: prod\n";
    let ours_docs = yaml::Deserializer::from_str(stream)
        .map(DefaultedCollections::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("stream structs");
    let reference_docs = serde_yaml::Deserializer::from_str(stream)
        .map(DefaultedCollections::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml stream structs");
    assert_eq!(ours_docs, reference_docs);
    assert_eq!(ours_docs[0], DefaultedCollections::default());
    assert_eq!(ours_docs[1].ports, [80]);
    assert_eq!(ours_docs[1].labels["env"], "prod");

    for explicit_null in ["null\n", "~\n"] {
        let error = yaml::from_str::<Vec<String>>(explicit_null)
            .expect_err("explicit null is not a parser-backed sequence");
        assert!(error.to_string().contains("expected"));
    }
    let explicit_null_field = yaml::from_str::<DefaultedCollections>("ports: null\nlabels:\n")
        .expect_err("explicit null field is not a sequence");
    assert!(explicit_null_field.to_string().contains("expected"));
}

#[test]
fn serde_api_multi_doc_values_can_be_read() {
    let docs: Vec<Value> =
        yaml::from_documents_str("---\nname: one\n---\nname: two\n").expect("docs");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["name"].as_str(), Some("one"));
    assert_eq!(docs[1]["name"].as_str(), Some("two"));
}

#[test]
fn serde_api_explicit_empty_documents_are_null() {
    let input = "---\n---\nname: second\n";
    let docs: Vec<Value> = yaml::from_documents_str(input).expect("docs");
    assert_eq!(docs.len(), 2);
    assert!(docs[0].is_null());
    assert_eq!(docs[1]["name"].as_str(), Some("second"));

    let values = yaml::Deserializer::from_slice(input.as_bytes())
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("stream values");
    assert_eq!(values, docs);
}

#[test]
fn serde_api_empty_stream_iterator_matches_serde_yaml_null_document() {
    let values = yaml::Deserializer::from_str("")
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("yaml empty stream values");
    let reference = serde_yaml::Deserializer::from_str("")
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml empty stream values");

    assert_eq!(values.len(), reference.len());
    assert_eq!(values.len(), 1);
    assert!(values[0].is_null());
    assert!(reference[0].is_null());

    let from_slice = yaml::Deserializer::from_slice(b"")
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("yaml empty slice stream values");
    assert_eq!(from_slice, values);

    let from_reader = yaml::Deserializer::from_reader(Cursor::new(Vec::<u8>::new()))
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("yaml empty reader stream values");
    assert_eq!(from_reader, values);
}

#[test]
fn serde_api_deserializer_iterates_document_stream() {
    let input = "---\nname: one\nports: [80]\nenabled: true\n---\nname: two\nports: [443]\nenabled: false\n";
    let configs = yaml::Deserializer::from_str(input)
        .map(Config::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("stream configs");
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].name, "one");
    assert_eq!(configs[1].ports, [443]);

    let values = yaml::Deserializer::from_reader(Cursor::new(input))
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("stream values");
    assert_eq!(values[0]["name"].as_str(), Some("one"));
    assert_eq!(values[1]["enabled"].as_bool(), Some(false));
}

#[test]
fn serde_api_deserializer_stream_items_match_public_serde_yaml_surface() {
    fn assert_yaml_stream_items<'de, I>(iter: I) -> I
    where
        I: Iterator<Item = yaml::Deserializer<'de>>,
    {
        iter
    }

    fn assert_reference_stream_items<'de, I>(iter: I) -> I
    where
        I: Iterator<Item = serde_yaml::Deserializer<'de>>,
    {
        iter
    }

    let input = "---\nname: one\n---\nname: two\n";
    let mut ours = assert_yaml_stream_items(yaml::Deserializer::from_str(input));
    let mut reference = assert_reference_stream_items(serde_yaml::Deserializer::from_str(input));

    let ours_first: Value =
        Value::deserialize(ours.next().expect("first yaml document")).expect("first yaml value");
    let reference_first: serde_yaml::Value =
        serde_yaml::Value::deserialize(reference.next().expect("first reference document"))
            .expect("first reference value");

    assert_eq!(ours_first["name"].as_str(), Some("one"));
    assert_eq!(reference_first["name"].as_str(), Some("one"));
    assert!(ours.next().is_some());
    assert!(reference.next().is_some());
    assert!(ours.next().is_none());
    assert!(reference.next().is_none());
}

#[test]
fn serde_api_direct_deserializer_errors_match_public_yaml_error_surface() {
    fn assert_yaml_error_result<T>(result: Result<T, yaml::Error>) -> T {
        result.expect("deserialization through public yaml::Error")
    }

    let direct: Value = assert_yaml_error_result(Value::deserialize(yaml::Deserializer::from_str(
        "name: direct\n",
    )));
    assert_eq!(direct["name"].as_str(), Some("direct"));

    let mut stream = yaml::Deserializer::from_str("---\nname: stream\n");
    let stream_item: Value =
        assert_yaml_error_result(Value::deserialize(stream.next().expect("stream item")));
    assert_eq!(stream_item["name"].as_str(), Some("stream"));

    let value = Value::String("value".to_string());
    let value_owned: String = assert_yaml_error_result(String::deserialize(value.clone()));
    let value_ref: String = assert_yaml_error_result(String::deserialize(&value));
    let value_into: String =
        assert_yaml_error_result(String::deserialize(value.into_deserializer()));
    assert_eq!(value_owned, "value");
    assert_eq!(value_ref, "value");
    assert_eq!(value_into, "value");

    let node = yaml::parse_str("name: node\n").expect("parse node");
    let node_ref: Value = assert_yaml_error_result(Value::deserialize(&node));
    let node_owned: Value = assert_yaml_error_result(Value::deserialize(node.clone()));
    assert_eq!(node_ref["name"].as_str(), Some("node"));
    assert_eq!(node_owned["name"].as_str(), Some("node"));
}

#[test]
fn serde_api_stream_yields_prior_value_doc_before_later_parse_error() {
    let input =
        "---\ndefaults: &defaults\n  port: !Port 8080\nservice: *defaults\n---\nbad: *missing\n";
    let mut docs = yaml::Deserializer::from_str(input);

    let first = docs.next().expect("first document");
    let value: Value = Value::deserialize(first).expect("first value");
    assert_eq!(
        value["service"]["port"]
            .as_tagged()
            .expect("tagged port")
            .value
            .as_u64(),
        Some(8080)
    );

    let second = docs.next().expect("second document");
    let error = Value::deserialize(second).expect_err("later alias parse error");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.line(), Some(6));
    assert_eq!(error.column(), Some(6));
    assert!(docs.next().is_none());
}

#[test]
fn serde_api_direct_deserializer_preserves_related_parse_diagnostics() {
    let input = "root:\n  true: first\n  true: second\n";

    let from_str_error = yaml::from_str::<Value>(input).expect_err("duplicate key");
    let direct_error = Value::deserialize(yaml::Deserializer::from_str(input))
        .expect_err("direct deserializer duplicate key");

    assert_eq!(direct_error.diagnostic(), from_str_error.diagnostic());
    assert_eq!(direct_error.line(), Some(3));
    assert_eq!(direct_error.column(), Some(3));
}

#[test]
fn serde_api_stream_yields_prior_typed_doc_before_later_parse_error() {
    let input =
        "---\ndefaults: &defaults\n  port: !Port 8080\nservice: *defaults\n---\nbad: *missing\n";
    let mut docs = yaml::Deserializer::from_str(input);

    let first = docs.next().expect("first document");
    let config: StreamAliasConfig =
        StreamAliasConfig::deserialize(first).expect("first typed config");
    assert_eq!(config.service.port, 8080);

    let second = docs.next().expect("second document");
    let error = StreamAliasConfig::deserialize(second).expect_err("later alias parse error");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.line(), Some(6));
    assert_eq!(error.column(), Some(6));
    assert!(docs.next().is_none());
}

#[test]
fn serde_api_stream_rejects_malformed_single_document_before_yielding_it() {
    let input = "---\n[ok]\ntrailing: bad\n";
    let mut docs = yaml::Deserializer::from_str(input);

    let first = docs.next().expect("first item is the parse error");
    let error = Value::deserialize(first)
        .expect_err("malformed document must not be yielded as Ok before its error");
    assert!(
        error
            .to_string()
            .contains("unexpected content after root document node"),
        "unexpected error: {error}"
    );
    assert_eq!(error.line(), Some(3));
    assert_eq!(error.column(), Some(1));
    assert!(docs.next().is_none());
}

#[test]
fn serde_api_ignored_any_direct_deserializer_validates_before_skipping() {
    IgnoredAny::deserialize(yaml::Deserializer::from_str("name: api\n"))
        .expect("valid single document can be ignored");

    let malformed = "[ok]\ntrailing: bad\n";
    let error = IgnoredAny::deserialize(yaml::Deserializer::from_str(malformed))
        .expect_err("ignored_any must preserve malformed input errors");
    assert!(
        error
            .to_string()
            .contains("unexpected content after root document node"),
        "unexpected error: {error}"
    );
    assert_eq!(error.line(), Some(2));
    assert_eq!(error.column(), Some(1));

    let multi_doc = "---\nname: api\n---\nname: worker\n";
    let error = IgnoredAny::deserialize(yaml::Deserializer::from_str(multi_doc))
        .expect_err("ignored_any must not silently collapse streams");
    assert!(
        error
            .to_string()
            .contains("expected a single YAML document"),
        "unexpected error: {error}"
    );
    assert_eq!(error.line(), Some(4));
    assert_eq!(error.column(), Some(1));
}

#[test]
fn serde_api_ignored_any_stream_item_preserves_later_parse_errors() {
    let input = "---\nname: api\n---\nbad: *missing\n";
    let mut docs = yaml::Deserializer::from_str(input);

    IgnoredAny::deserialize(docs.next().expect("first document"))
        .expect("first valid document can be ignored");
    let error = IgnoredAny::deserialize(docs.next().expect("second document"))
        .expect_err("later stream parse error is preserved");

    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.line(), Some(4));
    assert_eq!(error.column(), Some(6));
    assert!(docs.next().is_none());
}

#[test]
fn serde_api_flow_mapping_metadata_scalar_keys_are_transparent_for_typed_reads() {
    let anchored: FlowStringKeyRoot =
        yaml::from_str("root: {&direct direct-key: v}\n").expect("anchored scalar key");
    assert_eq!(anchored.root["direct-key"], "v");

    let aliased: FlowStringKeyRoot =
        yaml::from_str("key: &key alias-key\nroot: {? *key : alias-v}\n")
            .expect("aliased scalar key");
    assert_eq!(aliased.root["alias-key"], "alias-v");

    let tagged: FlowStringKeyRoot =
        yaml::from_str("root: {!Thing tagged-key: tagged-v}\n").expect("tagged scalar key");
    assert_eq!(tagged.root["tagged-key"], "tagged-v");
}

#[test]
fn serde_api_flow_mapping_metadata_sequence_keys_deserialize_to_vec_keys() {
    let anchored: FlowSequenceKeyRoot =
        yaml::from_str("root: {? &seq [a, b] : seq-v}\n").expect("anchored sequence key");
    assert_eq!(
        anchored.root[&vec!["a".to_string(), "b".to_string()]],
        "seq-v"
    );

    let aliased: FlowSequenceKeyRoot =
        yaml::from_str("seq: &seq [a, b]\nroot: {? *seq : alias-v}\n")
            .expect("aliased sequence key");
    assert_eq!(
        aliased.root[&vec!["a".to_string(), "b".to_string()]],
        "alias-v"
    );

    let tagged: FlowSequenceKeyRoot =
        yaml::from_str("root: {? !Thing [c, d] : tagged-v}\n").expect("tagged sequence key");
    assert_eq!(
        tagged.root[&vec!["c".to_string(), "d".to_string()]],
        "tagged-v"
    );
}

#[test]
fn serde_api_flow_mapping_complex_key_rejects_string_key_with_key_span() {
    let error = yaml::from_str::<FlowStringKeyRoot>("root: {? {a: b}: value}\n")
        .expect_err("complex mapping key cannot deserialize as string key");
    assert!(error.to_string().contains("expected string"));
    assert_eq!(error.line(), Some(1));
    assert_eq!(error.column(), Some(10));
}

#[test]
fn serde_api_deserializer_can_drive_single_document_deserialize() {
    let input = "name: app\nports: [80]\nenabled: true\n";
    let config =
        Config::deserialize(yaml::Deserializer::from_str(input)).expect("deserialize config");
    assert_eq!(
        config,
        Config {
            name: "app".to_string(),
            ports: vec![80],
            enabled: true,
        }
    );

    let value = Value::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
        .expect("deserialize value");
    assert_eq!(value["name"].as_str(), Some("app"));

    let port = u16::deserialize(yaml::Deserializer::from_str("!Port 8080"))
        .expect("tagged scalar through deserializer");
    assert_eq!(port, 8080);
    let optional = Option::<String>::deserialize(yaml::Deserializer::from_str("!Maybe null"))
        .expect("tagged null option through deserializer");
    assert_eq!(optional, None);
}

#[test]
fn serde_api_single_document_deserializer_rejects_streams() {
    let input = "---\nname: one\n---\nname: two\n";
    let error = Value::deserialize(yaml::Deserializer::from_str(input))
        .expect_err("single-document deserializer rejects streams");
    assert!(error.to_string().contains("single YAML document"));
    assert!(error.to_string().contains("line 4"));
}

#[test]
fn serde_api_reads_yaml_tags_as_enum_variants() {
    let input = "\
- !Unit
- !Newtype 1
- !Tuple [0, 0, 0]
- !Struct {x: 1.0, y: 2.0}
- !String tagged
";

    let values: Vec<TaggedEnum> = yaml::from_str(input).expect("tagged enum values");
    assert_eq!(
        values,
        vec![
            TaggedEnum::Unit,
            TaggedEnum::Newtype(1),
            TaggedEnum::Tuple(0, 0, 0),
            TaggedEnum::Struct { x: 1.0, y: 2.0 },
            TaggedEnum::String("tagged".to_string()),
        ]
    );
}

#[test]
fn serde_api_value_preserves_yaml_tags() {
    let value: Value = yaml::from_str(
        "scalar: !Thing x\nsequence: !Thing [first]\nmapping: !Thing {k: v}\nempty: !Thing\n",
    )
    .expect("tagged value");

    let scalar = value["scalar"].as_tagged().expect("scalar tag");
    assert_eq!(scalar.tag, Tag::new("Thing"));
    assert_eq!(scalar.value.as_str(), Some("x"));

    let sequence = value["sequence"].as_tagged().expect("sequence tag");
    assert_eq!(sequence.tag, Tag::new("Thing"));
    assert_eq!(sequence.value[0].as_str(), Some("first"));

    let mapping = value["mapping"].as_tagged().expect("mapping tag");
    assert_eq!(mapping.tag, Tag::new("Thing"));
    assert_eq!(mapping.value["k"].as_str(), Some("v"));

    let empty = value["empty"].as_tagged().expect("empty tag");
    assert_eq!(empty.tag, Tag::new("Thing"));
    assert!(empty.value.is_null());
}

#[test]
fn serde_api_tagged_value_deserializes_directly_like_serde_yaml() {
    let input =
        "scalar: !Thing x\nsequence: !Thing [first]\nmapping: !Thing {k: v}\nempty: !Thing\n";

    let values: BTreeMap<String, TaggedValue> =
        yaml::from_str(input).expect("direct tagged values");
    let node = yaml::parse_str(input).expect("tagged value node");
    let from_node: BTreeMap<String, TaggedValue> =
        yaml::from_node(&node).expect("direct tagged values from node");
    let value: Value = yaml::from_str(input).expect("tagged Value");
    let from_value: BTreeMap<String, TaggedValue> =
        yaml::from_value(value.clone()).expect("direct tagged values from value");
    let from_value_ref =
        BTreeMap::<String, TaggedValue>::deserialize(&value).expect("direct tagged values by ref");
    let reference: BTreeMap<String, serde_yaml::value::TaggedValue> =
        serde_yaml::from_str(input).expect("serde_yaml direct tagged values");

    for map in [&values, &from_node, &from_value, &from_value_ref] {
        let scalar = &map["scalar"];
        assert_eq!(scalar.tag, Tag::new("Thing"));
        assert_eq!(scalar.value.as_str(), Some("x"));

        let sequence = &map["sequence"];
        assert_eq!(sequence.tag, Tag::new("Thing"));
        assert_eq!(sequence.value[0].as_str(), Some("first"));

        let mapping = &map["mapping"];
        assert_eq!(mapping.tag, Tag::new("Thing"));
        assert_eq!(mapping.value["k"].as_str(), Some("v"));

        let empty = &map["empty"];
        assert_eq!(empty.tag, Tag::new("Thing"));
        assert!(empty.value.is_null());
    }

    assert!(reference["scalar"].tag == "Thing");
    assert_eq!(reference["scalar"].value.as_str(), Some("x"));
    assert!(reference["sequence"].tag == "Thing");
    assert_eq!(
        reference["sequence"].value.as_sequence().expect("sequence")[0].as_str(),
        Some("first")
    );
    assert!(reference["mapping"].tag == "Thing");
    assert_eq!(reference["mapping"].value["k"].as_str(), Some("v"));
    assert!(reference["empty"].tag == "Thing");
    assert!(reference["empty"].value.is_null());

    let error = yaml::from_str::<TaggedValue>("plain\n").expect_err("untagged value rejected");
    assert!(
        error.to_string().contains("tagged value"),
        "unexpected error: {error}"
    );
}

#[test]
fn serde_api_value_preserves_core_local_and_verbatim_tags() {
    let value: Value = yaml::from_str(
        "core: !!int 0x7B\ncustom: !g:ow\nverbatim: !<tag:example.com,2026:Thing> tagged\n",
    )
    .expect("tagged value");

    assert_eq!(
        value["core"].as_tagged().expect("core tag").tag,
        Tag::new("!!int")
    );
    assert_eq!(
        value["custom"].as_tagged().expect("custom tag").tag,
        Tag::new("!g:ow")
    );
    assert_eq!(
        value["verbatim"].as_tagged().expect("verbatim tag").tag,
        Tag::new("!<tag:example.com,2026:Thing>")
    );

    let fuzz_input = b"scnce: !g:ow\n\0: pTht ";
    let node = yaml::parse_bytes(fuzz_input).expect("parse fuzz regression");
    let from_node = Value::from(&node);
    let from_slice: Value = yaml::from_slice(fuzz_input).expect("deserialize fuzz regression");

    assert!(from_node.equivalent(&from_slice));

    let leading_bang = yaml::parse_str("!<!ab> value\n").expect("parse verbatim bang tag");
    let direct = Value::from(&leading_bang);
    let from_node: Value = yaml::from_node(&leading_bang).expect("deserialize verbatim bang tag");
    assert!(direct.equivalent(&from_node));
}

#[test]
fn serde_api_explicit_core_numeric_tags_support_yaml11_legacy_typed_reads() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct ExplicitCoreNumbers {
        hex: i64,
        octal: i64,
        sexagesimal: i64,
        negative_sexagesimal: i64,
        unsigned: u64,
        as_float: f64,
        float_from_int_tag: f64,
        sexagesimal_float: f64,
        sexagesimal_seconds_float: f64,
        inf: f64,
        neg_inf: f64,
        nan: f64,
    }

    fn assert_explicit_core_numbers(actual: &ExplicitCoreNumbers) {
        assert_eq!(actual.hex, 123);
        assert_eq!(actual.octal, 83);
        assert_eq!(actual.sexagesimal, 4800);
        assert_eq!(actual.negative_sexagesimal, -2400);
        assert_eq!(actual.unsigned, 42);
        assert_eq!(actual.as_float, 7.0);
        assert_eq!(actual.float_from_int_tag, 4830.0);
        assert_eq!(actual.sexagesimal_float, 4830.0);
        assert_eq!(actual.sexagesimal_seconds_float, 4830.5);
        assert_eq!(actual.inf, f64::INFINITY);
        assert_eq!(actual.neg_inf, f64::NEG_INFINITY);
        assert!(actual.nan.is_nan());
    }

    let input = "\
hex: !!int 0x7B
octal: !!int 0123
sexagesimal: !!int 1:20
negative_sexagesimal: !!int -1:20
unsigned: !!int +42
as_float: !!int 7
float_from_int_tag: !!int 1:20.5
sexagesimal_float: !!float 1:20.5
sexagesimal_seconds_float: !!float 1:20:30.5
inf: !!float .inf
neg_inf: !!float -.inf
nan: !!float .nan
";
    let ours: ExplicitCoreNumbers = yaml::from_str(input).expect("explicit core numeric tags");
    assert_explicit_core_numbers(&ours);

    let node = yaml::parse_str(input).expect("node");
    let from_node: ExplicitCoreNumbers =
        yaml::from_node(&node).expect("explicit core numeric tags from node");
    assert_explicit_core_numbers(&from_node);

    let value: Value = yaml::from_str(input).expect("tagged value");
    assert_eq!(
        value["hex"].as_tagged().expect("hex tag").tag,
        Tag::new("!!int")
    );
    assert_eq!(value["hex"].as_str(), Some("0x7B"));
    assert_eq!(value["hex"].as_i64(), Some(123));
    assert_eq!(value["hex"].as_u64(), Some(123));
    assert_eq!(value["hex"].as_i128(), Some(123));
    assert_eq!(value["hex"].as_u128(), Some(123));
    assert_eq!(value["hex"].as_f64(), Some(123.0));
    assert!(value["hex"].is_number());
    assert!(value["hex"].is_i64());
    assert!(value["hex"].is_u64());
    assert!(!value["hex"].is_f64());
    assert_eq!(value["octal"].as_str(), Some("0123"));
    assert_eq!(value["octal"].as_u64(), Some(83));
    assert_eq!(value["sexagesimal"].as_str(), Some("1:20"));
    assert_eq!(value["sexagesimal"].as_i64(), Some(4800));
    assert_eq!(value["negative_sexagesimal"].as_i64(), Some(-2400));
    assert_eq!(value["unsigned"].as_u64(), Some(42));
    assert_eq!(value["as_float"].as_f64(), Some(7.0));
    assert_eq!(value["float_from_int_tag"].as_i64(), None);
    assert_eq!(value["float_from_int_tag"].as_f64(), Some(4830.0));
    assert!(value["float_from_int_tag"].is_f64());
    assert_eq!(value["sexagesimal_float"].as_f64(), Some(4830.0));
    assert_eq!(value["sexagesimal_seconds_float"].as_f64(), Some(4830.5));
    assert_eq!(value["inf"].as_f64(), Some(f64::INFINITY));
    assert_eq!(value["neg_inf"].as_f64(), Some(f64::NEG_INFINITY));
    assert!(value["nan"].as_f64().expect("nan helper").is_nan());
    assert!(value["nan"].is_number());
    assert!(value["nan"].is_f64());
    let from_value: ExplicitCoreNumbers =
        yaml::from_value(value.clone()).expect("explicit core numeric tags from value");
    let from_value_ref =
        ExplicitCoreNumbers::deserialize(&value).expect("explicit core numeric tags by ref");
    assert_explicit_core_numbers(&from_value);
    assert_explicit_core_numbers(&from_value_ref);

    let invalid: Value = yaml::from_str("bad_int: !!int nope\nbad_float: !!float nope\n")
        .expect("invalid explicit numeric tags stay retained values");
    assert_eq!(invalid["bad_int"].as_str(), Some("nope"));
    assert_eq!(invalid["bad_int"].as_i64(), None);
    assert_eq!(invalid["bad_int"].as_u64(), None);
    assert_eq!(invalid["bad_int"].as_f64(), None);
    assert!(!invalid["bad_int"].is_number());
    assert_eq!(invalid["bad_float"].as_f64(), None);
    assert!(!invalid["bad_float"].is_number());
}

#[test]
fn serde_api_canonical_core_tags_match_short_core_tag_semantics() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct CanonicalCoreTags {
        canonical_int: i64,
        resolved_float: f64,
        canonical_bool: bool,
        resolved_null: Option<String>,
        canonical_str: String,
        resolved_binary: Vec<u8>,
        canonical_timestamp: yaml::Timestamp,
    }

    let input = "\
%TAG !yaml! tag:yaml.org,2002:
---
canonical_int: !<tag:yaml.org,2002:int> 0x7B
resolved_float: !yaml!float 1:20:30.5
canonical_bool: !<tag:yaml.org,2002:bool> ON
resolved_null: !yaml!null ~
canonical_str: !<tag:yaml.org,2002:str> true
resolved_binary: !yaml!binary SGVsbG8=
canonical_timestamp: !<tag:yaml.org,2002:timestamp> 2026-05-25
";
    let expected = CanonicalCoreTags {
        canonical_int: 123,
        resolved_float: 4830.5,
        canonical_bool: true,
        resolved_null: None,
        canonical_str: "true".to_string(),
        resolved_binary: b"Hello".to_vec(),
        canonical_timestamp: yaml::Timestamp::parse_yaml_1_1("2026-05-25").expect("timestamp"),
    };

    let typed: CanonicalCoreTags = yaml::from_str(input).expect("canonical core tags");
    let direct = CanonicalCoreTags::deserialize(yaml::Deserializer::from_str(input))
        .expect("direct canonical core tags");
    let node = yaml::parse_str(input).expect("canonical core tag node");
    let from_node: CanonicalCoreTags =
        yaml::from_node(&node).expect("canonical core tags from node");
    let value: Value = yaml::from_str(input).expect("canonical core tag value");
    let from_value: CanonicalCoreTags =
        yaml::from_value(value.clone()).expect("canonical core tags from value");
    let from_value_ref =
        CanonicalCoreTags::deserialize(&value).expect("canonical core tags by ref");

    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
    assert_eq!(from_node, expected);
    assert_eq!(from_value, expected);
    assert_eq!(from_value_ref, expected);

    let canonical_int = value["canonical_int"]
        .as_tagged()
        .expect("canonical int tag");
    assert_eq!(canonical_int.tag.handle, "!");
    assert_eq!(canonical_int.tag.suffix, "tag:yaml.org,2002:int");
    assert_eq!(value["canonical_int"].as_str(), Some("0x7B"));
    assert_eq!(value["canonical_int"].as_i64(), Some(123));
    assert_eq!(value["canonical_int"].as_u64(), Some(123));
    assert!(value["canonical_int"].is_number());

    let resolved_float = value["resolved_float"]
        .as_tagged()
        .expect("resolved float tag");
    assert_eq!(resolved_float.tag.handle, "!");
    assert_eq!(resolved_float.tag.suffix, "tag:yaml.org,2002:float");
    assert_eq!(value["resolved_float"].as_f64(), Some(4830.5));
    assert!(value["resolved_float"].is_f64());

    let canonical_bool = value["canonical_bool"]
        .as_tagged()
        .expect("canonical bool tag");
    assert_eq!(canonical_bool.tag.handle, "!");
    assert_eq!(canonical_bool.tag.suffix, "tag:yaml.org,2002:bool");
    assert_eq!(value["canonical_bool"].as_bool(), Some(true));
    assert_eq!(value["resolved_null"].as_null(), Some(()));
    assert_eq!(value["canonical_str"].as_str(), Some("true"));
    assert_eq!(value["resolved_binary"].as_str(), Some("SGVsbG8="));
    assert_eq!(
        value["canonical_timestamp"].as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-25")
    );
}

#[test]
fn serde_api_yaml11_collection_tags_have_typed_read_contract() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct CollectionTags {
        set: BTreeSet<String>,
        omap: Vec<(String, i64)>,
        pairs: Vec<(String, i64)>,
        canonical_set: BTreeSet<String>,
        resolved_omap: BTreeMap<String, i64>,
        resolved_pairs: Vec<(String, i64)>,
    }

    let input = "\
%TAG !yaml! tag:yaml.org,2002:
---
set: !!set
  ? alpha
  ? beta
omap: !!omap
  - first: 1
  - second: 2
pairs: !!pairs
  - repeat: 1
  - repeat: 2
canonical_set: !<tag:yaml.org,2002:set> {left: null, right: null}
resolved_omap: !yaml!omap [{left: 1}, {right: 2}]
resolved_pairs: !yaml!pairs [{same: 1}, {same: 2}]
";
    let expected = CollectionTags {
        set: BTreeSet::from(["alpha".to_string(), "beta".to_string()]),
        omap: vec![("first".to_string(), 1), ("second".to_string(), 2)],
        pairs: vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)],
        canonical_set: BTreeSet::from(["left".to_string(), "right".to_string()]),
        resolved_omap: BTreeMap::from([("left".to_string(), 1), ("right".to_string(), 2)]),
        resolved_pairs: vec![("same".to_string(), 1), ("same".to_string(), 2)],
    };

    let typed: CollectionTags = yaml::from_str(input).expect("YAML 1.1 collection tags");
    let direct = CollectionTags::deserialize(yaml::Deserializer::from_str(input))
        .expect("direct YAML 1.1 collection tags");
    let node = yaml::parse_str(input).expect("YAML 1.1 collection tag node");
    let from_node: CollectionTags =
        yaml::from_node(&node).expect("YAML 1.1 collection tags from node");
    let value: Value = yaml::from_str(input).expect("YAML 1.1 collection tag value");
    let from_value: CollectionTags =
        yaml::from_value(value.clone()).expect("YAML 1.1 collection tags from value");
    let from_value_ref =
        CollectionTags::deserialize(&value).expect("YAML 1.1 collection tags by ref");

    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
    assert_eq!(from_node, expected);
    assert_eq!(from_value, expected);
    assert_eq!(from_value_ref, expected);

    let set = value["set"].as_tagged().expect("set tag is retained");
    assert_eq!(set.tag.handle, "!!");
    assert_eq!(set.tag.suffix, "set");
    assert!(matches!(set.value, Value::Mapping(_)));

    let canonical_set = value["canonical_set"]
        .as_tagged()
        .expect("canonical set tag is retained");
    assert_eq!(canonical_set.tag.handle, "!");
    assert_eq!(canonical_set.tag.suffix, "tag:yaml.org,2002:set");
    assert!(matches!(canonical_set.value, Value::Mapping(_)));

    let resolved_omap = value["resolved_omap"]
        .as_tagged()
        .expect("resolved omap tag is retained");
    assert_eq!(resolved_omap.tag.handle, "!");
    assert_eq!(resolved_omap.tag.suffix, "tag:yaml.org,2002:omap");
    assert!(matches!(resolved_omap.value, Value::Sequence(_)));

    let resolved_pairs = value["resolved_pairs"]
        .as_tagged()
        .expect("resolved pairs tag is retained");
    assert_eq!(resolved_pairs.tag.handle, "!");
    assert_eq!(resolved_pairs.tag.suffix, "tag:yaml.org,2002:pairs");
    assert!(matches!(resolved_pairs.value, Value::Sequence(_)));
}

#[test]
fn serde_api_explicit_core_scalar_tags_override_implicit_resolution() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct ExplicitCoreScalars {
        string_null: String,
        optional_string_null: Option<String>,
        string_bool: String,
        yes: bool,
        off: bool,
        maybe: Option<String>,
        unit: (),
    }

    let input = "\
string_null: !!str null
optional_string_null: !!str null
string_bool: !!str true
yes: !!bool YES
off: !!bool off
maybe: !!null null
unit: !!null ~
";
    let expected = ExplicitCoreScalars {
        string_null: "null".to_string(),
        optional_string_null: Some("null".to_string()),
        string_bool: "true".to_string(),
        yes: true,
        off: false,
        maybe: None,
        unit: (),
    };

    let typed: ExplicitCoreScalars = yaml::from_str(input).expect("explicit core scalar tags");
    let direct: ExplicitCoreScalars =
        ExplicitCoreScalars::deserialize(yaml::Deserializer::from_str(input))
            .expect("direct explicit core scalar tags");
    let node = yaml::parse_str(input).expect("explicit core scalar tag node");
    let from_node: ExplicitCoreScalars =
        yaml::from_node(&node).expect("explicit core scalar tags from node");
    let value: Value = yaml::from_str(input).expect("explicit core scalar tag value");
    let from_value: ExplicitCoreScalars =
        yaml::from_value(value.clone()).expect("explicit core scalar tags from value");
    let from_value_ref =
        ExplicitCoreScalars::deserialize(&value).expect("explicit core scalar tags by ref");

    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
    assert_eq!(from_node, expected);
    assert_eq!(from_value, expected);
    assert_eq!(from_value_ref, expected);

    assert_eq!(
        value["string_null"].as_tagged().expect("!!str tag").tag,
        Tag::new("!!str")
    );
    assert_eq!(value["string_null"].as_str(), Some("null"));
    assert_eq!(value["string_bool"].as_str(), Some("true"));
    assert_eq!(
        value["yes"].as_tagged().expect("!!bool tag").tag,
        Tag::new("!!bool")
    );
    assert_eq!(value["yes"].as_bool(), Some(true));
    assert_eq!(value["off"].as_bool(), Some(false));
    assert_eq!(
        value["maybe"].as_tagged().expect("!!null tag").tag,
        Tag::new("!!null")
    );
    assert_eq!(value["maybe"].as_null(), Some(()));

    let directive_bool: bool = LoadOptions::yaml_version_directive()
        .from_str("%YAML 1.1\n--- !!bool YES\n")
        .expect("directive-driven explicit bool");
    assert!(directive_bool);

    let bool_error = yaml::from_str::<bool>("!!bool maybe\n").expect_err("invalid explicit bool");
    assert!(
        bool_error
            .to_string()
            .contains("failed to parse explicit !!bool scalar"),
        "{bool_error}"
    );
    let null_error =
        yaml::from_str::<Option<String>>("!!null foo\n").expect_err("invalid explicit null");
    assert!(
        null_error
            .to_string()
            .contains("failed to parse explicit !!null scalar"),
        "{null_error}"
    );
}

#[test]
fn serde_api_percent_tag_resolves_handles_and_stays_transparent_for_typed_reads() {
    let input = "%TAG !e! tag:example.com,2026:\n---\nvalue: !e!Thing tagged\n";
    let value: Value = yaml::from_str(input).expect("tag directive value");
    let tagged = value["value"].as_tagged().expect("resolved tag");

    assert_eq!(tagged.tag.handle, "!");
    assert_eq!(tagged.tag.suffix, "tag:example.com,2026:Thing");
    assert_eq!(tagged.value.as_str(), Some("tagged"));

    let typed: BTreeMap<String, String> = yaml::from_str(input).expect("typed transparent tag");
    assert_eq!(typed["value"], "tagged");
}

#[test]
fn serde_api_tag_directives_require_explicit_document_start_and_do_not_leak() {
    let missing_start = "%TAG !e! tag:example.com,2026:\nvalue: !e!Thing tagged\n";
    let error = yaml::from_str::<Value>(missing_start).expect_err("directive without marker");
    assert!(error.to_string().contains("explicit document start"));

    let declared = "%TAG !e! tag:example.com,2026:\n---\n!e!Thing one\n";
    let value: Value = yaml::from_str(declared).expect("declared tag");
    let tagged = value.as_tagged().expect("resolved tag");
    assert_eq!(tagged.tag.suffix, "tag:example.com,2026:Thing");

    let scoped = "%TAG !e! tag:example.com,2026:\n---\n!e!Thing one\n...\n---\n!e!Thing two\n";
    let error = yaml::from_documents_str::<Value>(scoped).expect_err("tag directive does not leak");
    assert!(
        error
            .to_string()
            .contains("undeclared TAG directive handle")
    );
}

#[test]
fn serde_api_tagged_value_can_drive_enum_deserialize_by_value_and_reference() {
    let value = Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new("Newtype"),
        value: Value::Number(Number::Unsigned(7)),
    }));

    let owned: TaggedEnum = yaml::from_value(value.clone()).expect("tagged enum by value");
    let borrowed = TaggedEnum::deserialize(&value).expect("tagged enum by reference");

    assert_eq!(owned, TaggedEnum::Newtype(7));
    assert_eq!(borrowed, TaggedEnum::Newtype(7));
}

#[test]
fn serde_api_tags_are_transparent_for_non_enum_typed_reads() {
    let input = "\
name: !Env prod
ports: !Ports [80, 443]
limits: !Limits {cpu: \"1\"}
enabled: !Flag true
optional: !Maybe null
";
    let expected = TaggedConfig {
        name: "prod".to_string(),
        ports: vec![80, 443],
        limits: BTreeMap::from([("cpu".to_string(), "1".to_string())]),
        enabled: true,
        optional: None,
    };

    let from_str: TaggedConfig = yaml::from_str(input).expect("from_str");
    let from_slice: TaggedConfig = yaml::from_slice(input.as_bytes()).expect("from_slice");
    let from_reader: TaggedConfig =
        yaml::from_reader(Cursor::new(input.as_bytes())).expect("from_reader");
    let node = yaml::parse_str(input).expect("parse node");
    let from_node: TaggedConfig = yaml::from_node(&node).expect("from_node");
    let value: Value = yaml::from_str(input).expect("value");
    let from_value: TaggedConfig = yaml::from_value(value.clone()).expect("from_value");
    let from_value_ref =
        TaggedConfig::deserialize(&value).expect("deserialize tagged value by reference");

    assert_eq!(from_str, expected);
    assert_eq!(from_slice, expected);
    assert_eq!(from_reader, expected);
    assert_eq!(from_node, expected);
    assert_eq!(from_value, expected);
    assert_eq!(from_value_ref, expected);
}

#[test]
fn serde_api_tagged_anchor_alias_matrix_preserves_tags_across_entrypoints() {
    struct Case {
        name: &'static str,
        input: &'static str,
        tag: &'static str,
        shape: TaggedAnchorShape,
    }

    let scalar = TaggedAnchorPayload::Text("value".to_string());
    let sequence = TaggedAnchorPayload::List(vec!["one".to_string(), "two".to_string()]);
    let mapping =
        TaggedAnchorPayload::Map(BTreeMap::from([("name".to_string(), "prod".to_string())]));
    let unsigned = TaggedAnchorPayload::CoreUnsigned {
        text: "7",
        value: 7,
    };
    let float = TaggedAnchorPayload::CoreFloat {
        text: "1.5",
        value: 1.5,
    };

    let cases = [
        Case {
            name: "block scalar value anchor before tag",
            input: "first: &a !Thing value\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(scalar.clone()),
        },
        Case {
            name: "block scalar value tag before anchor",
            input: "first: !Thing &a value\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(scalar.clone()),
        },
        Case {
            name: "flow sequence value anchor before tag",
            input: "first: &a !Thing [one, two]\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(sequence.clone()),
        },
        Case {
            name: "flow sequence value tag before anchor",
            input: "first: !Thing &a [one, two]\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(sequence.clone()),
        },
        Case {
            name: "indented sequence value anchor before tag",
            input: "first: &a !Thing\n  - one\n  - two\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(sequence.clone()),
        },
        Case {
            name: "indented sequence value tag before anchor",
            input: "first: !Thing &a\n  - one\n  - two\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(sequence),
        },
        Case {
            name: "flow mapping value anchor before tag",
            input: "first: &a !Thing {name: prod}\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(mapping.clone()),
        },
        Case {
            name: "flow mapping value tag before anchor",
            input: "first: !Thing &a {name: prod}\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(mapping.clone()),
        },
        Case {
            name: "indented mapping value anchor before tag",
            input: "first: &a !Thing\n  name: prod\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(mapping.clone()),
        },
        Case {
            name: "indented mapping value tag before anchor",
            input: "first: !Thing &a\n  name: prod\nsecond: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::ValuePair(mapping),
        },
        Case {
            name: "directive tag value anchor before tag",
            input: "%TAG !e! tag:example.com,2026:\n---\nfirst: &a !e!Thing value\nsecond: *a\n",
            tag: "!<tag:example.com,2026:Thing>",
            shape: TaggedAnchorShape::ValuePair(scalar.clone()),
        },
        Case {
            name: "directive tag value tag before anchor",
            input: "%TAG !e! tag:example.com,2026:\n---\nfirst: !e!Thing &a value\nsecond: *a\n",
            tag: "!<tag:example.com,2026:Thing>",
            shape: TaggedAnchorShape::ValuePair(scalar.clone()),
        },
        Case {
            name: "explicit int value anchor before tag",
            input: "first: &a !!int 7\nsecond: *a\n",
            tag: "!!int",
            shape: TaggedAnchorShape::ValuePair(unsigned.clone()),
        },
        Case {
            name: "explicit int value tag before anchor",
            input: "first: !!int &a 7\nsecond: *a\n",
            tag: "!!int",
            shape: TaggedAnchorShape::ValuePair(unsigned),
        },
        Case {
            name: "explicit float value anchor before tag",
            input: "first: &a !!float 1.5\nsecond: *a\n",
            tag: "!!float",
            shape: TaggedAnchorShape::ValuePair(float.clone()),
        },
        Case {
            name: "explicit float value tag before anchor",
            input: "first: !!float &a 1.5\nsecond: *a\n",
            tag: "!!float",
            shape: TaggedAnchorShape::ValuePair(float),
        },
        Case {
            name: "block scalar key anchor before tag",
            input: "root:\n  ? &a !Thing tagged-key\n  : first\nalias_value: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::KeyPair,
        },
        Case {
            name: "block scalar key tag before anchor",
            input: "root:\n  ? !Thing &a tagged-key\n  : first\nalias_value: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::KeyPair,
        },
        Case {
            name: "flow scalar key anchor before tag",
            input: "root: {? &a !Thing tagged-key : first}\nalias_value: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::KeyPair,
        },
        Case {
            name: "flow scalar key tag before anchor",
            input: "root: {? !Thing &a tagged-key : first}\nalias_value: *a\n",
            tag: "Thing",
            shape: TaggedAnchorShape::KeyPair,
        },
    ];

    for case in cases {
        let expected_tag = Tag::new(case.tag);
        let node = yaml::parse_str(case.input)
            .unwrap_or_else(|error| panic!("parse {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &Value::from(&node), &expected_tag, &case.shape);

        let from_node: Value = yaml::from_node(&node)
            .unwrap_or_else(|error| panic!("from_node {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_node, &expected_tag, &case.shape);

        let from_str: Value = yaml::from_str(case.input)
            .unwrap_or_else(|error| panic!("from_str {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_str, &expected_tag, &case.shape);

        let from_slice: Value = yaml::from_slice(case.input.as_bytes())
            .unwrap_or_else(|error| panic!("from_slice {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_slice, &expected_tag, &case.shape);

        let from_reader: Value = yaml::from_reader(Cursor::new(case.input.as_bytes()))
            .unwrap_or_else(|error| panic!("from_reader {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_reader, &expected_tag, &case.shape);

        let from_value: Value = yaml::from_value(from_str.clone())
            .unwrap_or_else(|error| panic!("from_value {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_value, &expected_tag, &case.shape);

        let from_value_ref = Value::deserialize(&from_str)
            .unwrap_or_else(|error| panic!("&Value {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &from_value_ref, &expected_tag, &case.shape);

        let document = yaml::Deserializer::from_str(case.input)
            .next()
            .unwrap_or_else(|| panic!("Deserializer::from_str {} yields one doc", case.name));
        let direct_str = Value::deserialize(document)
            .unwrap_or_else(|error| panic!("direct str deserializer {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &direct_str, &expected_tag, &case.shape);

        let document = yaml::Deserializer::from_slice(case.input.as_bytes())
            .next()
            .unwrap_or_else(|| panic!("Deserializer::from_slice {} yields one doc", case.name));
        let direct_slice = Value::deserialize(document)
            .unwrap_or_else(|error| panic!("direct slice deserializer {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &direct_slice, &expected_tag, &case.shape);

        let document = yaml::Deserializer::from_reader(Cursor::new(case.input.as_bytes()))
            .next()
            .unwrap_or_else(|| panic!("Deserializer::from_reader {} yields one doc", case.name));
        let direct_reader = Value::deserialize(document)
            .unwrap_or_else(|error| panic!("direct reader deserializer {}: {error}", case.name));
        assert_tagged_anchor_shape(case.name, &direct_reader, &expected_tag, &case.shape);

        for (surface, docs) in [
            (
                "from_documents_str",
                yaml::from_documents_str::<Value>(case.input),
            ),
            (
                "from_documents_slice",
                yaml::from_documents_slice::<Value>(case.input.as_bytes()),
            ),
            (
                "from_documents_reader",
                yaml::from_documents_reader::<Value, _>(Cursor::new(case.input.as_bytes())),
            ),
        ] {
            let docs = docs.unwrap_or_else(|error| panic!("{surface} {}: {error}", case.name));
            assert_eq!(docs.len(), 1, "{surface} {} doc count", case.name);
            assert_tagged_anchor_shape(case.name, &docs[0], &expected_tag, &case.shape);
        }

        match &case.shape {
            TaggedAnchorShape::ValuePair(expected) => {
                assert_tagged_anchor_value_typed_entrypoints(case.name, case.input, expected)
            }
            TaggedAnchorShape::KeyPair => {
                assert_tagged_anchor_key_typed_entrypoints(case.name, case.input)
            }
        }
    }
}

fn assert_tagged_anchor_shape(
    name: &str,
    value: &Value,
    expected_tag: &Tag,
    shape: &TaggedAnchorShape,
) {
    match shape {
        TaggedAnchorShape::ValuePair(expected) => {
            assert_tagged_anchor_value_pair(name, value, expected_tag, expected);
        }
        TaggedAnchorShape::KeyPair => assert_tagged_anchor_key_pair(name, value, expected_tag),
    }
}

fn assert_tagged_anchor_value_pair(
    name: &str,
    value: &Value,
    expected_tag: &Tag,
    expected: &TaggedAnchorPayload,
) {
    let first = assert_tagged_value(&value["first"], name, "first", expected_tag);
    let second = assert_tagged_value(&value["second"], name, "second", expected_tag);
    assert_eq!(
        first.value, second.value,
        "{name} alias value must retain the same tagged payload",
    );
    assert_tagged_payload(name, "first", &first.value, expected);
    assert_tagged_payload(name, "second", &second.value, expected);
}

fn assert_tagged_anchor_key_pair(name: &str, value: &Value, expected_tag: &Tag) {
    let root = value["root"]
        .as_mapping()
        .unwrap_or_else(|| panic!("{name} root must be a mapping"));
    let (tagged_key, entry_value) = root
        .iter()
        .find_map(|(key, value)| key.as_tagged().map(|tagged| (tagged, value)))
        .unwrap_or_else(|| panic!("{name} root must contain a tagged key"));
    assert_eq!(&tagged_key.tag, expected_tag, "{name} key tag");
    assert_eq!(tagged_key.value.as_str(), Some("tagged-key"));
    assert_eq!(entry_value.as_str(), Some("first"));

    let alias = assert_tagged_value(&value["alias_value"], name, "alias_value", expected_tag);
    assert_eq!(alias.value.as_str(), Some("tagged-key"));
}

fn assert_tagged_value<'a>(
    value: &'a Value,
    name: &str,
    field: &str,
    expected_tag: &Tag,
) -> &'a TaggedValue {
    let tagged = value
        .as_tagged()
        .unwrap_or_else(|| panic!("{name} {field} must be tagged"));
    assert_eq!(&tagged.tag, expected_tag, "{name} {field} tag");
    tagged
}

fn assert_tagged_payload(name: &str, field: &str, value: &Value, expected: &TaggedAnchorPayload) {
    match expected {
        TaggedAnchorPayload::Text(expected) => {
            assert_eq!(value.as_str(), Some(expected.as_str()), "{name} {field}");
        }
        TaggedAnchorPayload::List(expected) => {
            let actual = value
                .as_sequence()
                .unwrap_or_else(|| panic!("{name} {field} must be a sequence"));
            let actual = actual
                .iter()
                .map(|item| item.as_str().expect("sequence item string").to_string())
                .collect::<Vec<_>>();
            assert_eq!(actual.as_slice(), expected.as_slice(), "{name} {field}");
        }
        TaggedAnchorPayload::Map(expected) => {
            let actual = value
                .as_mapping()
                .unwrap_or_else(|| panic!("{name} {field} must be a mapping"));
            assert_eq!(actual.len(), expected.len(), "{name} {field} map len");
            for (key, expected_value) in expected {
                assert_eq!(
                    actual
                        .get(Value::String(key.clone()))
                        .and_then(Value::as_str),
                    Some(expected_value.as_str()),
                    "{name} {field}.{key}",
                );
            }
        }
        TaggedAnchorPayload::CoreUnsigned { text, .. } => {
            assert_eq!(value.as_str(), Some(*text), "{name} {field}");
        }
        TaggedAnchorPayload::CoreFloat { text, .. } => {
            assert_eq!(value.as_str(), Some(*text), "{name} {field}");
        }
    }
}

fn assert_tagged_anchor_value_typed_entrypoints(
    name: &str,
    input: &str,
    expected: &TaggedAnchorPayload,
) {
    match expected {
        TaggedAnchorPayload::Text(expected) => {
            assert_typed_anchor_entrypoints::<TaggedAnchorScalarRead>(
                name,
                input,
                TaggedAnchorScalarRead {
                    first: expected.clone(),
                    second: expected.clone(),
                },
            );
        }
        TaggedAnchorPayload::List(expected) => {
            assert_typed_anchor_entrypoints::<TaggedAnchorSequenceRead>(
                name,
                input,
                TaggedAnchorSequenceRead {
                    first: expected.clone(),
                    second: expected.clone(),
                },
            );
        }
        TaggedAnchorPayload::Map(expected) => {
            assert_typed_anchor_entrypoints::<TaggedAnchorMappingRead>(
                name,
                input,
                TaggedAnchorMappingRead {
                    first: expected.clone(),
                    second: expected.clone(),
                },
            );
        }
        TaggedAnchorPayload::CoreUnsigned { value, .. } => {
            assert_typed_anchor_entrypoints::<TaggedAnchorUnsignedRead>(
                name,
                input,
                TaggedAnchorUnsignedRead {
                    first: *value,
                    second: *value,
                },
            );
        }
        TaggedAnchorPayload::CoreFloat { value, .. } => {
            assert_typed_anchor_entrypoints::<TaggedAnchorFloatRead>(
                name,
                input,
                TaggedAnchorFloatRead {
                    first: *value,
                    second: *value,
                },
            );
        }
    }
}

fn assert_tagged_anchor_key_typed_entrypoints(name: &str, input: &str) {
    let expected = TaggedAnchorKeyRead {
        root: BTreeMap::from([("tagged-key".to_string(), "first".to_string())]),
        alias_value: "tagged-key".to_string(),
    };
    assert_typed_anchor_entrypoints::<TaggedAnchorKeyRead>(name, input, expected);
}

fn assert_typed_anchor_entrypoints<T>(name: &str, input: &str, expected: T)
where
    T: Clone + fmt::Debug + PartialEq + for<'de> Deserialize<'de>,
{
    let from_str =
        yaml::from_str::<T>(input).unwrap_or_else(|error| panic!("{name} from_str: {error}"));
    assert_eq!(from_str, expected, "{name} from_str");

    let from_slice = yaml::from_slice::<T>(input.as_bytes())
        .unwrap_or_else(|error| panic!("{name} from_slice: {error}"));
    assert_eq!(from_slice, expected, "{name} from_slice");

    let from_reader = yaml::from_reader::<_, T>(Cursor::new(input.as_bytes()))
        .unwrap_or_else(|error| panic!("{name} from_reader: {error}"));
    assert_eq!(from_reader, expected, "{name} from_reader");

    let node = yaml::parse_str(input).unwrap_or_else(|error| panic!("{name} parse node: {error}"));
    let from_node =
        yaml::from_node::<T>(&node).unwrap_or_else(|error| panic!("{name} from_node: {error}"));
    assert_eq!(from_node, expected, "{name} from_node");

    let value =
        yaml::from_str::<Value>(input).unwrap_or_else(|error| panic!("{name} value: {error}"));
    let from_value = yaml::from_value::<T>(value.clone())
        .unwrap_or_else(|error| panic!("{name} from_value: {error}"));
    assert_eq!(from_value, expected, "{name} from_value");

    let from_value_ref = T::deserialize(&value)
        .unwrap_or_else(|error| panic!("{name} deserialize from &Value: {error}"));
    assert_eq!(from_value_ref, expected, "{name} &Value");

    let document = yaml::Deserializer::from_str(input)
        .next()
        .unwrap_or_else(|| panic!("{name} Deserializer::from_str doc"));
    let direct_str = T::deserialize(document)
        .unwrap_or_else(|error| panic!("{name} direct str deserializer: {error}"));
    assert_eq!(direct_str, expected, "{name} direct str");

    let document = yaml::Deserializer::from_slice(input.as_bytes())
        .next()
        .unwrap_or_else(|| panic!("{name} Deserializer::from_slice doc"));
    let direct_slice = T::deserialize(document)
        .unwrap_or_else(|error| panic!("{name} direct slice deserializer: {error}"));
    assert_eq!(direct_slice, expected, "{name} direct slice");

    let document = yaml::Deserializer::from_reader(Cursor::new(input.as_bytes()))
        .next()
        .unwrap_or_else(|| panic!("{name} Deserializer::from_reader doc"));
    let direct_reader = T::deserialize(document)
        .unwrap_or_else(|error| panic!("{name} direct reader deserializer: {error}"));
    assert_eq!(direct_reader, expected, "{name} direct reader");

    for (surface, docs) in [
        ("from_documents_str", yaml::from_documents_str::<T>(input)),
        (
            "from_documents_slice",
            yaml::from_documents_slice::<T>(input.as_bytes()),
        ),
        (
            "from_documents_reader",
            yaml::from_documents_reader::<T, _>(Cursor::new(input.as_bytes())),
        ),
    ] {
        let docs = docs.unwrap_or_else(|error| panic!("{name} {surface}: {error}"));
        assert_eq!(docs, vec![expected.clone()], "{name} {surface}");
    }
}

#[test]
fn serde_api_tags_are_transparent_for_borrowed_retained_reads() {
    let input = "name: !Env borrowed\npath: !Path /srv/tagged\n";
    let node = yaml::parse_str(input).expect("parse node");
    let from_node: TaggedBorrowedConfig<'_> = yaml::from_node(&node).expect("from_node");
    let document = yaml::Deserializer::from_str(input)
        .next()
        .expect("document");
    let from_document: TaggedBorrowedConfig<'_> =
        TaggedBorrowedConfig::deserialize(document).expect("document deserialize");
    let value: Value = yaml::from_str(input).expect("value");
    let from_value_ref = TaggedBorrowedConfig::deserialize(&value).expect("value ref deserialize");

    let expected = TaggedBorrowedConfig {
        name: "borrowed",
        path: "/srv/tagged",
    };
    assert_eq!(from_node, expected);
    assert_eq!(from_document, expected);
    assert_eq!(from_value_ref, expected);
}

#[test]
fn serde_api_verbatim_tags_preserve_suffix_and_are_transparent() {
    let input = "value: !<tag:example.com,2026:Thing> tagged\n";
    let value: Value = yaml::from_str(input).expect("tagged value");
    let tagged = value["value"].as_tagged().expect("verbatim tag");

    assert_eq!(tagged.tag.handle, "!");
    assert_eq!(tagged.tag.suffix, "tag:example.com,2026:Thing");
    assert_eq!(tagged.value.as_str(), Some("tagged"));

    let typed: BTreeMap<String, String> = yaml::from_str(input).expect("typed map");
    assert_eq!(typed["value"], "tagged");
}

#[test]
fn serde_api_yaml11_timestamps_have_native_typed_reads_and_string_transparency() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct TaggedScalars {
        date: String,
        datetime: String,
        explicit: String,
        payload: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TimestampScalars {
        date: yaml::Timestamp,
        datetime: yaml::Timestamp,
        explicit: yaml::Timestamp,
    }

    let input = "\
date: 2026-05-24
datetime: 2026-05-24T12:34:56Z
explicit: !!timestamp 2026-05-25
payload: !!binary SGVsbG8=
";
    let value: Value = LoadOptions::yaml_1_1()
        .from_str(input)
        .expect("YAML 1.1 tagged scalar values");

    for (key, tag, source) in [
        ("date", "!!timestamp", "2026-05-24"),
        ("datetime", "!!timestamp", "2026-05-24T12:34:56Z"),
        ("explicit", "!!timestamp", "2026-05-25"),
        ("payload", "!!binary", "SGVsbG8="),
    ] {
        assert_eq!(value[key].as_str(), Some(source));
        let tagged = value[key].as_tagged().expect("retained scalar tag");
        assert_eq!(tagged.tag, Tag::new(tag));
        assert_eq!(tagged.value.as_str(), Some(source));
    }
    assert_eq!(
        value["date"].as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-24")
    );
    assert_eq!(
        value["datetime"].as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-24T12:34:56Z")
    );
    assert_eq!(
        value["explicit"].as_timestamp(),
        yaml::Timestamp::parse_yaml_1_1("2026-05-25")
    );
    assert!(value["payload"].as_timestamp().is_none());

    let expected = TaggedScalars {
        date: "2026-05-24".to_string(),
        datetime: "2026-05-24T12:34:56Z".to_string(),
        explicit: "2026-05-25".to_string(),
        payload: "SGVsbG8=".to_string(),
    };
    let typed: TaggedScalars = LoadOptions::yaml_1_1()
        .from_str(input)
        .expect("typed strings from YAML 1.1 schema");
    let direct: TaggedScalars =
        TaggedScalars::deserialize(LoadOptions::yaml_1_1().deserializer_from_str(input))
            .expect("direct deserializer typed strings");
    let from_value: TaggedScalars =
        yaml::from_value(value.clone()).expect("owned value typed strings");
    let from_value_ref: TaggedScalars =
        TaggedScalars::deserialize(&value).expect("borrowed value typed strings");

    assert_eq!(typed, expected);
    assert_eq!(direct, expected);
    assert_eq!(from_value, expected);
    assert_eq!(from_value_ref, expected);

    let expected_timestamps = TimestampScalars {
        date: yaml::Timestamp::parse_yaml_1_1("2026-05-24").expect("date timestamp"),
        datetime: yaml::Timestamp::parse_yaml_1_1("2026-05-24T12:34:56Z")
            .expect("datetime timestamp"),
        explicit: yaml::Timestamp::parse_yaml_1_1("2026-05-25").expect("explicit timestamp"),
    };
    let typed_timestamps: TimestampScalars = LoadOptions::yaml_1_1()
        .from_str(input)
        .expect("typed timestamps from YAML 1.1 schema");
    let direct_timestamps: TimestampScalars =
        TimestampScalars::deserialize(LoadOptions::yaml_1_1().deserializer_from_str(input))
            .expect("direct deserializer typed timestamps");
    let from_value_timestamps: TimestampScalars =
        yaml::from_value(value.clone()).expect("owned value typed timestamps");
    let from_value_ref_timestamps: TimestampScalars =
        TimestampScalars::deserialize(&value).expect("borrowed value typed timestamps");

    assert_eq!(typed_timestamps, expected_timestamps);
    assert_eq!(direct_timestamps, expected_timestamps);
    assert_eq!(from_value_timestamps, expected_timestamps);
    assert_eq!(from_value_ref_timestamps, expected_timestamps);
}

#[test]
fn serde_api_string_targets_match_serde_yaml_null_like_scalars() {
    let input = include_str!("fixtures/divergences/null-like-string-targets.yaml");

    let parsed: BTreeMap<String, String> = yaml::from_str(input).expect("typed string map");
    let reference: BTreeMap<String, String> =
        serde_yaml::from_str(input).expect("serde_yaml typed string map");
    assert_eq!(parsed, reference);

    let optional: BTreeMap<String, Option<String>> =
        yaml::from_str(input).expect("typed option string map");
    assert_eq!(optional["EMPTY"], None);
    assert_eq!(optional["TILDE"], None);
    assert_eq!(optional["NULL_LOWER"], None);
    assert_eq!(optional["NULL_UPPER"], None);
    assert_eq!(optional["NORMAL"], Some("value".to_string()));

    let value: Value = yaml::from_str(input).expect("value map");
    assert!(value["EMPTY"].is_null());
    assert!(value["TILDE"].is_null());
    assert!(value["NULL_LOWER"].is_null());
    assert!(value["NULL_UPPER"].is_null());
    assert_eq!(value["NORMAL"].as_str(), Some("value"));
}

#[test]
fn serde_api_string_targets_preserve_plain_scalar_source_for_typed_reads() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct SourceBackedStringTargets {
        responses: BTreeMap<String, String>,
        vars: BTreeMap<String, String>,
    }

    let input = include_str!("fixtures/divergences/source-backed-string-targets.yaml");

    let parsed: SourceBackedStringTargets = yaml::from_str(input).expect("typed string targets");
    let parsed_from_slice: SourceBackedStringTargets =
        yaml::from_slice(input.as_bytes()).expect("typed string targets from slice");
    let parsed_direct_slice: SourceBackedStringTargets =
        SourceBackedStringTargets::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("typed string targets from direct slice deserializer");
    let reference: SourceBackedStringTargets =
        serde_yaml::from_str(input).expect("serde_yaml typed string targets");
    assert_eq!(parsed, reference);
    assert_eq!(parsed_from_slice, reference);
    assert_eq!(parsed_direct_slice, reference);
    assert_eq!(parsed.responses["200"], "ok");
    assert_eq!(parsed.responses["404"], "missing");
    assert_eq!(parsed.vars["BOOL_TRUE"], "true");
    assert_eq!(parsed.vars["BOOL_FALSE"], "FALSE");
    assert_eq!(parsed.vars["INT"], "1_000");
    assert_eq!(parsed.vars["FLOAT"], "1.5");
    assert_eq!(parsed.vars["LEADING_ZERO"], "0123");
    assert_eq!(parsed.vars["CPU"], "100m");

    let value: Value = yaml::from_str(input).expect("value map");
    let error = yaml::from_value::<SourceBackedStringTargets>(value)
        .expect_err("spanless Value does not carry source scalar spelling");
    let display = error.to_string();
    assert!(display.contains("expected string"));
    assert_eq!(error.location(), None);
    assert!(!display.contains("line 0"));
}

#[test]
fn serde_api_stream_document_deserializer_preserves_type_error_span() {
    let input = "---\nname: app\nports: no\nenabled: true\n";
    let error = yaml::Deserializer::from_str(input)
        .map(Config::deserialize)
        .next()
        .expect("one document")
        .expect_err("type error");
    assert!(error.to_string().contains("expected sequence"));
    assert!(error.to_string().contains("line 3, column 8"));
}

#[test]
fn serde_api_numeric_range_errors_preserve_scalar_span() {
    let error = yaml::from_str::<StreamPortConfig>("port: 70000\n").expect_err("u16 range error");
    assert!(error.to_string().contains("invalid value"));
    assert_eq!(error.line(), Some(1));
    assert_eq!(error.column(), Some(7));
    assert_eq!(error.location().expect("location").index(), 6);

    let sequence_error = yaml::from_str::<Vec<u8>>("[1, 300]\n").expect_err("u8 range error");
    assert!(sequence_error.to_string().contains("invalid value"));
    assert_eq!(sequence_error.line(), Some(1));
    assert_eq!(sequence_error.column(), Some(5));
    assert_eq!(sequence_error.location().expect("location").index(), 4);

    let direct_error = yaml::from_str::<u16>("70000\n").expect_err("top-level u16 range error");
    assert!(direct_error.to_string().contains("invalid value"));
    assert_eq!(direct_error.line(), Some(1));
    assert_eq!(direct_error.column(), Some(1));
    assert_eq!(direct_error.location().expect("location").index(), 0);
}

#[test]
fn serde_api_value_can_drive_deserialize_by_reference() {
    let value: Value = yaml::from_str("name: app\nports: [80]\nenabled: true\n").expect("value");
    let config = Config::deserialize(&value).expect("deserialize from &Value");
    assert_eq!(config.name, "app");
    assert_eq!(config.ports, [80]);
    assert!(config.enabled);
}

#[test]
fn serde_api_from_node_supports_borrowed_config_fields() {
    let node = yaml::parse_str("name: node\npath: /srv/node\n").expect("node");
    let config: BorrowedConfig<'_> = yaml::from_node(&node).expect("borrowed config");

    assert_eq!(
        config,
        BorrowedConfig {
            name: "node",
            path: "/srv/node",
        }
    );
}

#[test]
fn serde_api_stream_document_deserializer_supports_borrowed_config_fields() {
    let input = "name: doc\npath: /srv/doc\n";
    let config: BorrowedConfig<'_> = {
        let document = yaml::Deserializer::from_str(input)
            .next()
            .expect("document");
        BorrowedConfig::deserialize(document).expect("borrowed config")
    };

    assert_eq!(
        config,
        BorrowedConfig {
            name: "doc",
            path: "/srv/doc",
        }
    );
    assert_borrowed_from(input, config.name);
    assert_borrowed_from(input, config.path);
}

#[test]
fn serde_api_from_str_and_from_slice_support_borrowed_config_fields() {
    let input = "name: app\npath: /srv/app\n";

    let from_str: BorrowedConfig<'_> = yaml::from_str(input).expect("borrowed from_str");
    let from_slice: BorrowedConfig<'_> =
        yaml::from_slice(input.as_bytes()).expect("borrowed from_slice");

    assert_eq!(
        from_str,
        BorrowedConfig {
            name: "app",
            path: "/srv/app",
        }
    );
    assert_eq!(from_slice, from_str);
    assert_borrowed_from(input, from_str.name);
    assert_borrowed_from(input, from_str.path);
    assert_borrowed_from(input, from_slice.name);
    assert_borrowed_from(input, from_slice.path);
}

#[test]
fn serde_api_deserializer_from_str_and_from_slice_borrow_from_source() {
    let input = "name: app\npath: /srv/app\n";

    let from_str: BorrowedConfig<'_> =
        BorrowedConfig::deserialize(yaml::Deserializer::from_str(input))
            .expect("borrowed direct deserializer from_str");
    let from_slice: BorrowedConfig<'_> =
        BorrowedConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("borrowed direct deserializer from_slice");
    let reference: BorrowedConfig<'_> =
        BorrowedConfig::deserialize(serde_yaml::Deserializer::from_str(input))
            .expect("serde_yaml borrowed direct deserializer");

    assert_eq!(from_str, reference);
    assert_eq!(from_slice, reference);
    assert_borrowed_from(input, from_str.name);
    assert_borrowed_from(input, from_str.path);
    assert_borrowed_from(input, from_slice.name);
    assert_borrowed_from(input, from_slice.path);
}

#[test]
fn serde_api_deserializer_stream_documents_borrow_from_source() {
    let input = "---\nname: first\npath: /srv/first\n---\nname: second\npath: /srv/second\n";

    let parsed = yaml::Deserializer::from_str(input)
        .map(BorrowedConfig::deserialize)
        .collect::<Result<Vec<BorrowedConfig<'_>>, _>>()
        .expect("borrowed config stream");
    let reference = serde_yaml::Deserializer::from_str(input)
        .map(BorrowedConfig::deserialize)
        .collect::<Result<Vec<BorrowedConfig<'_>>, _>>()
        .expect("serde_yaml borrowed config stream");

    assert_eq!(parsed, reference);
    assert_eq!(parsed.len(), 2);
    assert_borrowed_from(input, parsed[0].name);
    assert_borrowed_from(input, parsed[0].path);
    assert_borrowed_from(input, parsed[1].name);
    assert_borrowed_from(input, parsed[1].path);
}

#[test]
fn serde_api_deserializer_borrows_simple_quoted_and_non_ascii_scalars() {
    let input = "name: \"cafe\"\npath: '/srv/app'\n";
    let quoted: BorrowedConfig<'_> =
        BorrowedConfig::deserialize(yaml::Deserializer::from_str(input))
            .expect("borrowed simple quoted direct deserializer");
    assert_eq!(quoted.name, "cafe");
    assert_eq!(quoted.path, "/srv/app");
    assert_borrowed_from(input, quoted.name);
    assert_borrowed_from(input, quoted.path);

    let input = "name: café\npath: /srv/😀\n";
    let non_ascii: BorrowedConfig<'_> =
        BorrowedConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("borrowed non-ASCII direct deserializer");
    assert_eq!(non_ascii.name, "café");
    assert_eq!(non_ascii.path, "/srv/😀");
    assert_borrowed_from(input, non_ascii.name);
    assert_borrowed_from(input, non_ascii.path);
}

#[test]
fn serde_api_deserializer_borrows_source_backed_string_targets() {
    let input = include_str!("fixtures/divergences/source-backed-string-targets.yaml");
    let parsed: BorrowedScalarTargets<'_> =
        BorrowedScalarTargets::deserialize(yaml::Deserializer::from_str(input))
            .expect("borrowed source-backed direct deserializer");
    let parsed_from_slice: BorrowedScalarTargets<'_> =
        BorrowedScalarTargets::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
            .expect("borrowed source-backed direct slice deserializer");
    let reference: BTreeMap<String, BTreeMap<String, String>> =
        serde_yaml::from_str(input).expect("serde_yaml source-backed string targets");

    assert_eq!(parsed.responses["200"], reference["responses"]["200"]);
    assert_eq!(parsed.responses["404"], reference["responses"]["404"]);
    assert_eq!(parsed.vars["BOOL_TRUE"], "true");
    assert_eq!(parsed.vars["INT"], "1_000");
    assert_eq!(parsed.vars["FLOAT"], "1.5");
    assert_eq!(
        parsed_from_slice.responses["200"],
        reference["responses"]["200"]
    );
    assert_eq!(
        parsed_from_slice.responses["404"],
        reference["responses"]["404"]
    );
    assert_eq!(parsed_from_slice.vars["BOOL_TRUE"], "true");
    assert_eq!(parsed_from_slice.vars["INT"], "1_000");
    assert_eq!(parsed_from_slice.vars["FLOAT"], "1.5");
    assert_borrowed_from(
        input,
        parsed.responses.keys().next().expect("first response key"),
    );
    assert_borrowed_from(input, parsed.vars["BOOL_TRUE"]);
    assert_borrowed_from(input, parsed.vars["INT"]);
    assert_borrowed_from(input, parsed.vars["FLOAT"]);
    assert_borrowed_from(
        input,
        parsed_from_slice
            .responses
            .keys()
            .next()
            .expect("first response key from slice"),
    );
    assert_borrowed_from(input, parsed_from_slice.vars["BOOL_TRUE"]);
    assert_borrowed_from(input, parsed_from_slice.vars["INT"]);
    assert_borrowed_from(input, parsed_from_slice.vars["FLOAT"]);
}

#[test]
fn serde_api_deserializer_reader_remains_owned_for_borrowed_targets() {
    let input = "name: app\npath: /srv/app\n";
    let error = BorrowedConfig::deserialize(yaml::Deserializer::from_reader(Cursor::new(
        input.as_bytes(),
    )))
    .expect_err("reader deserializer cannot return borrowed input fields");

    assert!(error.to_string().contains("borrowed"));
}

#[test]
fn serde_api_from_str_borrows_plain_scalar_map_keys_and_source_backed_values() {
    let input = include_str!("fixtures/divergences/source-backed-string-targets.yaml");
    let parsed: BorrowedScalarTargets<'_> =
        yaml::from_str(input).expect("borrowed source-backed string targets");
    let parsed_from_slice: BorrowedScalarTargets<'_> = yaml::from_slice(input.as_bytes())
        .expect("borrowed source-backed string targets from slice");
    let reference: BTreeMap<String, BTreeMap<String, String>> =
        serde_yaml::from_str(input).expect("serde_yaml source-backed string targets");

    assert_eq!(parsed.responses["200"], reference["responses"]["200"]);
    assert_eq!(parsed.responses["404"], reference["responses"]["404"]);
    assert_eq!(parsed.vars["BOOL_TRUE"], "true");
    assert_eq!(parsed.vars["BOOL_FALSE"], "FALSE");
    assert_eq!(parsed.vars["INT"], "1_000");
    assert_eq!(parsed.vars["FLOAT"], "1.5");
    assert_eq!(parsed.vars["LEADING_ZERO"], "0123");
    assert_eq!(parsed.vars["CPU"], "100m");
    assert_eq!(
        parsed_from_slice.responses["200"],
        reference["responses"]["200"]
    );
    assert_eq!(
        parsed_from_slice.responses["404"],
        reference["responses"]["404"]
    );
    assert_eq!(parsed_from_slice.vars["BOOL_TRUE"], "true");
    assert_eq!(parsed_from_slice.vars["BOOL_FALSE"], "FALSE");
    assert_eq!(parsed_from_slice.vars["INT"], "1_000");
    assert_eq!(parsed_from_slice.vars["FLOAT"], "1.5");
    assert_eq!(parsed_from_slice.vars["LEADING_ZERO"], "0123");
    assert_eq!(parsed_from_slice.vars["CPU"], "100m");
    assert_borrowed_from(
        input,
        parsed.responses.keys().next().expect("first response key"),
    );
    assert_borrowed_from(input, parsed.vars["BOOL_TRUE"]);
    assert_borrowed_from(input, parsed.vars["INT"]);
    assert_borrowed_from(
        input,
        parsed_from_slice
            .responses
            .keys()
            .next()
            .expect("first response key from slice"),
    );
    assert_borrowed_from(input, parsed_from_slice.vars["BOOL_TRUE"]);
    assert_borrowed_from(input, parsed_from_slice.vars["INT"]);
}

#[test]
fn serde_api_borrowed_entrypoints_match_serde_yaml_for_transformed_scalars() {
    for (name, input) in [
        ("double-quoted-escape", "value: \"line\\n\"\n"),
        ("literal-block", "value: |\n  line\n"),
        ("folded-block", "value: >\n  first\n  second\n"),
    ] {
        let ours = yaml::from_str::<BorrowedValue<'_>>(input).map(|value| value.value.to_string());
        let reference =
            serde_yaml::from_str::<BorrowedValue<'_>>(input).map(|value| value.value.to_string());
        assert_eq!(
            ours.is_ok(),
            reference.is_ok(),
            "{name} borrowed &str behavior should match serde_yaml"
        );

        let ours_cow: CowValue<'_> = yaml::from_str(input).expect("Cow value");
        let reference_cow: CowValue<'_> =
            serde_yaml::from_str(input).expect("serde_yaml Cow value");
        assert_eq!(ours_cow, reference_cow, "{name} Cow value");
    }
}

#[test]
fn serde_api_borrowed_entrypoint_errors_preserve_spans() {
    let unknown = yaml::from_str::<StrictConfig>("name: app\nextra: true\n")
        .expect_err("unknown field error");
    assert!(unknown.to_string().contains("unknown field `extra`"));
    assert_eq!(unknown.line(), Some(2));
    assert_eq!(unknown.column(), Some(1));

    let range = yaml::from_str::<StreamPortConfig>("port: 70000\n").expect_err("range error");
    assert!(range.to_string().contains("invalid value"));
    assert_eq!(range.line(), Some(1));
    assert_eq!(range.column(), Some(7));

    let direct_unknown =
        StrictConfig::deserialize(yaml::Deserializer::from_str("name: app\nextra: true\n"))
            .expect_err("direct deserializer unknown field error");
    assert!(direct_unknown.to_string().contains("unknown field `extra`"));
    assert!(direct_unknown.to_string().contains("line 2, column 1"));

    let direct_range = StreamPortConfig::deserialize(yaml::Deserializer::from_str("port: 70000\n"))
        .expect_err("direct deserializer range error");
    assert!(direct_range.to_string().contains("invalid value"));
    assert!(direct_range.to_string().contains("line 1, column 7"));

    let mut invalid = b"name: app\npath: /srv/app\n".to_vec();
    invalid.push(0xFF);
    let utf8 = yaml::from_slice::<BorrowedConfig<'_>>(&invalid).expect_err("invalid UTF-8");
    assert_eq!(utf8.line(), Some(3));
    assert_eq!(utf8.column(), Some(1));
    assert_eq!(
        utf8.location().expect("location").index(),
        invalid.len() - 1
    );

    let direct_utf8 = BorrowedConfig::deserialize(yaml::Deserializer::from_slice(&invalid))
        .expect_err("direct deserializer invalid UTF-8");
    assert!(direct_utf8.to_string().contains("input is not valid UTF-8"));
    assert!(direct_utf8.to_string().contains("line 3, column 1"));
}

#[test]
fn serde_api_value_reference_deserializer_borrows_strings() {
    let value: Value = yaml::from_str("name: value\npath: /srv/value\n").expect("value");
    let source_name = value["name"].as_str().expect("name string");
    let source_path = value["path"].as_str().expect("path string");

    let config = BorrowedConfig::deserialize(&value).expect("borrowed value config");

    assert_eq!(config.name, "value");
    assert_eq!(config.path, "/srv/value");
    assert_eq!(config.name.as_ptr(), source_name.as_ptr());
    assert_eq!(config.path.as_ptr(), source_path.as_ptr());
}

#[test]
fn serde_api_unknown_field_error_uses_key_span() {
    let node = yaml::parse_str("name: app\nextra: true\n").expect("node");
    let error = yaml::from_node::<StrictConfig>(&node).expect_err("unknown field");

    assert!(error.to_string().contains("unknown field `extra`"));
    assert_eq!(error.line(), Some(2));
    assert_eq!(error.column(), Some(1));
}

#[test]
fn serde_api_value_index_missing_returns_null_sentinel() {
    let value: Value = yaml::from_str("name: app\nitems: [one]\n").expect("value");
    assert!(value["missing"].is_null());
    assert!(value["items"][10].is_null());
    assert!(value[0].is_null());
}

#[test]
fn serde_api_value_reads_expand_merge_keys_by_default() {
    let input = "\
defaults: &defaults
  retries: 3
  command: deploy
job:
  <<: *defaults
  command: smoke
";

    let mut value: Value = yaml::from_str(input).expect("value with default merge");
    assert!(value["job"]["<<"].is_null());
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));

    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml value with literal merge");
    reference.apply_merge().expect("serde_yaml apply merge");
    value.apply_merge().expect("yaml apply merge");

    assert!(value["job"]["<<"].is_null());
    assert_eq!(
        value["job"]["retries"].as_u64(),
        reference["job"]["retries"].as_u64()
    );
    assert_eq!(
        value["job"]["command"].as_str(),
        reference["job"]["command"].as_str()
    );
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));
    assert_eq!(value["job"]["command"].as_str(), Some("smoke"));
}

#[test]
fn serde_api_value_apply_merge_matches_serde_yaml_merge_list_order() {
    let input = "\
base1: &base1 {a: 1, shared: first}
base2: &base2 {b: 2, c: 2, shared: second}
merged:
  <<: [*base1, *base2]
  b: explicit
";

    let mut value: Value = yaml::from_str(input).expect("value with merge list");
    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml value with merge list");

    value.apply_merge().expect("yaml apply merge list");
    reference
        .apply_merge()
        .expect("serde_yaml apply merge list");

    assert!(value["merged"]["<<"].is_null());
    assert_eq!(
        value["merged"]["a"].as_u64(),
        reference["merged"]["a"].as_u64()
    );
    assert_eq!(
        value["merged"]["b"].as_str(),
        reference["merged"]["b"].as_str()
    );
    assert_eq!(
        value["merged"]["c"].as_u64(),
        reference["merged"]["c"].as_u64()
    );
    assert_eq!(
        value["merged"]["shared"].as_str(),
        reference["merged"]["shared"].as_str()
    );
    assert_eq!(value["merged"]["shared"].as_str(), Some("first"));
    assert_eq!(value["merged"]["b"].as_str(), Some("explicit"));
}

#[test]
fn serde_api_value_apply_merge_recurses_through_sequences_and_tagged_values() {
    let input = "\
defaults: &defaults {retries: 3, timeout: 10}
jobs:
  - name: build
    config:
      <<: *defaults
      timeout: 20
tagged: !Job
  <<: *defaults
  timeout: 30
";

    let mut value: Value = yaml::from_str(input).expect("nested merge value");
    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml nested merge value");

    value.apply_merge().expect("yaml recursive merge");
    reference.apply_merge().expect("serde_yaml recursive merge");

    assert!(value["jobs"][0]["config"]["<<"].is_null());
    assert_eq!(
        value["jobs"][0]["config"]["retries"].as_u64(),
        reference["jobs"][0]["config"]["retries"].as_u64()
    );
    assert_eq!(
        value["jobs"][0]["config"]["timeout"].as_u64(),
        reference["jobs"][0]["config"]["timeout"].as_u64()
    );
    assert!(value["tagged"]["<<"].is_null());
    assert_eq!(
        value["tagged"]["retries"].as_u64(),
        reference["tagged"]["retries"].as_u64()
    );
    assert_eq!(value["jobs"][0]["config"]["timeout"].as_u64(), Some(20));
    assert_eq!(value["tagged"]["timeout"].as_u64(), Some(30));
}

#[test]
fn serde_api_value_apply_merge_handles_quoted_and_tagged_merge_keys_like_serde_yaml() {
    let quoted_input = "\
base: &base {a: 1}
target:
  '<<': *base
";
    let mut quoted: Value = yaml::from_str(quoted_input).expect("quoted merge key value");
    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(quoted_input).expect("serde_yaml quoted merge key value");

    quoted.apply_merge().expect("yaml quoted merge key");
    reference
        .apply_merge()
        .expect("serde_yaml quoted merge key");

    assert!(quoted["target"]["<<"].is_null());
    assert_eq!(
        quoted["target"]["a"].as_u64(),
        reference["target"]["a"].as_u64()
    );

    let tagged_key_input = "target: {!Thing <<: tagged, plain: value}\n";
    let mut tagged_key: Value = yaml::from_str(tagged_key_input).expect("tagged key value");
    let mut tagged_reference: serde_yaml::Value =
        serde_yaml::from_str(tagged_key_input).expect("serde_yaml tagged key value");

    tagged_key.apply_merge().expect("yaml tagged merge key");
    tagged_reference
        .apply_merge()
        .expect("serde_yaml tagged merge key");

    assert_eq!(tagged_reference["target"]["plain"].as_str(), Some("value"));
    assert_eq!(tagged_key["target"]["plain"].as_str(), Some("value"));
    let target = tagged_key["target"].as_mapping().expect("target mapping");
    assert!(
        target.keys().any(|key| matches!(key, Value::Tagged(tagged)
            if tagged.value.as_str() == Some("<<"))),
        "tagged << key must stay literal"
    );
}

#[test]
fn serde_api_default_merge_pre_expands_merge_source_mappings() {
    let input = "\
base: &base
  <<: {a: 1}
  b: 2
target:
  <<: *base
";

    let mut value: Value = yaml::from_str(input).expect("nested merge source value");

    assert_eq!(value["base"]["a"].as_u64(), Some(1));
    assert!(value["base"]["<<"].is_null());
    assert_eq!(value["target"]["a"].as_u64(), Some(1));
    assert_eq!(value["target"]["b"].as_u64(), Some(2));
    assert!(value["target"]["<<"].is_null());

    let before = value.clone();
    value
        .apply_merge()
        .expect("default nested merge expansion is idempotent");
    assert!(value.equivalent(&before));
}

#[test]
fn serde_api_value_apply_merge_reports_invalid_merge_payloads() {
    let cases = [
        (
            "item: {<<: scalar}\n",
            "expected a mapping or list of mappings for merging, but found scalar",
        ),
        (
            "item: {<<: [scalar]}\n",
            "expected a mapping for merging, but found scalar",
        ),
        (
            "item: {<<: [[]]}\n",
            "expected a mapping for merging, but found sequence",
        ),
        (
            "item: {<<: !Thing {a: b}}\n",
            "unexpected tagged value in merge",
        ),
    ];

    for (input, expected) in cases {
        let error = yaml::from_str::<Value>(input).expect_err("invalid default merge payload");
        assert!(error.location().is_some());
        assert!(
            error.to_string().contains(expected),
            "yaml error `{error}` should contain `{expected}`"
        );

        let mut reference: serde_yaml::Value =
            serde_yaml::from_str(input).expect("serde_yaml invalid merge payload parses");
        let reference_error = reference.apply_merge().expect_err("serde_yaml merge error");
        assert!(reference_error.location().is_none());
        assert!(
            reference_error.to_string().contains(expected),
            "serde_yaml error `{reference_error}` should contain `{expected}`"
        );
    }
}

#[test]
fn serde_api_unsigned_numbers_above_i64_are_supported() {
    let max = u64::MAX;
    let value: Value = yaml::from_str(&format!("id: {max}\n")).expect("value");
    let number = match &value["id"] {
        Value::Number(number) => *number,
        other => panic!("expected number, got {other:?}"),
    };
    assert!(matches!(number, Number::Unsigned(value) if value == u128::from(max)));
    assert!(number.is_u64());
    assert!(!number.is_i64());
    assert_eq!(number.as_u64(), Some(max));
    assert_eq!(number.as_u128(), Some(u128::from(max)));
    assert_eq!(value["id"].as_u64(), Some(max));
    assert_eq!(value["id"].as_u128(), Some(u128::from(max)));
    assert!(value["id"].as_i64().is_none());

    let typed: BTreeMap<String, u64> = yaml::from_str(&format!("id: {max}\n")).expect("typed u64");
    assert_eq!(typed["id"], max);
}

#[test]
fn serde_api_value_numeric_predicates_match_serde_yaml() {
    let input = "small: 42\nsigned: -7\nlarge: 9223372036854775817\nfloat: 1.5\nstring: \"2\"\n";
    let value: Value = yaml::from_str(input).expect("yaml value");
    let reference: serde_yaml::Value = serde_yaml::from_str(input).expect("serde_yaml value");

    for key in ["small", "signed", "large", "float", "string"] {
        assert_eq!(value[key].is_i64(), reference[key].is_i64(), "{key} is_i64");
        assert_eq!(value[key].is_u64(), reference[key].is_u64(), "{key} is_u64");
        assert_eq!(value[key].is_f64(), reference[key].is_f64(), "{key} is_f64");
        assert_eq!(value[key].as_i64(), reference[key].as_i64(), "{key} as_i64");
        assert_eq!(value[key].as_u64(), reference[key].as_u64(), "{key} as_u64");
        assert_eq!(value[key].as_f64(), reference[key].as_f64(), "{key} as_f64");
    }

    let tagged = Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new("!Port"),
        value: Value::from(8080u64),
    }));
    assert!(tagged.is_u64());
    assert!(tagged.is_i64());
    assert!(!tagged.is_f64());
    assert_eq!(tagged.as_u64(), Some(8080));
}

#[test]
fn serde_api_number_public_helpers_match_serde_yaml() {
    for (ours, reference) in [
        (Number::from(42i64), serde_yaml::Number::from(42i64)),
        (Number::from(-7i64), serde_yaml::Number::from(-7i64)),
        (Number::from(u64::MAX), serde_yaml::Number::from(u64::MAX)),
        (Number::from(1.25f64), serde_yaml::Number::from(1.25f64)),
        (
            Number::from(f64::INFINITY),
            serde_yaml::Number::from(f64::INFINITY),
        ),
        (
            Number::from(f64::NEG_INFINITY),
            serde_yaml::Number::from(f64::NEG_INFINITY),
        ),
    ] {
        assert_eq!(ours.is_i64(), reference.is_i64(), "{ours}");
        assert_eq!(ours.is_u64(), reference.is_u64(), "{ours}");
        assert_eq!(ours.is_f64(), reference.is_f64(), "{ours}");
        assert_eq!(ours.as_i64(), reference.as_i64(), "{ours}");
        assert_eq!(ours.as_u64(), reference.as_u64(), "{ours}");
        assert_eq!(ours.as_f64(), reference.as_f64(), "{ours}");
        assert_eq!(ours.is_nan(), reference.is_nan(), "{ours}");
        assert_eq!(ours.is_infinite(), reference.is_infinite(), "{ours}");
        assert_eq!(ours.is_finite(), reference.is_finite(), "{ours}");
        assert_eq!(ours.to_string(), reference.to_string(), "{ours}");
    }

    let ours_nan = Number::from(f64::NAN);
    let reference_nan = serde_yaml::Number::from(f64::NAN);
    assert!(ours_nan.is_nan());
    assert_eq!(ours_nan.is_nan(), reference_nan.is_nan());
    assert_eq!(ours_nan.is_finite(), reference_nan.is_finite());
    assert_eq!(ours_nan.to_string(), reference_nan.to_string());
    assert_eq!(ours_nan, Number::from(f64::NAN));

    for repr in [
        "42",
        "-7",
        "18446744073709551615",
        "1.25",
        ".nan",
        ".inf",
        "+.inf",
        "-.inf",
    ] {
        let ours: Number = repr.parse().unwrap_or_else(|error| {
            panic!("yaml Number parses {repr}: {error}");
        });
        let reference: serde_yaml::Number = repr.parse().unwrap_or_else(|error| {
            panic!("serde_yaml Number parses {repr}: {error}");
        });
        assert_eq!(ours.to_string(), reference.to_string(), "{repr}");
        assert_eq!(ours.is_i64(), reference.is_i64(), "{repr}");
        assert_eq!(ours.is_u64(), reference.is_u64(), "{repr}");
        assert_eq!(ours.is_f64(), reference.is_f64(), "{repr}");
    }
    assert!("not-a-number".parse::<Number>().is_err());
}

#[test]
fn serde_api_value_from_impls_match_serde_yaml_construction() {
    assert!(matches!(Value::from(true), Value::Bool(true)));
    assert_eq!(Value::from("text").as_str(), Some("text"));
    assert_eq!(Value::from(String::from("owned")).as_str(), Some("owned"));
    assert_eq!(
        Value::from(Cow::Borrowed("borrowed")).as_str(),
        Some("borrowed")
    );
    assert_eq!(Value::from(42i64).as_i64(), Some(42));
    assert_eq!(Value::from(u64::MAX).as_u64(), Some(u64::MAX));
    assert_eq!(Value::from(1.5f64).as_f64(), Some(1.5));

    let from_vec = Value::from(vec!["lorem", "ipsum", "dolor"]);
    let reference_from_vec = serde_yaml::Value::from(vec!["lorem", "ipsum", "dolor"]);
    assert_eq!(from_vec.as_sequence().map(Vec::len), Some(3));
    assert_eq!(reference_from_vec.as_sequence().map(Vec::len), Some(3));
    assert_eq!(from_vec[1].as_str(), reference_from_vec[1].as_str());

    let slice: &[&str] = &["alpha", "beta"];
    let from_slice = Value::from(slice);
    let reference_from_slice = serde_yaml::Value::from(slice);
    assert_eq!(from_slice.as_sequence().map(Vec::len), Some(2));
    assert_eq!(reference_from_slice.as_sequence().map(Vec::len), Some(2));
    assert_eq!(from_slice[0].as_str(), reference_from_slice[0].as_str());

    let collected: Value = [1u64, 2, 3].into_iter().collect();
    assert_eq!(collected.as_sequence().map(Vec::len), Some(3));
    assert_eq!(collected[2].as_u64(), Some(3));

    let mut mapping = Mapping::new();
    mapping.insert("name".into(), "app".into());
    let value = Value::from(mapping);
    assert_eq!(value["name"].as_str(), Some("app"));
}

#[test]
fn serde_api_value_into_deserializer_matches_serde_yaml() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct MiniConfig {
        name: String,
        ports: Vec<u16>,
        enabled: bool,
    }

    let input = "name: app\nports: [80, 443]\nenabled: true\n";
    let value: Value = yaml::from_str(input).expect("yaml value");
    let config =
        MiniConfig::deserialize(value.into_deserializer()).expect("yaml Value into deserializer");

    let reference_value: serde_yaml::Value = serde_yaml::from_str(input).expect("serde_yaml value");
    let reference = MiniConfig::deserialize(reference_value.into_deserializer())
        .expect("serde_yaml Value into deserializer");
    assert_eq!(config, reference);
}

#[test]
fn serde_api_number_deserializes_directly_and_as_deserializer() {
    let number: Number = yaml::from_str("42\n").expect("number from YAML scalar");
    assert_eq!(number.as_i64(), Some(42));

    let from_value = Number::deserialize(Value::from(u64::MAX)).expect("number from Value");
    assert_eq!(from_value.as_u64(), Some(u64::MAX));

    let owned_u64 = u64::deserialize(Number::from(17u64)).expect("owned number deserializer");
    assert_eq!(owned_u64, 17);

    let borrowed = Number::from(-9i64);
    let borrowed_i64 = i64::deserialize(&borrowed).expect("borrowed number deserializer");
    assert_eq!(borrowed_i64, -9);

    let owned_float = f64::deserialize(Number::from(1.25f64)).expect("float deserializer");
    assert_eq!(owned_float, 1.25);

    let nan = f64::deserialize(Number::from(f64::NAN)).expect("nan deserializer");
    assert!(nan.is_nan());

    let reference_owned =
        u64::deserialize(serde_yaml::Number::from(17u64)).expect("serde_yaml owned number");
    assert_eq!(owned_u64, reference_owned);
}

#[test]
fn serde_api_target_aware_i128_u128_scalars_match_serde_yaml() {
    for scalar in [
        i64::MAX.to_string(),
        (u64::MAX as u128).to_string(),
        i128::MAX.to_string(),
        i128::MIN.to_string(),
    ] {
        let input = format!("id: {scalar}\n");
        let parsed: BTreeMap<String, i128> = yaml::from_str(&input)
            .unwrap_or_else(|error| panic!("yaml parses {scalar} into i128: {error}"));
        let reference: BTreeMap<String, i128> = serde_yaml::from_str(&input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {scalar} into i128: {error}"));
        assert_eq!(parsed, reference, "{scalar}");
    }

    for scalar in [
        i64::MAX.to_string(),
        (u64::MAX as u128).to_string(),
        i128::MAX.to_string(),
        u128::MAX.to_string(),
    ] {
        let input = format!("id: {scalar}\n");
        let parsed: BTreeMap<String, u128> = yaml::from_str(&input)
            .unwrap_or_else(|error| panic!("yaml parses {scalar} into u128: {error}"));
        let reference: BTreeMap<String, u128> = serde_yaml::from_str(&input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {scalar} into u128: {error}"));
        assert_eq!(parsed, reference, "{scalar}");
    }
}

#[test]
fn serde_api_to_value_i128_u128_shape_matches_serde_yaml() {
    let in_range_i128 = yaml::to_value(i128::from(i64::MAX)).expect("i64 max i128 to_value");
    let in_range_i128_reference =
        serde_yaml::to_value(i128::from(i64::MAX)).expect("serde_yaml i64 max i128 to_value");
    assert_eq!(in_range_i128.as_u64(), in_range_i128_reference.as_u64());
    assert_eq!(in_range_i128.as_u64(), Some(i64::MAX as u64));

    let negative_i128 = yaml::to_value(i128::from(i64::MIN)).expect("i64 min i128 to_value");
    let negative_i128_reference =
        serde_yaml::to_value(i128::from(i64::MIN)).expect("serde_yaml i64 min i128 to_value");
    assert_eq!(negative_i128.as_i64(), negative_i128_reference.as_i64());
    assert_eq!(negative_i128.as_i64(), Some(i64::MIN));

    let large_i128 = yaml::to_value(i128::MAX).expect("i128 max to_value");
    let large_i128_reference =
        serde_yaml::to_value(i128::MAX).expect("serde_yaml i128 max to_value");
    assert_eq!(large_i128.as_str(), large_i128_reference.as_str());
    let large_i128_text = i128::MAX.to_string();
    assert_eq!(large_i128.as_str(), Some(large_i128_text.as_str()));

    let in_range_u128 = yaml::to_value(u128::from(u64::MAX)).expect("u64 max u128 to_value");
    let in_range_u128_reference =
        serde_yaml::to_value(u128::from(u64::MAX)).expect("serde_yaml u64 max u128 to_value");
    assert_eq!(in_range_u128.as_u64(), in_range_u128_reference.as_u64());
    assert_eq!(in_range_u128.as_u64(), Some(u64::MAX));

    let large_u128 = yaml::to_value(u128::MAX).expect("u128 max to_value");
    let large_u128_reference =
        serde_yaml::to_value(u128::MAX).expect("serde_yaml u128 max to_value");
    assert_eq!(large_u128.as_str(), large_u128_reference.as_str());
    let large_u128_text = u128::MAX.to_string();
    assert_eq!(large_u128.as_str(), Some(large_u128_text.as_str()));

    let direct = i128::MAX
        .serialize(yaml::value::Serializer)
        .expect("direct value serializer");
    assert_eq!(direct, large_i128);
}

#[test]
fn serde_api_parser_backed_value_preserves_widened_i128_u128_numbers() {
    let input = "i128_max: 170141183460469231731687303715884105727\nu128_max: 340282366920938463463374607431768211455\n";
    let value: Value = yaml::from_str(input).expect("parse widened numeric value");

    assert_eq!(value["i128_max"].as_i128(), Some(i128::MAX));
    assert_eq!(value["u128_max"].as_u128(), Some(u128::MAX));
    assert!(value["i128_max"].as_str().is_none());
    assert!(value["u128_max"].as_str().is_none());
}

#[test]
fn serde_api_large_integer_string_targets_match_serde_yaml() {
    let input = "i128_max: 170141183460469231731687303715884105727\nu128_max: 340282366920938463463374607431768211455\nu128_overflow: 340282366920938463463374607431768211456\n";
    let parsed: BTreeMap<String, String> =
        yaml::from_str(input).expect("yaml source-backed large integer strings");
    let reference: BTreeMap<String, String> =
        serde_yaml::from_str(input).expect("serde_yaml large integer strings");
    assert_eq!(parsed, reference);
    assert_eq!(
        parsed["u128_overflow"],
        "340282366920938463463374607431768211456"
    );
}

#[test]
fn serde_api_i128_u128_range_errors_preserve_scalar_span() {
    for (name, input, expected) in [
        (
            "i128-overflow",
            "id: 170141183460469231731687303715884105728\n",
            "i128",
        ),
        (
            "u128-overflow",
            "id: 340282366920938463463374607431768211456\n",
            "unsigned integer",
        ),
        ("negative-u128", "id: -1\n", "unsigned integer"),
    ] {
        let error = match name {
            "i128-overflow" => {
                yaml::from_str::<BTreeMap<String, i128>>(input).expect_err("i128 overflow rejected")
            }
            _ => yaml::from_str::<BTreeMap<String, u128>>(input).expect_err("u128 range rejected"),
        };
        assert!(
            error.to_string().contains(expected),
            "{name}: unexpected error {error}"
        );
        assert_eq!(error.span().line, 1, "{name}");
        assert_eq!(error.span().column, 5, "{name}");
    }
}

#[test]
fn serde_api_from_value_numeric_conversion_matrix() {
    let small_unsigned = Value::Number(Number::Unsigned(5));
    let as_i64: i64 = yaml::from_value(small_unsigned.clone()).expect("small unsigned to i64");
    let as_u64: u64 = yaml::from_value(small_unsigned.clone()).expect("small unsigned to u64");
    let as_i128: i128 = yaml::from_value(small_unsigned.clone()).expect("small unsigned to i128");
    let as_u128: u128 = yaml::from_value(small_unsigned.clone()).expect("small unsigned to u128");
    let as_f64: f64 = yaml::from_value(small_unsigned).expect("small unsigned to f64");
    assert_eq!(as_i64, 5);
    assert_eq!(as_u64, 5);
    assert_eq!(as_i128, 5);
    assert_eq!(as_u128, 5);
    assert_eq!(as_f64, 5.0);

    let large_unsigned = Value::Number(Number::Unsigned(u128::from(u64::MAX)));
    assert!(yaml::from_value::<i64>(large_unsigned.clone()).is_err());
    assert_eq!(
        yaml::from_value::<u64>(large_unsigned.clone()).expect("large unsigned to u64"),
        u64::MAX
    );
    assert_eq!(
        yaml::from_value::<i128>(large_unsigned.clone()).expect("large unsigned to i128"),
        i128::from(u64::MAX)
    );
    assert_eq!(
        yaml::from_value::<u128>(large_unsigned).expect("large unsigned to u128"),
        u128::from(u64::MAX)
    );

    let huge_unsigned = Value::Number(Number::Unsigned(u128::MAX));
    assert!(yaml::from_value::<i64>(huge_unsigned.clone()).is_err());
    assert!(yaml::from_value::<u64>(huge_unsigned.clone()).is_err());
    assert!(yaml::from_value::<i128>(huge_unsigned.clone()).is_err());
    assert_eq!(
        yaml::from_value::<u128>(huge_unsigned).expect("huge unsigned to u128"),
        u128::MAX
    );
}

#[test]
fn serde_api_plain_special_floats_match_serde_yaml() {
    #[derive(Debug, Deserialize)]
    struct SpecialFloats {
        nan: f64,
        inf: f64,
        plus_inf: f64,
        neg_inf: f64,
    }

    let input = "nan: .NaN\ninf: .inf\nplus_inf: +.INF\nneg_inf: -.inf\n";
    let value: Value = yaml::from_str(input).expect("value");
    assert!(value["nan"].as_f64().expect("nan").is_nan());
    assert_eq!(value["inf"].as_f64(), Some(f64::INFINITY));
    assert_eq!(value["plus_inf"].as_f64(), Some(f64::INFINITY));
    assert_eq!(value["neg_inf"].as_f64(), Some(f64::NEG_INFINITY));

    let ours: SpecialFloats = yaml::from_str(input).expect("typed special floats");
    let reference: SpecialFloats =
        serde_yaml::from_str(input).expect("serde_yaml typed special floats");
    assert!(ours.nan.is_nan());
    assert!(reference.nan.is_nan());
    assert_eq!(ours.inf, reference.inf);
    assert_eq!(ours.plus_inf, reference.plus_inf);
    assert_eq!(ours.neg_inf, reference.neg_inf);

    let strings: BTreeMap<String, String> =
        yaml::from_str(input).expect("source-backed string targets");
    let reference_strings: BTreeMap<String, String> =
        serde_yaml::from_str(input).expect("serde_yaml string targets");
    assert_eq!(strings, reference_strings);
    assert!("+.nan".parse::<Number>().is_err());
    assert!("+.nan".parse::<serde_yaml::Number>().is_err());
}
