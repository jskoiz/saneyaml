# Compatibility Notes

This crate is aiming at a replacement candidate for **Serde read paths first**:
`serde_yaml`-style `from_str`, `from_slice`, and `from_reader` for common
developer configuration files, with parser/tree/event behavior compared against
`yaml-rust2` and `saphyr`. It is not claiming byte-for-byte emitter parity or a
lossless YAML editor surface.

The compatibility target is intentionally split:

- Primary API target: `serde_yaml` read-side ergonomics for config-shaped YAML.
- Parser reference target: YAML 1.2 tree/event acceptance comparable to
  `yaml-rust2` and `saphyr` for supported syntax.
- Ecosystem divergence target: libyaml/YAML 1.1-era behavior is documented and
  rejected unless a future migration milestone explicitly chooses it.

| Area | Prototype policy | libyaml / YAML 1.1 paths | yaml-rust2 / saphyr | serde_yaml |
|---|---|---|---|---|
| YAML version | Numeric `%YAML` version directives are accepted as syntax metadata; scalar resolution remains YAML 1.2/core-config oriented | Often YAML 1.1 heritage | Compare as YAML 1.2-oriented Rust parsers | Serde data model |
| `on`, `off`, `yes`, `no` | Strings | Often booleans; aliases like `on` and `yes` can collide as the same key | Compare per schema | Usually data-model dependent |
| Duplicate keys | Error for duplicate scalar, sequence, and mapping keys after alias expansion, with mapping-key identity order-insensitive like public `Mapping` equality and typed scalar key domains distinct (`1` and `"1"` are different keys); nonnegative signed and unsigned integer keys share identity; signed-zero float keys share identity; raw events still expose duplicate keys | Psych/libyaml can construct duplicate scalar keys as last-wins values | yaml-rust2 rejects some duplicate collection keys, while saphyr accepts selected cases such as X38W | `serde_yaml` rejects duplicate scalar keys |
| Merge key `<<` | Preserved as a literal key by default in parser/tree/Serde `Value` reads; alias values are expanded as ordinary cloned values, but merged keys are not inserted unless callers explicitly invoke `yaml::Value::apply_merge()` | Common legacy feature, often expanded with earlier merge-list mappings winning and explicit target keys overriding merged keys | Preserved literally in current tree loaders | Preserved literally in `Value`; opt-in `Value::apply_merge()` expands merges |
| Anchors and aliases | Supported for acyclic value expansion; graph identity is not preserved; colon-bearing anchor names and anchors on empty scalar nodes are accepted with recorded tree-shape divergences | Supported, sometimes with graph identity and legacy loader-specific tree shapes | Supported by clone-on-alias loading; saphyr loads selected empty scalar anchor nodes as empty strings | Data-model dependent, accepted in common read paths |
| Custom tags | Preserved as tagged tree/Value nodes for `Value` and Serde enum support; transparent metadata for ordinary typed Serde reads; `%TAG` handles are resolved for the following explicit document; undeclared named handles are rejected; schema coercion is not implemented | Supported as tags | Supported as tags | Partial/lossy |
| Multiline quoted flow scalars | Supported with YAML line folding | Some libyaml paths reject selected YAML 1.2 flow-key cases | Accepted by yaml-rust2/saphyr | Some cases rejected |
| Adjacent flow mapping values | Accept YAML 1.2 adjacent flow mapping values, including colon-prefixed adjacent plain scalars | Psych/libyaml accepts C2DT but rejects 5MUD, 5T43, and 58MP | yaml-rust2/saphyr accept all four selected cases | `serde_yaml` accepts C2DT but rejects 5MUD, 5T43, and 58MP |
| Bare/explicit document streams | YAML 1.2 bare documents after `...` are supported, including root literal scalars whose content begins at column 1, and directive-looking lines inside open flow collections are parsed as content | Some libyaml-era paths reject these streams or treat percent-prefixed flow content as directive-sensitive | Accepted by yaml-rust2/saphyr | `serde_yaml` rejects the full M7A3 stream after the first document and rejects UT92 |
| Comments/formatting | Discarded | Not semantic | Not semantic | Discarded |
| Emission | Deterministic structural YAML for emittable trees; duplicate-effective mapping keys, over-depth trees including caller-built complex keys, and directly nested tags are rejected before output; public writers follow `serde_yaml` document-marker policy by omitting `---` for the first ordinary document and inserting `---` between stream documents | Manual comparison only | Manual comparison only | Public writer document-marker policy is matched; byte-for-byte formatting parity remains out of scope |
| Numeric extensions | Decimal ints/floats plus underscores and YAML special floats are resolved; hex/octal/binary remain strings unless explicitly tagged | YAML 1.1 has broad numeric typing | YAML 1.2 core support varies by crate | Data-model dependent |
| Directives | Numeric `%YAML` version directives and `%TAG` are accepted as syntax/event inputs; reserved unknown directives are ignored but still require an explicit document start; version directives do not switch scalar schema; directive metadata is exposed on `DocumentStart` events | Exposed and may affect version/schema handling | Exposed by parser layers | Usually not a Serde value |
| Explicit core tags | Tree and `Value` loading preserve explicit `!!binary`, `!!timestamp`, `!!int`, and `!!float` tags; typed Serde reads coerce explicit `!!int`/`!!float` numeric targets while preserving tags in retained values | YAML 1.1 typed binary/timestamp/numeric support is common | Tag-aware behavior varies, including `BadValue` for some explicit core tags | Partial/lossy |

## Public API Compatibility Surface

Current read APIs:

- `yaml::from_str<T>(&str) -> Result<T>`
- `yaml::from_slice<T>(&[u8]) -> Result<T>`
- `yaml::from_reader<T, R: std::io::Read>(R) -> Result<T>`
- `yaml::from_node<T>(&Node) -> Result<T>`
- `yaml::from_documents_str<T>(&str) -> Result<Vec<T>>`
- `yaml::from_documents_slice<T>(&[u8]) -> Result<Vec<T>>`
- `yaml::from_documents_reader<T, R: std::io::Read>(R) -> Result<Vec<T>>`
- `yaml::from_value<T>(yaml::Value) -> Result<T>`
- `yaml::Deserializer::{from_str, from_slice, from_reader}` for iterating
  multi-document streams as standard Serde document deserializers, and for
  single-document `T::deserialize(yaml::Deserializer::from_str(input))`
- `yaml::with::singleton_map::deserialize` for read-side enum field
  annotations compatible with `serde_yaml::with::singleton_map`
- `yaml::with::singleton_map_recursive::deserialize` for read-side nested enum
  field annotations compatible with `serde_yaml::with::singleton_map_recursive`
- `yaml::to_value<T: serde::Serialize>(T) -> Result<yaml::Value>` for
  common config-shaped structs, maps, sequences, scalar values, and Serde enum
  representations
- `yaml::to_string<T: serde::Serialize>(&T) -> Result<String>` and
  `yaml::to_writer<W: std::io::Write, T: serde::Serialize>(W, &T)` for
  deterministic structural emission from serializable config-shaped values
- `yaml::Serializer<W: std::io::Write>` with `new`, `flush`, and `into_inner`
  for `serde_yaml::Serializer`-style writer usage. Each top-level
  `Serialize::serialize(&mut serializer)` call writes one structural YAML
  document.
- `yaml::value::Serializer` and `yaml::value::to_value` for
  `serde_yaml::value`-style value serialization paths
- `yaml::with::singleton_map::serialize` and
  `yaml::with::singleton_map_recursive::serialize` for write-side enum field
  annotations compatible with the corresponding `serde_yaml::with` helpers
- `yaml::parse_str`, `parse_bytes`, `parse_documents`, and `parse_events`

Migration-facing API status is tracked by `MIGRATION.md` and the executable
`tests/serde_yaml_swap_harness.rs` harness. The current swap matrix covers:

| `serde_yaml` surface | `yaml` surface | Status |
|---|---|---|
| `from_str`, `from_slice`, `from_reader` | `yaml::from_str`, `yaml::from_slice`, `yaml::from_reader` | Config-shaped typed reads and `Value` reads covered; reader-backed borrowed targets remain owned-only |
| `Deserializer::{from_str, from_slice, from_reader}` | `yaml::Deserializer::{from_str, from_slice, from_reader}` | Direct Serde use and stream iteration covered, with one empty-stream iterator divergence documented below |
| `Value`, `Mapping`, `Number` | `yaml::Value`, `yaml::Mapping`, `yaml::Number` | Common read, mutation, sealed indexing, helper, trait, and number conversion surfaces covered |
| `value::to_value`, `value::Serializer` | `yaml::value::to_value`, `yaml::value::Serializer` | Value-backed serialization covered for common config shapes, tags, bytes, and 128-bit integer policy |
| `to_string`, `to_writer`, `Serializer` | `yaml::to_string`, `yaml::to_writer`, `yaml::Serializer` | Structural writer support covered; byte-for-byte emitter formatting parity remains out of scope |
| `with::singleton_map`, `with::singleton_map_recursive` | `yaml::with::singleton_map`, `yaml::with::singleton_map_recursive` | Read/write enum-field annotation paths covered |

`yaml::Value` is a spanless read-side Serde value, matching the replacement
direction of `serde_yaml::Value`: sequences contain `Vec<Value>`, mappings use
`yaml::Mapping`, `Value::Tagged` preserves YAML tags, and tagged nodes remain
visible when deserializing into `Value` or into YAML-tagged enum variants.
`yaml::Value::apply_merge()` is available as an opt-in post-load helper for
`serde_yaml::Value::apply_merge()`-style merge key expansion; the parser, tree
loader, event stream, and default `Value` reads still preserve `<<` literally.
For non-enum typed reads, tags are transparent metadata: `!Env prod` can
deserialize into `String`, `!Ports [80, 443]` into `Vec<u16>`, and
`!Maybe null` into `Option<T>`. `Value::default()` is null, `Value` can drive
`Deserialize::deserialize(value)` by value or by reference, value-backed nulls
match `serde_yaml::Value` by acting as empty sequence or mapping inputs when
the target type asks for a collection, and parser-backed empty or void nodes
also act as empty collection inputs while explicit `null` and `~` remain null
scalars. String/index lookup returns a null sentinel for missing paths.
`yaml::Index` and `yaml::mapping::Index` are sealed preview traits; use the
built-in string, `usize`, and `Value` lookup surfaces instead of implementing
custom index types. Parser spans remain on
`yaml::Node`, whose recursive tree payload is `yaml::NodeValue`; typed
`from_str` and `from_node` continue to deserialize from that spanful tree so
error diagnostics keep source locations. Direct Serde use through
`Deserialize::deserialize(yaml::Deserializer::from_str(input))` preserves the
same primary and related parse diagnostic spans, such as duplicate-key
references to the original key. Replacement-facing Serde deserializer surfaces
(`yaml::Deserializer`, stream document deserializers, `Node`, `Value`,
`&Value`, and `Value::into_deserializer()`) expose the public `yaml::Error`
type as their Serde associated error.

Borrowed deserialization is supported from retained data structures and from
borrowed input buffers: `yaml::from_str(&str)`, `yaml::from_slice(&[u8])`,
`yaml::Deserializer::from_str(&str)`,
`yaml::Deserializer::from_slice(&[u8])`, `yaml::from_node(&Node)`,
stream items from `yaml::Deserializer`, and `Deserialize::deserialize(&Value)` can
deserialize borrowed string fields tied to the input/tree/value lifetime.
All parser, event, and Serde read entrypoints ignore a single UTF-8 BOM only at
stream byte offset 0, matching common `serde_yaml` and reference-loader
behavior for BOM-prefixed config files while keeping spans anchored to the
original byte buffer.
Direct input entrypoints borrow only scalars whose value can be represented as
a slice of the original input; transformed scalars such as escaped quoted
strings and block scalars still require owned `String`/`Cow` targets.

Explicit empty documents in YAML streams are preserved as `Value::Null` rather
than dropped, matching the common Serde/reference-crate stream shape.
`yaml::Deserializer` yields successfully parsed documents before the first
later parser error item in a stream; the `from_documents_*` helpers remain
all-or-error convenience APIs.
For empty input, direct `yaml::from_str::<Value>("")`,
`serde_yaml::from_str::<serde_yaml::Value>("")`, and direct
`Value::deserialize(Deserializer::from_str(""))` all produce null values. The
stream iterator differs: `yaml::Deserializer::from_str("")` yields zero
documents while `serde_yaml::Deserializer::from_str("")` yields one null
document.

The writer serializer, `yaml::to_value`, `yaml::to_string`, and
`yaml::to_writer` are structural write-side bridges for replacement code that
needs owned values or deterministic config output. Non-finite YAML floats emit
as `.nan`, `.inf`, and `-.inf`, while strings with those spellings are quoted,
so parsed special floats and special-float-looking strings remain stable under
the parser/emitter round-trip invariant. `yaml::to_value` and
`yaml::value::Serializer` match `serde_yaml` for generic 128-bit integer
serialization by constructing numeric values for `i64`/`u64`-range inputs and
strings for out-of-range `i128`/`u128` inputs, and match `serde_yaml`'s
singleton `collect_str` tag-map shape for `TaggedValue`; empty public tag
constructors and empty Serde variant tags are rejected like `serde_yaml`, while
parser-backed explicit non-specific `!` tags remain preserved. Value-backed byte
serialization follows `serde_yaml::value::Serializer` by producing a numeric
sequence, while document writers reject `serialize_bytes` inputs like
`serde_yaml` during the normal value serialization pass, so custom
`Serialize` implementations are not invoked a second time for byte preflight.
Read-side byte visitors follow `serde_yaml`: parser-backed YAML scalars reject
`deserialize_bytes`/`deserialize_byte_buf`, value-backed numeric byte sequences
deserialize to `Vec<u8>` through normal sequence handling, and direct byte
visitors reject both value strings and value sequences.
Parser-backed `yaml::Value` reads still retain widened `i128`/`u128` numbers.
`yaml::to_string`, `yaml::to_writer`, and
`yaml::Serializer<W>` omit an explicit `---` for the first ordinary document and
insert `---` before later stream documents, matching `serde_yaml`'s public writer
boundary policy; byte-for-byte emitter parity with `serde_yaml` remains outside
the current replacement target. Reader
and document-vector helpers still require `DeserializeOwned` because they
cannot return borrows from consumed readers or temporary document vectors.
`yaml::Deserializer::from_reader` is likewise owned-only for borrowed `&str`
targets.

## Event API Status

`parse_events` returns a parser-backed event stream with balanced stream,
document, sequence, and mapping boundaries. Events now carry scalar style,
flow-vs-block collection style, tag metadata, anchor metadata, alias events, directive metadata on
`DocumentStart`, and explicit document start/end state. This is intended to
track the useful parser-event surface of `yaml-rust2`/`saphyr` while retaining
this crate's `Span` diagnostics. A normalized event parity harness compares the
selected event stream shape against `yaml-rust2` and `saphyr-parser`; it strips
reference-specific anchor ids, spans, and directive payloads where those APIs do
not expose equivalent data.

A normalized loaded-tree parity harness also compares selected document value
shapes against `yaml-rust2::YamlLoader` and `saphyr::Yaml`. It strips tag
metadata when comparing with `yaml-rust2`, whose tree type has no tag variant,
and keeps a separate tag-preserving comparison against `saphyr` for custom
tagged nodes. This is value-shape parity, not a claim of graph identity,
lossless source preservation, or universal schema agreement.

Relative to libyaml, the event layer maps document implicitness to explicit
marker booleans, document directives to `DocumentStart` metadata, scalar and
collection anchors/tags to `EventMeta`, alias events to `Alias`, and scalar
style to `ScalarStyle`, and sequence/mapping spelling to `CollectionStyle`.
`DocumentStart` and `DocumentEnd` spans identify the
marker token itself; directives stay on `DocumentStart`, and root properties
after `---` stay on the following node event. `%TAG` directives are
per-document and do not leak. Libyaml-only event metadata remains intentionally
out of scope: scalar plain/quoted implicit tag flags, sequence/mapping implicit
tag flags, raw scalar spelling, schema construction decisions, and graph
identity are not exposed.

Event policy:

- `DocumentStart` exposes `%YAML 1.2` version metadata and `%TAG`
  handle/prefix metadata with spans.
- reserved unknown directives are ignored and do not appear in
  `DocumentStart` metadata.
- `%TAG` handle resolution is scoped to the following explicit document.
- undeclared named handles such as `!e!Thing` are rejected in tree and event
  parsing.
- aliases are emitted as raw `Alias` events, including scalar block mapping-key
  aliases, instead of being expanded in the event stream.
- sequence and mapping start events expose `CollectionStyle::Block` or
  `CollectionStyle::Flow`.
- duplicate-key validation is a tree-loading policy; `parse_events` exposes
  duplicate scalar, sequence, mapping, and tagged keys as raw key/value events.
- recursive aliases are allowed in `parse_events` as raw alias references, but
  unknown aliases are rejected.
- flow mapping keys are parsed as normal nodes for anchors, aliases, tags,
  scalar keys, sequence keys, and mapping keys.

Serde numeric policy:

- typed integer reads support `i128` and `u128` targets, including scalar
  values beyond `u64::MAX` when the target is `String` and raw source spelling
  is needed.
- `yaml::Number` stores signed and unsigned integers as `i128`/`u128`;
  `as_i64`/`as_u64` remain range-checked convenience accessors, while
  `as_i128`/`as_u128` expose the widened representation. Public equality,
  hashing, ordering, `Mapping` lookup, and emitter duplicate-key preflight
  normalize same-magnitude nonnegative signed and unsigned integers to the
  same identity, matching `serde_yaml` positive integer behavior while keeping
  text keys such as `"1"` distinct.
- `yaml::Number` follows Rust `f64` equality for finite float identity in
  public `Value`/`Mapping` comparisons, so `0.0` and `-0.0` are the same key.
  Parser duplicate-key rejection and emitter duplicate-key preflight use that
  same signed-zero identity rather than raw float bits.
- generic `Serialize` inputs to `yaml::to_value` and `yaml::value::Serializer`
  follow `serde_yaml::value::Serializer` by stringifying 128-bit integers that
  do not fit in `i64` or `u64`; parsed `yaml::Value` trees keep widened
  `Number` values.
- `yaml::Value` and `yaml::Number` expose the common `serde_yaml` read-side
  numeric helpers (`is_i64`, `is_u64`, `is_f64`, finite/NaN/infinity checks),
  primitive construction, string/sequence/map construction, `Number`
  `Display`/`FromStr`, direct `Number` deserializer support, and
  `serde_yaml`-style public comparison/hash traits for `Value`, `Mapping`,
  `TaggedValue`, `Tag`, and `Number` where the upstream types expose them.
- writer and value serializers cap initial allocation from caller-provided
  Serde collection length hints; actual serialized collection size can still
  grow normally as elements arrive.
- `yaml::mapping` exposes `serde_yaml`-style public iterator names
  (`Iter`, `IterMut`, `IntoIter`, `Keys`, `IntoKeys`, `Values`,
  `ValuesMut`, and `IntoValues`), and those iterators implement
  `ExactSizeIterator` in addition to double-ended traversal.
- integer range errors from `from_str`, `from_slice`, `from_reader`,
  `from_node`, and `Deserializer::from_str` preserve the scalar span.

Known event limitations remain:

- raw scalar spelling is not exposed; scalar event values are normalized
- document start markers can carry root node content/properties such as
  `--- &root`, but document end markers still reject non-comment trailing text
- tree loading still expands acyclic aliases and does not preserve graph
  identity, even though `parse_events` exposes alias events without expansion

## Fixture Gates

The compatibility harness checks shared acceptance across this crate,
`serde_yaml`, `yaml-rust2`, and `saphyr`, plus dedicated Rust-reference
parity/divergence cases where libyaml-backed `serde_yaml` disagrees, for:

- the pinned selected YAML test-suite manifest, currently 123 fixtures with
  explicit per-case `expected`, `source`, and parser/tree/Serde `policy`
  fields: 80 normal accepts, 41 syntax/error rejects, and YAML-suite
  2JQS/X38W as intentional tree/Serde-only rejections while raw parser events
  remain available. The manifest also owns the selected-suite parity ledger:
  `parity.event`, `parity.tree`, and `parity.shared_reference` must match the
  Rust source gates exactly. Current selected-suite ledgers cover event parity
  for 78 accepted cases with 2 documented event-shape deferrals, loaded-tree
  value-shape parity for 77 accepted cases with 3 documented tree-shape
  deferrals, and shared-reference acceptance for 57 accepted cases with 23
  documented `serde_yaml`/libyaml divergence deferrals
- core scalars
- block and flow collections
- explicit block mapping entries
- plain block mapping keys containing YAML-safe punctuation, including
  YAML-suite 2EBW
- directive-looking plain scalar continuation lines, including YAML-suite XLQ9
- compact block mappings
- full-line comments before, after, and between explicit documents,
  including YAML-suite JHB9
- indentless block sequences as mapping values
- tagged block collection nodes, including YAML-suite 57H4
- acyclic anchors and aliases, including anchor redefinition, scalar
  mapping-key anchors, tag/anchor property-order preservation for aliases,
  anchor-only flow nodes that resolve to null, scalar block mapping-key aliases
  in raw events, YAML-suite 2SXE colon-bearing block anchor and alias names, and
  YAML-suite PW8X anchors on empty scalar nodes. Colon-bearing anchor names and
  anchor-only empty scalar nodes are covered as documented tree-shape
  divergences where reference loaders disagree. The Serde API matrix also
  checks tag/anchor alias preservation across parser nodes, retained `Value`,
  direct deserializers, reader/document helpers, and concrete typed reads
- merge-key spelling preserved as a literal key with alias-expanded value
- block scalar indentation and chomping headers
- zero-indented root block scalars, including block scalar headers on explicit
  document-start lines and comment-looking content inside folded scalars,
  covered by YAML-suite W4TN, FP8R, and DK3J as YAML 1.2 Rust-parser parity
  cases with a `serde_yaml`/libyaml rejection split
- block scalar tab-starting content lines are rejected with span diagnostics,
  including YAML-suite Y79Y, while indented tab content is accepted, including
  YAML-suite Y79Y/001
- tabs used as token separation are accepted where YAML 1.2 permits them,
  including YAML-suite 6BCT, 6CA3, Q5MG, Y79Y/002, and Y79Y/010, with a
  recorded `serde_yaml`/libyaml divergence for the root/tab separation cases
- block scalar trailing-line chomping, including literal keep chomping with a
  spaces-only content line from YAML-suite 6FWR and empty scalar chomping from
  YAML-suite K858
- folded block scalars with leading blank, paragraph breaks, more-indented
  lines, spaces-only blank lines, and tab-leading detected-indentation content,
  including YAML-suite F6MC, 6VJK, 4Q9F, TS54, 7T8X, 93WF, K527, and R4YG.
  R4YG is covered as YAML 1.2 Rust-parser parity with a recorded
  `serde_yaml`/libyaml tab-character rejection split
- multiline plain scalars in mappings
- multiline plain scalars with empty-line paragraph breaks
- multiline plain scalars in block sequence items whose continuation line
  looks like an under-indented sequence entry, including YAML-suite AB8U
- multiline flow-style scalar empty-line folding, including YAML-suite 5GBF
- multiline flow scalars in block mappings, including YAML-suite 4CQQ
- trailing comments after multiline quoted, plain, block, explicit key, and
  anchor/property forms, including YAML-suite XW4D and RZP5
- multiline flow sequences and mappings with comment/line-break separators
- multiline plain flow mapping keys without values, including YAML-suite 8KB6
- multiline quoted flow mapping keys, including YAML-suite 9SA2
- flow mapping key metadata, including YAML-suite X38W and 6BFJ anchors,
  aliases, tags, scalar keys, sequence keys, and mapping keys
- multiline single-pair flow mapping entries in flow sequences, including
  YAML-suite QF4Y and CT4Q
- zero-indented block sequences as explicit mapping keys and values, including
  YAML-suite 6PBE
- anchored zero-indented block sequences as mapping values, including
  YAML-suite SKE5
- implicit flow mapping entries inside flow sequences, including explicit and collection keys
- flow mapping scalar and collection keys, missing values, and URL-shaped plain keys
- structural duplicate-key rejection for scalar, sequence, and mapping keys after
  alias expansion, including YAML-suite 2JQS duplicate missing block mapping keys
  and X38W alias-expanded collection keys
- multi-document streams
- raw event metadata for scalar style, tags, anchors, aliases, directives, and
  explicit document markers, including YAML-suite MZX3, S4JQ, 6M2F, BU8L,
  3GZX, W4TN, U3C3, 6CK3, 6LVF, and FTA2
- `%TAG` shorthand resolution with URI percent-decoding in suffixes, including
  YAML-suite 6CK3
- tag and anchor property combinations are reference-gated against event,
  loaded-tree, and shared-acceptance harnesses for YAML-suite BU8L and 9KAX
- YAML 1.2 bare document streams, including YAML-suite M7A3, with a recorded
  `serde_yaml`/libyaml divergence rather than inclusion in the shared
  acceptance set
- directive-looking lines inside open flow collections, including YAML-suite
  UT92, as YAML 1.2 Rust-parser parity with a recorded `serde_yaml`/libyaml
  divergence
- empty implicit mapping keys as null keys in selected block and flow forms,
  including YAML-suite S3PD, CFD4, M2N8-00, and UKK6-00, while retaining
  duplicate-null-key rejection
- explicit non-specific tag cases: YAML-suite UKK6/02 and S4JQ are accepted in
  event/shared-reference gates where the references agree, but remain
  documented tree-shape divergences because loaders disagree on the empty/tagged
  value shape
- selected upstream YAML-suite error fixtures, including SR86 anchor-plus-alias
  node properties, CML9/T833 missing comma failures, 6JTT unclosed flow
  sequence, CTN5 extra comma rejection in flow collections, YJV2 dash-only
  plain scalars in flow sequences, 9JBA/CVW2 adjacent comment-looking text in
  and after a flow sequence, 9C9N wrong-indented flow sequence continuation, 236B
  invalid value after a mapping, DK4H/ZXT5
  implicit flow-sequence keys followed by newlines, 5LLU bad block-scalar indentation after
  spaces-only lines, Y79Y tab-starting block scalar content and tab separation
  after block indicators, SY6V/G9HC/GT5M invalid anchor/sequence placements,
  5U3A same-line block sequence mapping values, ZCZ6 nested plain mapping
  values, 8XDJ/BF9H/BS4K comment-terminated plain scalar continuations, and
  9HCY/EB22/RHX7/9MMA/B63P invalid directive lifecycle forms,
  9KBC/CXX2 block mappings on explicit document-start lines, 4JVG
  duplicate anchor properties on one node, and 2G84 malformed block scalar
  indentation indicators, plus JY7Z/Q4CL trailing content after double-quoted
  mapping values, and QB6E/DK95-01 wrong-indented multiline double-quoted
  mapping values
- selected upstream YAML-suite double-quoted scalar fixtures, including
  3RLN-001/3RLN-002 escaped and indentation tabs, KH5V-001 inline escaped
  tabs, 6WPF/KSS4 same-indent folded continuations, and even-backslash folded
  line continuations that preserve literal backslashes
- custom YAML tags for Serde enum, `Value::Tagged`, and transparent typed read support
- GitHub Actions, including matrix expressions, workflow_dispatch inputs,
  array-form triggers, preset permissions, string/list/group runner targets,
  and a pinned upstream `actions/starter-workflows` Node.js CI snapshot
- Docker Compose-style config, including raw event coverage for anchors,
  aliases, literal merge keys, and polymorphic service fields such as
  environment maps/lists, healthcheck command strings/lists, env files,
  profiles, depends_on condition maps, typed volume mounts, service platforms,
  deploy resource limits/reservations, and a pinned upstream
  `docker/awesome-compose` nginx/flask/mysql snapshot with secrets, networks,
  build forms, and list/map depends_on shapes
- Kubernetes multi-document manifests, including Helm-rendered streams with
  comment-only empty documents, explicit stream terminators, block scalar data,
  CRDs with embedded OpenAPI v3 schemas and custom resources, and YAML 1.2
  string treatment for `yes`/`on`
- Helm values and Chart.yaml metadata/dependencies, including semver-like
  strings, constraint strings, annotation keys, maintainers, dependency tags,
  aliases, `import-values`, and OCI chart repository URLs
- OpenAPI fragments, including path templates, examples, extension keys, and `application/problem+json`
- Cloudflare Wrangler-style YAML
- Ansible-style playbooks, including `!vault` and `!unsafe` tagged values and
  raw event coverage for tag/style metadata
- the real-world fixture registry in `tests/fixtures/real-world/SOURCE.toml`,
  currently 26 files and 32 YAML documents, with per-fixture domain, source
  type, version surface, license/redaction note, reduction note, expected
  document count, and gate coverage; every registered domain must include
  non-synthetic upstream/adapted provenance, currently covering GitHub Actions,
  Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and Ansible
- shared-reference acceptance for every registered real-world fixture against
  this crate, `serde_yaml` 0.9.34, `yaml-rust2` 0.11.0, and `saphyr` 0.0.6
  as pinned in `Cargo.toml`
- normalized loaded-tree parity for the registered real-world fixtures against
  `yaml-rust2` 0.11.0 and `saphyr` 0.0.6, covering the same GitHub Actions,
  Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and Ansible fixture set
  used by event parity

The adoption path should be driven by failing conformance fixtures, real-world
config incompatibilities, and safety gaps. Compatibility shims are deliberately
out of scope unless a future migration milestone explicitly calls for them.
