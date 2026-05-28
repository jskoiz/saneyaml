use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use yaml::Value;

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

#[test]
fn yaml11_collection_tag_manifest_is_complete() {
    let manifest = manifest();
    assert_eq!(manifest.case.len(), 7);
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
            "set" | "omap" | "pairs" | "bundle"
        ));
        assert!(matches!(case.expected.as_str(), "accept" | "error"));
        assert!(
            case.tag == "!!set"
                || case.tag == "!!omap"
                || case.tag == "!!pairs"
                || case.tag.starts_with("tag:yaml.org,2002:")
                || case.tag.starts_with("%TAG ")
        );
    }
}

#[test]
fn yaml11_collection_tags_deserialize_to_typed_rust_collections() {
    for case in manifest()
        .case
        .into_iter()
        .filter(|case| case.expected == "accept")
    {
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
