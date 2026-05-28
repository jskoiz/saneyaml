use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyMigrationPack {
    flags: LegacyFlags,
    numbers: LegacyNumbers,
    timestamps: LegacyTimestamps,
    payload: Vec<u8>,
    set: BTreeSet<String>,
    omap: BTreeMap<String, i64>,
    pairs: Vec<(String, i64)>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyFlags {
    deploy: bool,
    dry_run: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyNumbers {
    file_mode: i64,
    hex_limit: i64,
    session: i64,
    invalid_octal: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyTimestamps {
    release: Timestamp,
}

#[derive(Debug, Deserialize, PartialEq)]
struct LegacyServiceConfig {
    service: LegacyService,
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
    assert_eq!(manifest.case.len(), 10);
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
            "set" | "omap" | "pairs" | "bundle" | "scalar-matrix" | "merge" | "duplicate-key"
        ));
        assert!(matches!(case.expected.as_str(), "accept" | "error"));
        assert!(
            case.tag == "!!set"
                || case.tag == "!!omap"
                || case.tag == "!!pairs"
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
