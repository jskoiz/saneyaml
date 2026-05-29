use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use yaml::{LoadOptions, Timestamp, Value};

const MANIFEST: &str = include_str!("fixtures/yaml11-conformance/manifest.toml");
const FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/yaml11-conformance"
);

#[derive(Debug, Deserialize)]
struct Manifest {
    case: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    path: String,
    kind: String,
    tag: String,
    expected: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ResolvedBundle {
    set: BTreeSet<String>,
    omap: BTreeMap<String, i64>,
    pairs: Vec<(String, i64)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LegacyMigrationPack {
    flags: LegacyFlags,
    numbers: LegacyNumbers,
    timestamps: LegacyTimestamps,
    payload: Vec<u8>,
    set: BTreeSet<String>,
    omap: BTreeMap<String, i64>,
    pairs: Vec<(String, i64)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LegacyFlags {
    deploy: bool,
    dry_run: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LegacyNumbers {
    file_mode: i64,
    hex_limit: i64,
    session: i64,
    invalid_octal: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LegacyTimestamps {
    release: Timestamp,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyServiceConfig {
    service: LegacyService,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyBinaryPayload {
    payload: Vec<u8>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyService {
    mode: i64,
    enabled: bool,
    retries: i64,
    owner: String,
}

#[test]
fn yaml11_conformance_manifest_is_complete() {
    let manifest = manifest();
    assert_eq!(manifest.case.len(), 17);
    let manifest_paths = manifest
        .case
        .iter()
        .map(|case| case.path.clone())
        .collect::<BTreeSet<_>>();
    let actual_paths = fixture_paths(Path::new(FIXTURE_ROOT));
    assert_eq!(manifest_paths, actual_paths);

    for case in &manifest.case {
        assert!(matches!(
            case.kind.as_str(),
            "set"
                | "omap"
                | "pairs"
                | "bundle"
                | "scalar-matrix"
                | "flow-scalar-matrix"
                | "merge"
                | "duplicate-key"
                | "multi-doc"
                | "binary"
        ));
        assert!(matches!(case.expected.as_str(), "accept" | "error"));
        assert!(
            case.tag == "!!set"
                || case.tag == "!!omap"
                || case.tag == "!!pairs"
                || case.tag == "!!merge"
                || case.tag == "%YAML 1.1"
                || case.tag.starts_with("tag:yaml.org,2002:")
                || case.tag.starts_with("%TAG ")
        );
    }
}

#[test]
fn yaml11_collection_tags_deserialize_to_typed_rust_collections() {
    for case in manifest().case.into_iter().filter(|case| {
        case.expected == "accept"
            && matches!(case.kind.as_str(), "set" | "omap" | "pairs" | "bundle")
    }) {
        let source = read_fixture(&case.path);
        match case.id.as_str() {
            "set-short" => {
                let set: BTreeSet<String> = yaml::from_str(&source).expect("short !!set");
                assert_eq!(
                    set,
                    BTreeSet::from(["alpha".to_string(), "beta".to_string()])
                );
                assert_tagged_payload(&source, "!!", "set", "mapping");
            }
            "set-canonical" => {
                let set: BTreeSet<String> = yaml::from_str(&source).expect("canonical !!set");
                assert_eq!(
                    set,
                    BTreeSet::from(["left".to_string(), "right".to_string()])
                );
                assert_tagged_payload(&source, "!", "tag:yaml.org,2002:set", "mapping");
            }
            "omap-short" => {
                let pairs: Vec<(String, i64)> = yaml::from_str(&source).expect("short !!omap");
                assert_eq!(
                    pairs,
                    vec![("first".to_string(), 1), ("second".to_string(), 2)]
                );
                let map: BTreeMap<String, i64> =
                    yaml::from_str(&source).expect("short !!omap as map");
                assert_eq!(
                    map,
                    BTreeMap::from([("first".to_string(), 1), ("second".to_string(), 2)])
                );
                assert_tagged_payload(&source, "!!", "omap", "sequence");
            }
            "pairs-short" => {
                let pairs: Vec<(String, i64)> = yaml::from_str(&source).expect("short !!pairs");
                assert_eq!(
                    pairs,
                    vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)]
                );
                assert_tagged_payload(&source, "!!", "pairs", "sequence");
            }
            "resolved-tag-handle" => {
                let bundle: ResolvedBundle = yaml::from_str(&source).expect("resolved %TAG bundle");
                assert_eq!(
                    bundle.set,
                    BTreeSet::from(["x".to_string(), "y".to_string()])
                );
                assert_eq!(
                    bundle.omap,
                    BTreeMap::from([("left".to_string(), 1), ("right".to_string(), 2)])
                );
                assert_eq!(
                    bundle.pairs,
                    vec![("same".to_string(), 1), ("same".to_string(), 2)]
                );
            }
            other => panic!("unhandled accepted YAML 1.1 collection case {other}"),
        }
    }
}

#[test]
fn yaml11_collection_tags_reject_lossy_typed_shapes() {
    let set_source = read_fixture("set-rejects-non-null-values.yaml");
    let error = yaml::from_str::<BTreeSet<String>>(&set_source)
        .expect_err("non-null !!set values are not ignored");
    assert!(
        error
            .to_string()
            .contains("expected explicit !!set entry value to be null"),
        "{error}"
    );

    let omap_source = read_fixture("omap-rejects-non-singleton-entry.yaml");
    let error = yaml::from_str::<Vec<(String, i64)>>(&omap_source)
        .expect_err("multi-pair !!omap entries are not flattened");
    assert!(
        error
            .to_string()
            .contains("expected explicit !!omap entry to contain exactly one pair"),
        "{error}"
    );
}

#[test]
fn yaml11_legacy_migration_pack_covers_default_explicit_and_directive_modes() {
    let source = read_fixture("legacy-migration-pack.yaml");

    let default: Value = yaml::from_str(&source).expect("default YAML 1.2-oriented value");
    assert_eq!(default["flags"]["deploy"].as_str(), Some("ON"));
    assert_eq!(default["flags"]["dry_run"].as_str(), Some("no"));
    assert_eq!(default["numbers"]["file_mode"].as_i64(), Some(644));
    assert_eq!(default["numbers"]["hex_limit"].as_str(), Some("0x10"));
    assert_eq!(default["numbers"]["session"].as_str(), Some("1:20:30"));
    assert_eq!(default["numbers"]["invalid_octal"].as_i64(), Some(9));
    assert_eq!(
        default["timestamps"]["release"].as_str(),
        Some("2026-05-24")
    );
    assert!(default["timestamps"]["release"].as_timestamp().is_none());

    let expected = LegacyMigrationPack {
        flags: LegacyFlags {
            deploy: true,
            dry_run: false,
        },
        numbers: LegacyNumbers {
            file_mode: 420,
            hex_limit: 16,
            session: 4830,
            invalid_octal: "09".to_string(),
        },
        timestamps: LegacyTimestamps {
            release: Timestamp::parse_yaml_1_1("2026-05-24").expect("timestamp"),
        },
        payload: b"Hello".to_vec(),
        set: BTreeSet::from(["admin".to_string(), "operator".to_string()]),
        omap: BTreeMap::from([("first".to_string(), 1), ("second".to_string(), 2)]),
        pairs: vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)],
    };

    let explicit: LegacyMigrationPack = LoadOptions::yaml_1_1()
        .from_str(&source)
        .expect("explicit YAML 1.1 fixture");
    let directive: LegacyMigrationPack = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven YAML 1.1 fixture");
    assert_eq!(explicit, expected);
    assert_eq!(directive, expected);
    assert_legacy_pack_public_entrypoints(LoadOptions::yaml_1_1(), &source, &expected, "explicit");
    assert_legacy_pack_public_entrypoints(
        LoadOptions::yaml_version_directive(),
        &source,
        &expected,
        "directive",
    );

    let directive_value: Value = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven YAML 1.1 value");
    assert_eq!(
        directive_value["timestamps"]["release"].as_timestamp(),
        Some(expected.timestamps.release)
    );
    assert_eq!(directive_value["payload"].as_str(), Some("SGVsbG8="));
    assert_eq!(
        directive_value["payload"]
            .as_tagged()
            .expect("binary tag retained")
            .tag,
        yaml::Tag::new("!<tag:yaml.org,2002:binary>")
    );
}

#[test]
fn yaml11_legacy_merge_fixture_expands_after_directive_schema_resolution() {
    let source = read_fixture("legacy-merge-directive.yaml");

    let default: Value = yaml::from_str(&source).expect("default merge fixture");
    assert!(default["service"]["<<"].is_null());
    assert_eq!(default["service"]["mode"].as_i64(), Some(644));
    assert_eq!(default["service"]["enabled"].as_str(), Some("no"));
    assert_eq!(default["service"]["retries"].as_str(), Some("0x3"));

    let directive: Value = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven merge fixture");
    assert!(directive["service"]["<<"].is_null());
    assert_eq!(directive["service"]["mode"].as_i64(), Some(420));
    assert_eq!(directive["service"]["enabled"].as_bool(), Some(false));
    assert_eq!(directive["service"]["retries"].as_i64(), Some(3));
    assert_eq!(directive["service"]["owner"].as_str(), Some("ops"));

    let typed: LegacyServiceConfig = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven typed merge fixture");
    assert_eq!(
        typed,
        LegacyServiceConfig {
            service: LegacyService {
                mode: 420,
                enabled: false,
                retries: 3,
                owner: "ops".to_string(),
            },
        }
    );
}

#[test]
fn yaml11_explicit_merge_tag_fixture_expands_and_keeps_literal_tags() {
    let source = read_fixture("explicit-merge-tags.yaml");
    let value: Value = yaml::from_str(&source).expect("explicit merge-tag fixture");

    for (service, image) in [
        ("tagged_service", "app:tagged"),
        ("canonical_service", "app:canonical"),
        ("resolved_service", "app:resolved"),
    ] {
        assert!(
            value[service]["<<"].is_null(),
            "{service} merge key removed"
        );
        assert_eq!(value[service]["image"].as_str(), Some(image));
        assert_eq!(value[service]["replicas"].as_u64(), Some(2));
    }

    assert!(value["resolved_list_service"]["<<"].is_null());
    assert_eq!(
        value["resolved_list_service"]["shared"].as_str(),
        Some("first")
    );
    assert_eq!(value["resolved_list_service"]["retries"].as_u64(), Some(3));
    assert_eq!(
        value["resolved_list_service"]["timeout"].as_str(),
        Some("explicit")
    );

    assert_tagged_key(
        &value["string_service"],
        yaml::Tag::new("!!str"),
        "<<",
        "literal",
    );
    assert_eq!(
        value["string_service"]["image"].as_str(),
        Some("app:string")
    );
    assert_tagged_key(
        &value["custom_service"],
        yaml::Tag::new("Thing"),
        "<<",
        "literal",
    );
    assert_eq!(
        value["custom_service"]["image"].as_str(),
        Some("app:custom")
    );

    let yaml11: Value = LoadOptions::yaml_1_1()
        .from_str(&source)
        .expect("explicit merge tags under YAML 1.1 schema");
    assert!(yaml11.equivalent(&value));
}

#[test]
fn yaml11_legacy_merge_edge_fixture_recovers_like_psych() {
    let source = read_fixture("legacy-merge-edge-recovery.yaml");

    let default_error = yaml::parse_str(&source)
        .expect_err("default YAML 1.2 construction rejects repeated merge keys");
    assert!(
        default_error
            .to_string()
            .contains("duplicate mapping key `<<`"),
        "{default_error}"
    );

    let directive: Value = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven YAML 1.1 merge recovery");
    assert_yaml11_merge_edge_value(&directive);

    let explicit: Value = LoadOptions::yaml_1_1()
        .from_str(&source)
        .expect("explicit YAML 1.1 merge recovery");
    assert!(explicit.equivalent(&directive));

    let parsed = LoadOptions::yaml_version_directive()
        .parse_str(&source)
        .expect("directive-driven YAML 1.1 merge tree");
    assert!(Value::from(&parsed).equivalent(&directive));
}

#[test]
fn yaml11_explicit_merge_tag_bad_payload_fixture_reports_scalar_span() {
    let source = read_fixture("explicit-merge-tag-bad-payload.yaml");
    let error = yaml::parse_str(&source).expect_err("invalid explicit merge-tag payload");
    assert!(
        error
            .to_string()
            .contains("expected a mapping or list of mappings for merging, but found scalar"),
        "{error}"
    );
    assert_eq!(error.line(), Some(3));
    assert_eq!(error.column(), Some(15));

    let value_error =
        yaml::from_str::<Value>(&source).expect_err("Value read rejects invalid merge payload");
    assert_eq!(value_error.line(), Some(3));
    assert_eq!(value_error.column(), Some(15));

    let yaml11: Value = LoadOptions::yaml_1_1()
        .from_str(&source)
        .expect("YAML 1.1 keeps invalid merge payload literal");
    assert_tagged_key(
        &yaml11["service"],
        yaml::Tag::new("!!merge"),
        "<<",
        "scalar",
    );
}

#[test]
fn yaml11_legacy_bool_key_collision_fixture_keeps_default_safe_and_reports_legacy_span() {
    let source = read_fixture("legacy-bool-key-collision.yaml");

    let default: Value = yaml::from_str(&source).expect("default keeps bool-like keys as strings");
    assert_eq!(default["on"].as_str(), Some("push"));
    assert_eq!(default["yes"].as_str(), Some("deploy"));

    let explicit = LoadOptions::yaml_1_1()
        .parse_str(&source)
        .expect_err("explicit YAML 1.1 keys collide");
    assert!(
        explicit
            .to_string()
            .contains("duplicate mapping key `true`")
    );
    assert_eq!(explicit.span().line, 4);
    assert_eq!(explicit.span().column, 1);

    let directive = LoadOptions::yaml_version_directive()
        .parse_str(&source)
        .expect_err("directive-driven YAML 1.1 keys collide");
    assert!(
        directive
            .to_string()
            .contains("duplicate mapping key `true`")
    );
    assert_eq!(directive.span().line, 4);
    assert_eq!(directive.span().column, 1);
}

#[test]
fn yaml11_entrypoint_matrix_reports_legacy_duplicate_key_spans() {
    for fixture in [
        "legacy-bool-key-collision.yaml",
        "legacy-numeric-key-collision.yaml",
    ] {
        let source = read_fixture(fixture);
        for options in [
            LoadOptions::yaml_1_1(),
            LoadOptions::yaml_version_directive(),
        ] {
            assert_legacy_duplicate_key_error_entrypoints(options, &source, fixture);
        }
    }
}

#[test]
fn yaml11_legacy_scalar_edge_stream_switches_per_document() {
    let source = read_fixture("legacy-scalar-edge-stream.yaml");
    let docs: Vec<Value> = LoadOptions::yaml_version_directive()
        .from_documents_str(&source)
        .expect("directive-driven edge stream");

    assert_eq!(docs.len(), 2);
    let legacy = &docs[0];
    assert!(legacy["nulls"]["tilde"].is_null());
    assert!(legacy["nulls"]["lower"].is_null());
    assert!(legacy["nulls"]["upper"].is_null());
    assert!(
        legacy["floats"]["inf"]
            .as_f64()
            .expect("positive infinity")
            .is_infinite()
    );
    assert!(
        legacy["floats"]["neg_inf"]
            .as_f64()
            .expect("negative infinity")
            .is_sign_negative()
    );
    assert!(legacy["floats"]["nan"].as_f64().expect("NaN").is_nan());
    assert_eq!(legacy["floats"]["sexagesimal"].as_f64(), Some(4830.5));
    assert_eq!(legacy["numbers"]["invalid_octal"].as_str(), Some("09"));
    assert_eq!(legacy["numbers"]["binary"].as_i64(), Some(10));
    for (field, source) in [
        ("date", "2026-05-24"),
        ("datetime_z", "2026-05-24T12:34:56Z"),
        ("spaced_offset", "2026-05-24 12:34:56 -7"),
        ("fractional", "2026-05-24t12:34:56.789+05:30"),
    ] {
        assert_eq!(
            legacy["timestamps"][field].as_timestamp(),
            Timestamp::parse_yaml_1_1(source),
            "{field} timestamp"
        );
    }
    assert_eq!(legacy["payload"].as_str(), Some("SGVsbG8="));
    assert_eq!(
        legacy["payload"]
            .as_tagged()
            .expect("binary tag retained")
            .tag,
        yaml::Tag::new("!<tag:yaml.org,2002:binary>")
    );

    let defaulted = &docs[1];
    assert_eq!(defaulted["flag"].as_str(), Some("ON"));
    assert_eq!(defaulted["octal"].as_i64(), Some(123));
    assert_eq!(defaulted["timestamp"].as_str(), Some("2026-05-24"));
    assert!(defaulted["timestamp"].as_timestamp().is_none());

    let streamed = LoadOptions::yaml_version_directive()
        .deserializer_from_str(&source)
        .map(Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("directive-driven stream deserializes");
    assert_eq!(streamed, docs);
    assert_value_sequences_equivalent(
        LoadOptions::yaml_version_directive()
            .from_documents_slice(source.as_bytes())
            .expect("directive-driven slice stream deserializes"),
        &docs,
        "from_documents_slice",
    );
    assert_value_sequences_equivalent(
        LoadOptions::yaml_version_directive()
            .from_documents_reader(Cursor::new(source.as_bytes()))
            .expect("directive-driven reader stream deserializes"),
        &docs,
        "from_documents_reader",
    );
    assert_value_sequences_equivalent(
        LoadOptions::yaml_version_directive()
            .deserializer_from_slice(source.as_bytes())
            .map(Value::deserialize)
            .collect::<Result<Vec<_>, _>>()
            .expect("directive-driven slice stream deserializer"),
        &docs,
        "deserializer_from_slice",
    );
    assert_value_sequences_equivalent(
        LoadOptions::yaml_version_directive()
            .deserializer_from_reader(Cursor::new(source.as_bytes()))
            .map(Value::deserialize)
            .collect::<Result<Vec<_>, _>>()
            .expect("directive-driven reader stream deserializer"),
        &docs,
        "deserializer_from_reader",
    );
    let parsed = LoadOptions::yaml_version_directive()
        .parse_documents(&source)
        .expect("directive-driven stream parses");
    assert_value_sequences_equivalent(
        parsed.iter().map(Value::from).collect(),
        &docs,
        "parse_documents",
    );
    let lossless = yaml::parse_lossless(&source).expect("YAML 1.1 stream parses losslessly");
    assert_eq!(lossless.as_source(), source);
    assert_eq!(lossless.to_string(), source);
    assert_eq!(lossless.documents().len(), docs.len());
    assert_eq!(
        lossless.documents()[0]
            .directives()
            .yaml_version
            .as_ref()
            .expect("first document declares YAML version")
            .minor,
        1
    );
}

#[test]
fn yaml11_legacy_flow_scalar_fixture_switches_inside_flow_collections() {
    let source = read_fixture("legacy-flow-scalar-directive.yaml");

    let default: Value = yaml::from_str(&source).expect("default flow scalar fixture");
    assert_eq!(default["flow_scalars"][0].as_str(), Some("ON"));
    assert_eq!(default["flow_scalars"][1].as_i64(), Some(12));
    assert_eq!(default["flow_scalars"][2].as_str(), Some("0x10"));
    assert_eq!(default["flow_scalars"][3].as_str(), Some("1:20"));
    assert_eq!(default["flow_scalars"][4].as_str(), Some("2026-05-24"));
    assert_eq!(default["flow_mapping"]["enabled"].as_str(), Some("OFF"));
    assert_eq!(default["flow_mapping"]["octal"].as_i64(), Some(12));
    assert_eq!(default["flow_mapping"]["hex"].as_str(), Some("0x10"));
    assert_eq!(default["flow_mapping"]["clock"].as_str(), Some("1:20"));
    assert_eq!(default["flow_keys"]["on"].as_str(), Some("push"));
    assert_eq!(default["flow_keys"]["off"].as_str(), Some("stop"));
    assert_eq!(default["flow_keys"][12].as_str(), Some("octal"));
    assert_eq!(default["flow_keys"]["012"].as_str(), Some("quoted"));

    let directive: Value = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("directive-driven flow scalar fixture");
    assert_eq!(directive["flow_scalars"][0].as_bool(), Some(true));
    assert_eq!(directive["flow_scalars"][1].as_i64(), Some(10));
    assert_eq!(directive["flow_scalars"][2].as_i64(), Some(16));
    assert_eq!(directive["flow_scalars"][3].as_i64(), Some(4800));
    assert_eq!(
        directive["flow_scalars"][4].as_timestamp(),
        Timestamp::parse_yaml_1_1("2026-05-24")
    );
    assert_eq!(directive["flow_mapping"]["enabled"].as_bool(), Some(false));
    assert_eq!(directive["flow_mapping"]["octal"].as_i64(), Some(10));
    assert_eq!(directive["flow_mapping"]["hex"].as_i64(), Some(16));
    assert_eq!(directive["flow_mapping"]["clock"].as_i64(), Some(4800));

    let flow_keys = directive["flow_keys"]
        .as_mapping()
        .expect("flow keys mapping");
    assert_eq!(
        flow_keys.get(Value::from(true)).and_then(Value::as_str),
        Some("push")
    );
    assert_eq!(
        flow_keys.get(Value::from(false)).and_then(Value::as_str),
        Some("stop")
    );
    assert_eq!(
        flow_keys.get(Value::from(10i64)).and_then(Value::as_str),
        Some("octal")
    );
    assert_eq!(flow_keys.get("012").and_then(Value::as_str), Some("quoted"));

    let explicit: Value = LoadOptions::yaml_1_1()
        .from_str(&source)
        .expect("explicit YAML 1.1 flow scalar fixture");
    assert!(explicit.equivalent(&directive));

    let parsed = LoadOptions::yaml_version_directive()
        .parse_str(&source)
        .expect("directive-driven flow scalar tree parses");
    assert!(Value::from(&parsed).equivalent(&directive));
}

#[test]
fn yaml11_legacy_invalid_binary_fixture_reports_typed_decode_error() {
    let source = read_fixture("legacy-invalid-binary.yaml");

    let retained: Value = LoadOptions::yaml_version_directive()
        .from_str(&source)
        .expect("invalid binary spelling is retained until byte target decode");
    assert_eq!(retained["payload"].as_str(), Some("SGVsbG8*"));
    assert_eq!(
        retained["payload"]
            .as_tagged()
            .expect("binary tag retained")
            .tag,
        yaml::Tag::new("!!binary")
    );

    let error = LoadOptions::yaml_version_directive()
        .from_str::<LegacyBinaryPayload>(&source)
        .expect_err("invalid binary payload rejects typed byte target");
    assert!(
        error
            .to_string()
            .contains("invalid explicit !!binary scalar"),
        "{error}"
    );
    assert_eq!(error.span().line, 3);
    assert_eq!(error.span().column, 19);
}

#[test]
fn yaml11_legacy_numeric_key_collision_fixture_keeps_default_safe() {
    let source = read_fixture("legacy-numeric-key-collision.yaml");

    let default: Value = yaml::from_str(&source).expect("default keeps decimal key identity safe");
    let Value::Mapping(default_entries) = default else {
        panic!("expected default mapping");
    };
    assert_eq!(default_entries.len(), 2);

    let explicit = LoadOptions::yaml_1_1()
        .parse_str(&source)
        .expect_err("explicit YAML 1.1 numeric keys collide");
    assert!(explicit.to_string().contains("duplicate mapping key `8`"));
    assert_eq!(explicit.span().line, 4);
    assert_eq!(explicit.span().column, 1);

    let directive = LoadOptions::yaml_version_directive()
        .parse_str(&source)
        .expect_err("directive-driven YAML 1.1 numeric keys collide");
    assert!(directive.to_string().contains("duplicate mapping key `8`"));
    assert_eq!(directive.span().line, 4);
    assert_eq!(directive.span().column, 1);
}

fn assert_tagged_payload(source: &str, handle: &str, suffix: &str, shape: &str) {
    let value: Value = yaml::from_str(source).expect("tagged collection value");
    let tagged = value.as_tagged().expect("collection tag is retained");
    assert_eq!(tagged.tag.handle, handle);
    assert_eq!(tagged.tag.suffix, suffix);
    match (&tagged.value, shape) {
        (Value::Mapping(_), "mapping") | (Value::Sequence(_), "sequence") => {}
        (other, _) => panic!("unexpected tagged payload shape {other:?}"),
    }
}

fn assert_tagged_key(mapping: &Value, tag: yaml::Tag, key: &str, expected: &str) {
    let mapping = mapping.as_mapping().expect("tagged-key mapping");
    assert!(
        mapping.iter().any(
            |(candidate, value)| matches!(candidate, Value::Tagged(tagged)
            if tagged.tag == tag
                && tagged.value.as_str() == Some(key)
                && value.as_str() == Some(expected))
        ),
        "expected tagged key {tag:?} {key:?}: {expected:?}"
    );
}

fn assert_yaml11_merge_edge_value(value: &Value) {
    for target in ["repeated_merge", "repeated_tagged_merge"] {
        assert!(value[target]["<<"].is_null(), "{target} merge key removed");
        assert_eq!(value[target]["shared"].as_str(), Some("second"));
        assert_eq!(value[target]["image"].as_str(), Some("app:second"));
        assert_eq!(value[target]["retries"].as_u64(), Some(3));
        assert_eq!(value[target]["timeout"].as_u64(), Some(10));
        assert_eq!(value[target]["keep"].as_str(), Some("value"));
    }

    assert!(value["override_merge"]["<<"].is_null());
    assert_eq!(value["override_merge"]["shared"].as_str(), Some("explicit"));
    assert_eq!(
        value["override_merge"]["image"].as_str(),
        Some("app:second")
    );
    assert_eq!(value["override_merge"]["retries"].as_u64(), Some(3));
    assert_eq!(value["override_merge"]["timeout"].as_u64(), Some(10));

    assert_eq!(value["scalar_merge"]["<<"].as_str(), Some("scalar"));
    assert_eq!(value["scalar_merge"]["keep"].as_str(), Some("value"));

    assert_tagged_key(
        &value["tagged_scalar_merge"],
        yaml::Tag::new("!!merge"),
        "<<",
        "literal",
    );
    assert_eq!(
        value["sequence_scalar_merge"]["<<"][0].as_str(),
        Some("scalar")
    );

    assert_tagged_key(
        &value["literal_and_merge"],
        yaml::Tag::new("!!str"),
        "<<",
        "literal",
    );
    assert_eq!(
        value["literal_and_merge"]["image"].as_str(),
        Some("explicit")
    );
    assert_eq!(value["literal_and_merge"]["shared"].as_str(), Some("first"));
}

fn assert_legacy_pack_public_entrypoints(
    options: LoadOptions,
    source: &str,
    expected: &LegacyMigrationPack,
    label: &str,
) {
    let from_slice: LegacyMigrationPack = options
        .from_slice(source.as_bytes())
        .unwrap_or_else(|error| panic!("{label} from_slice: {error}"));
    let from_reader: LegacyMigrationPack = options
        .from_reader(Cursor::new(source.as_bytes()))
        .unwrap_or_else(|error| panic!("{label} from_reader: {error}"));
    let parsed_bytes = options
        .parse_bytes(source.as_bytes())
        .unwrap_or_else(|error| panic!("{label} parse_bytes: {error}"));
    let parsed_str = options
        .parse_str(source)
        .unwrap_or_else(|error| panic!("{label} parse_str: {error}"));
    let document_nodes = options
        .parse_documents(source)
        .unwrap_or_else(|error| panic!("{label} parse_documents: {error}"));
    let document_values: Vec<LegacyMigrationPack> = options
        .from_documents_str(source)
        .unwrap_or_else(|error| panic!("{label} from_documents_str: {error}"));
    let document_values_slice: Vec<LegacyMigrationPack> = options
        .from_documents_slice(source.as_bytes())
        .unwrap_or_else(|error| panic!("{label} from_documents_slice: {error}"));
    let document_values_reader: Vec<LegacyMigrationPack> = options
        .from_documents_reader(Cursor::new(source.as_bytes()))
        .unwrap_or_else(|error| panic!("{label} from_documents_reader: {error}"));
    let direct_slice =
        LegacyMigrationPack::deserialize(options.deserializer_from_slice(source.as_bytes()))
            .unwrap_or_else(|error| panic!("{label} deserializer_from_slice: {error}"));
    let direct_reader = LegacyMigrationPack::deserialize(
        options.deserializer_from_reader(Cursor::new(source.as_bytes())),
    )
    .unwrap_or_else(|error| panic!("{label} deserializer_from_reader: {error}"));
    let from_node: LegacyMigrationPack =
        yaml::from_node(&parsed_str).unwrap_or_else(|error| panic!("{label} from_node: {error}"));
    let from_value: LegacyMigrationPack = yaml::from_value(Value::from(&parsed_str))
        .unwrap_or_else(|error| panic!("{label} from_value: {error}"));
    let lossless = yaml::parse_lossless(source)
        .unwrap_or_else(|error| panic!("{label} parse_lossless: {error}"));

    assert_eq!(&from_slice, expected, "{label} from_slice");
    assert_eq!(&from_reader, expected, "{label} from_reader");
    assert_eq!(&direct_slice, expected, "{label} deserializer_from_slice");
    assert_eq!(&direct_reader, expected, "{label} deserializer_from_reader");
    assert_eq!(&from_node, expected, "{label} from_node");
    assert_eq!(&from_value, expected, "{label} from_value");
    assert_eq!(document_values, vec![expected.clone()], "{label} documents");
    assert_eq!(
        document_values_slice,
        vec![expected.clone()],
        "{label} documents slice"
    );
    assert_eq!(
        document_values_reader,
        vec![expected.clone()],
        "{label} documents reader"
    );
    assert_eq!(document_nodes.len(), 1, "{label} parsed document count");
    assert!(Value::from(&parsed_bytes).equivalent(&Value::from(&parsed_str)));
    assert_eq!(lossless.as_source(), source);
    assert_eq!(lossless.to_string(), source);
    assert_eq!(
        lossless.documents()[0]
            .directives()
            .yaml_version
            .as_ref()
            .expect("YAML 1.1 directive retained")
            .minor,
        1
    );
}

fn assert_legacy_duplicate_key_error_entrypoints(
    options: LoadOptions,
    source: &str,
    fixture: &str,
) {
    for (entrypoint, error) in [
        (
            "parse_bytes",
            options.parse_bytes(source.as_bytes()).unwrap_err(),
        ),
        (
            "from_slice",
            options.from_slice::<Value>(source.as_bytes()).unwrap_err(),
        ),
        (
            "from_reader",
            options
                .from_reader::<_, Value>(Cursor::new(source.as_bytes()))
                .unwrap_err(),
        ),
        (
            "from_documents_slice",
            options
                .from_documents_slice::<Value>(source.as_bytes())
                .unwrap_err(),
        ),
        (
            "from_documents_reader",
            options
                .from_documents_reader::<Value, _>(Cursor::new(source.as_bytes()))
                .unwrap_err(),
        ),
        (
            "deserializer_from_slice",
            Value::deserialize(options.deserializer_from_slice(source.as_bytes())).unwrap_err(),
        ),
        (
            "deserializer_from_reader",
            Value::deserialize(options.deserializer_from_reader(Cursor::new(source.as_bytes())))
                .unwrap_err(),
        ),
    ] {
        assert!(
            error.to_string().contains("duplicate mapping key"),
            "{fixture} {entrypoint} unexpected error: {error}"
        );
        assert_eq!(error.span().line, 4, "{fixture} {entrypoint} line");
        assert_eq!(error.span().column, 1, "{fixture} {entrypoint} column");
    }
}

fn assert_value_sequences_equivalent(actual: Vec<Value>, expected: &[Value], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label} document count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual.equivalent(expected),
            "{label} document {index} mismatch: {actual:?} != {expected:?}"
        );
    }
}

fn manifest() -> Manifest {
    toml::from_str(MANIFEST).expect("YAML 1.1 conformance manifest parses")
}

fn read_fixture(path: &str) -> String {
    fs::read_to_string(Path::new(FIXTURE_ROOT).join(path))
        .unwrap_or_else(|error| panic!("read fixture {path}: {error}"))
}

fn fixture_paths(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .map(|entry| {
            let path: PathBuf = entry
                .unwrap_or_else(|error| panic!("read entry in {}: {error}", root.display()))
                .path();
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("fixture path is UTF-8: {}", path.display()))
                .to_string()
        })
        .filter(|path| path.ends_with(".yaml"))
        .collect()
}
