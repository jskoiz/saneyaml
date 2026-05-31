use proptest::prelude::*;
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeOwned, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Cursor,
    path::{Path, PathBuf},
};
use yaml::{
    AnchorId, CollectionStyle, Error, Event, EventDocumentDirectives, EventMeta, LoadOptions,
    LosslessNodeKind, LosslessStream, LosslessTriviaKind, Node, NodeId, NodeValue, Number, Schema,
    Span, Tag, TaggedNode, Value,
};

const PARSE_BYTES_REQUIRED_SEEDS: &[&str] = &[
    "yaml11-alias-key-collision",
    "yaml11-scalar-denominator",
    "yaml11-signed-zero-key-collision",
    "yts-8g76-comment-lines",
    "yts-98yd-comment-only",
    "yts-k54u-tab-after-document-header",
];
const SERDE_ENTRYPOINTS_REQUIRED_SEEDS: &[&str] = &[
    "yaml11-alias-key-collision",
    "yaml11-omap-non-singleton-entry",
    "yaml11-pairs-scalar-entry",
    "yaml11-scalar-denominator",
    "yaml11-set-non-null-values",
    "yaml11-signed-zero-key-collision",
    "yaml11-value-duplicate-key",
    "yaml11-value-resolved-handle",
    "yts-fh7j-tags-on-empty-scalars",
    "yts-wz62-empty-tagged-flow-values",
];
const SERDE_SERIALIZER_REQUIRED_SEEDS: &[&str] = &[
    "bytes-rejection",
    "document-stream",
    "enum-helper-shape",
    "kubernetes-resource",
    "nested-options",
    "openapi-shape",
    "struct-config",
];
const EVENT_STREAM_REQUIRED_SEEDS: &[&str] = &[
    "alias-expansion-bomb",
    "alias-recursive-flow",
    "docker-compose-anchors",
    "directive-tag-handle",
    "yts-3r3p-root-sequence-anchor",
    "yts-5tym-local-tag-prefix-stream",
    "yts-6kgn-empty-anchor-alias",
    "yts-7fwl-verbatim-tags",
    "yts-7bmt-anchor-key-properties",
    "yts-7bub-commented-alias",
    "yts-8g76-comment-lines",
    "yts-98yd-comment-only",
    "yts-cn3r-flow-anchor-properties",
    "yts-cup7-tagged-anchor-alias",
    "yts-e76z-anchor-alias-keys",
    "yts-k54u-tab-after-document-header",
    "yts-q9wf-separation-spaces-flow-mapping",
    "yts-ugm3-invoice-tag-anchor-alias",
    "yts-y2gn-colon-anchor-name",
    "yts-zwk4-anchor-explicit-key",
    "yts-zxt5-implicit-key-adjacent-newline",
];
const EMIT_ROUNDTRIP_REQUIRED_SEEDS: &[&str] = &[
    "anchors-and-aliases",
    "default-merge",
    "github-actions-anchors",
    "helm-block-merge",
    "openapi-flow-tags",
    "yaml11-directive-tags-merge",
];
const APPLY_MERGE_REQUIRED_SEEDS: &[&str] = &[
    "apply-merge-scalar-error",
    "apply-merge-sequence-element-error",
    "apply-merge-single",
    "apply-merge-tagged-container-recursion",
    "docker-compose-compose-anchors",
];
const SCHEMA_MODES_REQUIRED_SEEDS: &[&str] = &[
    "yaml11-alias-key-collision",
    "yaml11-collection-tags",
    "yaml11-directive-driven",
    "yaml11-explicit-merge-tags",
    "yaml11-omap-non-singleton-entry",
    "yaml11-pairs-scalar-entry",
    "yaml11-scalar-edge-stream",
    "yaml11-scalar-denominator",
    "yaml11-set-non-null-values",
    "yaml11-signed-zero-key-collision",
    "yaml11-value-duplicate-key",
    "yaml11-value-resolved-handle",
    "yaml12-config-words",
    "yts-fh7j-tags-on-empty-scalars",
    "yts-wz62-empty-tagged-flow-values",
];
const LOSSLESS_GRAPH_REQUIRED_SEEDS: &[&str] = &[
    "comments_anchor.yml",
    "document_reset_anchor.yml",
    "github_actions_comments_markers.yml",
    "helm_block_scalar_merge.yml",
    "multi_doc_merge_anchor_reset.yml",
    "openapi_flow_anchor_tags.yml",
    "recursive_alias.yml",
    "yaml11_merge_comments_alias_graph.yml",
    "yaml11_recursive_merge_comments.yml",
    "yts-3r3p-root-sequence-anchor.yml",
    "yts-6kgn-empty-anchor-alias.yml",
    "yts-7bmt-anchor-key-properties.yml",
    "yts-7bub-commented-alias.yml",
    "yts-cn3r-flow-anchor-properties.yml",
    "yts-cup7-tagged-anchor-alias.yml",
    "yts-e76z-anchor-alias-keys.yml",
    "yts-y2gn-colon-anchor-name.yml",
    "yts-zwk4-anchor-explicit-key.yml",
];
const LOSSLESS_EDIT_REQUIRED_SEEDS: &[&str] = &[
    "delete-compose-root-source",
    "insert-directive-document-comment",
    "insert-document-comment",
    "replace-flow-mapping",
    "replace-real-world-block-scalar",
    "replace-scalar",
    "replace-tagged-block-scalar",
    "source-multi-doc-merge-replace",
    "structural-map-delete",
    "structural-map-insert",
    "structural-map-merge-comment-insert",
    "structural-map-replace",
    "structural-flow-map-delete",
    "structural-flow-map-insert",
    "structural-flow-openapi-seq-insert",
    "structural-flow-sequence-delete",
    "structural-flow-sequence-insert",
    "structural-sequence-delete",
    "structural-sequence-insert",
    "structural-sequence-replace",
];

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        max_shrink_iters: 1024,
        ..ProptestConfig::default()
    })]

    #[test]
    fn parser_never_panics_on_bytes(input in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let result = std::panic::catch_unwind(|| yaml::parse_bytes(&input))
            .expect("parser must not panic");
        assert_parse_result_invariants(&input, &result);
    }

    #[test]
    fn invalid_utf8_diagnostics_stay_in_bounds(prefix in proptest::collection::vec(prop_oneof![
        Just(b'a'),
        Just(b'\n'),
        Just(b' '),
        Just(b':'),
    ], 0..512)) {
        let mut input = prefix;
        input.push(0xFF);
        std::panic::catch_unwind(|| {
            let error = yaml::parse_bytes(&input).expect_err("inserted invalid UTF-8");
            let span = error.span();
            prop_assert!(span.start <= input.len());
            prop_assert!(span.end <= input.len());
            prop_assert!(span.line >= 1);
            prop_assert!(span.column >= 1);
            assert_error_invariants(&input, &error);
            Ok::<(), TestCaseError>(())
        }).expect("parser must not panic on invalid UTF-8 diagnostics")?;
    }

    #[test]
    fn parser_never_panics_on_utf8(input in "\\PC{0,4096}") {
        let result = std::panic::catch_unwind(|| yaml::parse_str(&input))
            .expect("parser must not panic");
        assert_parse_result_invariants(input.as_bytes(), &result);
    }

    #[test]
    fn event_stream_never_panics_on_utf8(input in "\\PC{0,4096}") {
        let result = std::panic::catch_unwind(|| yaml::parse_events(&input))
            .expect("event parser must not panic");
        assert_event_result_invariants(input.as_bytes(), &result);
    }

    #[test]
    fn emitted_generated_trees_parse_equivalently(node in arb_node(4)) {
        if has_non_roundtrippable_explicit_core_scalar_tag(&node) {
            return Ok(());
        }
        let emitted = match yaml::to_string(&node) {
            Ok(emitted) => emitted,
            Err(error) if error.to_string().contains("duplicate mapping key") => return Ok(()),
            Err(error) if error.to_string().contains("literal YAML merge keys") => return Ok(()),
            Err(error)
                if error
                    .to_string()
                    .contains("nested YAML tags cannot be emitted directly") =>
            {
                return Ok(());
            }
            Err(error) => {
                return Err(TestCaseError::fail(format!("unexpected emit error: {error}")));
            }
        };
        let reparsed = yaml::parse_str(&emitted).expect("parse emitted node");
        prop_assert!(reparsed.equivalent(&node));
        let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed node");
        prop_assert_eq!(emitted_again, emitted);
    }

    #[test]
    fn from_node_and_from_value_agree_for_generated_values(node in arb_node(4)) {
        let from_node: Value = yaml::from_node(&node).expect("from_node value");
        let from_value: Value = yaml::from_value(Value::from(node.clone())).expect("from_value value");
        prop_assert!(from_node.equivalent(&from_value));
        prop_assert!(from_value.equivalent(&Value::from(&node)));
    }
}

#[test]
fn parser_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| {
            let result = yaml::parse_bytes(&input);
            assert_parse_result_invariants(&input, &result);
        })
        .unwrap_or_else(|_| panic!("parser must not panic on corpus fixture {name}"));
    }
}

#[test]
fn parse_bytes_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("parse_bytes") {
        std::panic::catch_unwind(|| {
            let result = yaml::parse_bytes(&input);
            assert_parse_result_invariants(&input, &result);
        })
        .unwrap_or_else(|_| panic!("parser must not panic on parse_bytes fuzz corpus {name}"));
    }
}

#[test]
fn event_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| {
            if let Ok(text) = std::str::from_utf8(&input) {
                let result = yaml::parse_events(text);
                assert_event_result_invariants(&input, &result);
            }
        })
        .unwrap_or_else(|_| panic!("event parser must not panic on corpus fixture {name}"));
    }
}

#[test]
fn event_stream_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("event_stream") {
        std::panic::catch_unwind(|| {
            if let Ok(text) = std::str::from_utf8(&input) {
                let result = yaml::parse_events(text);
                assert_event_result_invariants(&input, &result);
            }
        })
        .unwrap_or_else(|_| {
            panic!("event parser must not panic on event_stream fuzz corpus {name}")
        });
    }
}

#[test]
fn emit_roundtrip_fuzz_corpus_emits_stably() {
    for (name, input) in fuzz_corpus_inputs("emit_roundtrip") {
        std::panic::catch_unwind(|| {
            if let Ok(node) = yaml::parse_bytes(&input) {
                assert_emit_roundtrip_invariants(&node);
            }
        })
        .unwrap_or_else(|_| panic!("emitter must not drift on emit_roundtrip fuzz corpus {name}"));
    }
}

#[test]
fn serde_corpus_entrypoints_keep_diagnostics_in_bounds() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| assert_serde_entrypoint_invariants(&input)).unwrap_or_else(
            |_| panic!("Serde entrypoints must not panic on corpus fixture {name}"),
        );
    }
}

#[test]
fn serde_entrypoints_fuzz_corpus_keeps_diagnostics_in_bounds() {
    for (name, input) in fuzz_corpus_inputs("serde_entrypoints") {
        std::panic::catch_unwind(|| assert_serde_entrypoint_invariants(&input)).unwrap_or_else(
            |_| panic!("Serde entrypoints must not panic on serde_entrypoints fuzz corpus {name}"),
        );
    }
}

#[test]
fn schema_modes_fuzz_corpus_keeps_diagnostics_in_bounds() {
    for (name, input) in fuzz_corpus_inputs("schema_modes") {
        std::panic::catch_unwind(|| assert_schema_mode_invariants(&input)).unwrap_or_else(|_| {
            panic!("schema modes must not panic on schema_modes fuzz corpus {name}")
        });
    }
}

#[test]
fn lossless_graph_fuzz_corpus_keeps_graph_and_span_invariants() {
    for (name, input) in fuzz_corpus_inputs("lossless_graph") {
        std::panic::catch_unwind(|| assert_lossless_graph_invariants(&input)).unwrap_or_else(
            |_| panic!("lossless graph must not panic on lossless_graph fuzz corpus {name}"),
        );
    }
}

#[test]
fn lossless_edit_fuzz_corpus_validates_edits_or_diagnostics() {
    for (name, input) in fuzz_corpus_inputs("lossless_edit") {
        std::panic::catch_unwind(|| assert_lossless_edit_invariants(&input)).unwrap_or_else(|_| {
            panic!("lossless edits must not panic on lossless_edit fuzz corpus {name}")
        });
    }
}

#[test]
fn apply_merge_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
        .chain(apply_merge_synthetic_inputs())
    {
        std::panic::catch_unwind(|| assert_apply_merge_invariants(&input)).unwrap_or_else(|_| {
            panic!("Value::apply_merge must not panic on corpus fixture {name}")
        });
    }
}

#[test]
fn apply_merge_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("apply_merge") {
        std::panic::catch_unwind(|| assert_apply_merge_invariants(&input)).unwrap_or_else(|_| {
            panic!("Value::apply_merge must not panic on apply_merge fuzz corpus {name}")
        });
    }
}

#[test]
fn apply_merge_semantic_corpus_matches_serde_yaml() {
    for (name, input) in apply_merge_semantic_inputs().chain(fuzz_corpus_inputs("apply_merge")) {
        std::panic::catch_unwind(|| assert_apply_merge_semantics(&input)).unwrap_or_else(|_| {
            panic!("Value::apply_merge semantics must match serde_yaml for {name}")
        });
    }
}

#[test]
fn fuzz_corpora_cover_release_targets_and_named_safety_seeds() {
    let expected: BTreeMap<&str, (usize, &[&str])> = BTreeMap::from([
        ("apply_merge", (16, APPLY_MERGE_REQUIRED_SEEDS)),
        ("emit_roundtrip", (14, EMIT_ROUNDTRIP_REQUIRED_SEEDS)),
        ("event_stream", (101, EVENT_STREAM_REQUIRED_SEEDS)),
        ("lossless_edit", (28, LOSSLESS_EDIT_REQUIRED_SEEDS)),
        ("lossless_graph", (29, LOSSLESS_GRAPH_REQUIRED_SEEDS)),
        ("parse_bytes", (884, PARSE_BYTES_REQUIRED_SEEDS)),
        ("schema_modes", (24, SCHEMA_MODES_REQUIRED_SEEDS)),
        ("serde_entrypoints", (292, SERDE_ENTRYPOINTS_REQUIRED_SEEDS)),
        ("serde_serializer", (7, SERDE_SERIALIZER_REQUIRED_SEEDS)),
    ]);
    let expected_targets = expected.keys().copied().collect::<BTreeSet<_>>();
    assert_eq!(declared_fuzz_targets(), expected_targets);
    assert_eq!(fuzz_corpus_targets(), expected_targets);

    for (target, (minimum_files, required_seeds)) in expected {
        let seeds = fuzz_corpus_file_names(target);
        assert!(
            seeds.len() >= minimum_files,
            "{target} corpus has {} files; release floor is {minimum_files}",
            seeds.len()
        );
        for required_seed in required_seeds {
            assert!(
                seeds.contains(*required_seed),
                "{target} corpus must keep release safety seed {required_seed}"
            );
        }
    }
}

#[test]
fn malformed_block_scalar_header_corpus_rejects_with_in_bounds_spans() {
    for (name, input) in malformed_block_scalar_header_inputs() {
        for error in [
            yaml::parse_str(input).expect_err("parse_str rejects malformed block scalar header"),
            yaml::parse_events(input)
                .expect_err("parse_events rejects malformed block scalar header"),
            yaml::from_str::<Value>(input)
                .expect_err("from_str rejects malformed block scalar header"),
        ] {
            assert!(
                error.to_string().contains("invalid block scalar header"),
                "{name} unexpected error: {error}"
            );
            assert_error_invariants(input.as_bytes(), &error);
        }
    }
}

#[test]
fn duplicate_tree_key_corpus_rejects_tree_loading_with_in_bounds_spans() {
    for (name, input) in duplicate_tree_key_inputs() {
        yaml::parse_events(input).expect("raw events preserve duplicate collection keys");
        for error in [
            yaml::parse_str(input).expect_err("parse_str rejects duplicate tree key"),
            yaml::from_str::<Value>(input).expect_err("from_str rejects duplicate tree key"),
        ] {
            assert!(
                error.to_string().contains("duplicate mapping key"),
                "{name} unexpected error: {error}"
            );
            assert!(
                !error.diagnostic().related.is_empty(),
                "{name} reports previous key span"
            );
            assert_error_invariants(input.as_bytes(), &error);
        }
    }
}

fn fixture_inputs() -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut inputs = Vec::new();
    for relative in ["yaml-test-suite/data", "real-world", "divergences"] {
        collect_yaml_fixtures(&root.join(relative), &root, &mut inputs);
    }
    inputs
}

fn fuzz_corpus_inputs(target: &str) -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/corpus")
        .join(target);
    let mut inputs = Vec::new();
    collect_all_files(&root, &root, &mut inputs);
    inputs
}

fn fuzz_corpus_file_names(target: &str) -> BTreeSet<String> {
    fuzz_corpus_inputs(target)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

fn fuzz_corpus_targets() -> BTreeSet<&'static str> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus");
    fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("read corpus root {}: {error}", root.display()))
        .map(|entry| {
            let path = entry
                .unwrap_or_else(|error| panic!("read corpus root entry: {error}"))
                .path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("corpus dir has UTF-8 filename: {}", path.display()));
            match name {
                "apply_merge" => "apply_merge",
                "emit_roundtrip" => "emit_roundtrip",
                "event_stream" => "event_stream",
                "lossless_edit" => "lossless_edit",
                "lossless_graph" => "lossless_graph",
                "parse_bytes" => "parse_bytes",
                "schema_modes" => "schema_modes",
                "serde_entrypoints" => "serde_entrypoints",
                "serde_serializer" => "serde_serializer",
                other => panic!("unexpected fuzz corpus target {other}"),
            }
        })
        .collect()
}

fn declared_fuzz_targets() -> BTreeSet<&'static str> {
    let manifest: toml::Value =
        toml::from_str(include_str!("../fuzz/Cargo.toml")).expect("fuzz Cargo.toml parses");
    manifest["bin"]
        .as_array()
        .expect("fuzz Cargo.toml declares bin targets")
        .iter()
        .map(|bin| {
            let name = bin["name"].as_str().expect("fuzz bin declares name");
            match name {
                "apply_merge" => "apply_merge",
                "emit_roundtrip" => "emit_roundtrip",
                "event_stream" => "event_stream",
                "lossless_edit" => "lossless_edit",
                "lossless_graph" => "lossless_graph",
                "parse_bytes" => "parse_bytes",
                "schema_modes" => "schema_modes",
                "serde_entrypoints" => "serde_entrypoints",
                "serde_serializer" => "serde_serializer",
                other => panic!("unexpected fuzz target {other}"),
            }
        })
        .collect()
}

fn collect_yaml_fixtures(path: &Path, root: &Path, inputs: &mut Vec<(String, Vec<u8>)>) {
    if path.is_dir() {
        let mut children = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read fixture directory {}: {error}", path.display()))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| {
                        panic!("read fixture directory entry {}: {error}", path.display())
                    })
                    .path()
            })
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            collect_yaml_fixtures(&child, root, inputs);
        }
        return;
    }

    if !matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yaml" | "yml")
    ) {
        return;
    }

    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let input = fs::read(path)
        .unwrap_or_else(|error| panic!("read fixture file {}: {error}", path.display()));
    inputs.push((name, input));
}

fn collect_all_files(path: &Path, root: &Path, inputs: &mut Vec<(String, Vec<u8>)>) {
    if path.is_dir() {
        let mut children = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read corpus directory {}: {error}", path.display()))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| {
                        panic!("read corpus directory entry {}: {error}", path.display())
                    })
                    .path()
            })
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            collect_all_files(&child, root, inputs);
        }
        return;
    }

    if !path.is_file() {
        return;
    }

    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let input = fs::read(path)
        .unwrap_or_else(|error| panic!("read corpus file {}: {error}", path.display()));
    inputs.push((name, input));
}

fn synthetic_corpus_inputs() -> impl Iterator<Item = (String, Vec<u8>)> {
    [
        (
            "flow-anchor-only-null-nodes",
            b"root: [&empty, *empty]\nkeyed: {? &key : value}\n".as_slice(),
        ),
        ("root-literal-indented", b"|\n  line\n".as_slice()),
        ("root-folded-indented", b">\n  first\n  second\n".as_slice()),
        ("blank-separated-indicator", b"@\n\n!".as_slice()),
        ("blank-separated-plain", b"@\n\ng".as_slice()),
        ("blank-separated-control", b"\x01\n\n\x01".as_slice()),
        (
            "root-literal-indent-zero-percent",
            b"|\n%!PS-Adobe-2.0 # Not the first line\n".as_slice(),
        ),
        (
            "yaml-double-quoted-escapes",
            b"x: \"\\e\"\nroot: [\"\\a\", \"\\v\", \"\\_\", \"\\N\", \"\\L\", \"\\P\"]\n"
                .as_slice(),
        ),
        (
            "yaml-double-quoted-even-backslash-fold",
            b"value: \"a\\\\\n  b\"\n".as_slice(),
        ),
        (
            "large-integer-widths",
            b"i128_max: 170141183460469231731687303715884105727\nu128_max: 340282366920938463463374607431768211455\nu128_overflow: 340282366920938463463374607431768211456\n".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, input)| (name.to_string(), input.to_vec()))
}

fn malformed_block_scalar_header_inputs() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        ("mapping-alpha", "key: |x\n"),
        ("mapping-zero", "key: |0\n"),
        ("mapping-chomping", "key: |+-\n"),
        ("mapping-space", "key: | x\n"),
        ("root-alpha", "|x\n"),
        ("document-indent-zero", "--- |0\n"),
        ("document-two-digit-indent", "--- |10\n"),
        ("sequence-chomping", "- |+-\n"),
    ]
    .into_iter()
}

fn duplicate_tree_key_inputs() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        (
            "yts-x38w",
            include_str!("fixtures/yaml-test-suite/data/X38W/in.yaml"),
        ),
        (
            "sequence-key",
            "root: {? [a, b]: first, ? [a, b]: second}\n",
        ),
        ("mapping-key", "root: {? {x: y}: first, ? {x: y}: second}\n"),
        (
            "tagged-scalar-key",
            "root: {!Tag dup: first, dup: second}\n",
        ),
        (
            "tagged-sequence-key",
            "root: {!Tag [a, b]: first, [a, b]: second}\n",
        ),
        (
            "tagged-mapping-key",
            "root: {!Tag {x: y}: first, {x: y}: second}\n",
        ),
        (
            "permuted-mapping-key",
            "root: {? {a: 1, b: 2}: first, ? {b: 2, a: 1}: second}\n",
        ),
        (
            "alias-expanded-permuted-mapping-key",
            "left: &left {a: 1, b: 2}\nright: &right {b: 2, a: 1}\nroot: {? *left : first, ? *right : second}\n",
        ),
        (
            "tagged-permuted-mapping-key",
            "root: {!Tag {a: 1, b: 2}: first, {b: 2, a: 1}: second}\n",
        ),
        (
            "signed-zero-float-key",
            "root:\n  0.0: positive\n  -0.0: negative\n",
        ),
    ]
    .into_iter()
}

fn apply_merge_synthetic_inputs() -> impl Iterator<Item = (String, Vec<u8>)> {
    [
        (
            "apply-merge-single",
            b"base: &base {a: 1}\ntarget: {<<: *base, b: 2}\n".as_slice(),
        ),
        (
            "apply-merge-list",
            b"first: &first {shared: first}\nsecond: &second {shared: second, b: 2}\ntarget: {<<: [*first, *second]}\n"
                .as_slice(),
        ),
        ("apply-merge-scalar-error", b"target: {<<: scalar}\n".as_slice()),
        (
            "apply-merge-sequence-element-error",
            b"target: {<<: [scalar]}\n".as_slice(),
        ),
        (
            "apply-merge-tagged-error",
            b"target: {<<: !Thing {a: 1}}\n".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, input)| (name.to_string(), input.to_vec()))
}

fn apply_merge_semantic_inputs() -> impl Iterator<Item = (String, Vec<u8>)> {
    [
        (
            "apply-merge-block-style",
            b"base: &base\n  retries: 3\n  timeout: 10\ntarget:\n  <<: *base\n  timeout: 20\n"
                .as_slice(),
        ),
        (
            "apply-merge-list-precedence",
            b"first: &first {shared: first, a: 1}\nsecond: &second {shared: second, b: 2}\ntarget:\n  <<: [*first, *second]\n  b: explicit\n"
                .as_slice(),
        ),
        (
            "apply-merge-nested-source",
            b"base: &base\n  <<: {a: 1}\n  b: 2\ntarget:\n  <<: *base\n"
                .as_slice(),
        ),
        (
            "apply-merge-sequence-recursive",
            b"base: &base {retries: 3, timeout: 10}\njobs:\n  - name: build\n    config:\n      <<: *base\n      timeout: 20\n"
                .as_slice(),
        ),
        (
            "apply-merge-invalid-scalar",
            b"target: {<<: scalar}\n".as_slice(),
        ),
        (
            "apply-merge-invalid-sequence-element",
            b"target: {<<: [scalar]}\n".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, input)| (name.to_string(), input.to_vec()))
}

#[derive(Debug, Deserialize)]
struct BorrowedConfig<'a> {
    name: &'a str,
    path: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct BorrowedVars<'a> {
    vars: BTreeMap<&'a str, &'a str>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OwnedReaderConfig {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    ints: BTreeMap<String, i128>,
    #[serde(default)]
    uints: BTreeMap<String, u128>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedAliasValues {
    first: String,
    second: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct RootStringConfig {
    root: BTreeMap<String, String>,
    #[serde(default)]
    alias_value: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ValueStructuralTags {
    value_key: String,
    #[serde(default)]
    value_mapping: BTreeMap<String, String>,
}

#[derive(Debug)]
struct FuzzBytes;

struct FuzzByteVisitor;

impl<'de> Visitor<'de> for FuzzByteVisitor {
    type Value = FuzzBytes;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bytes")
    }

    fn visit_bytes<E>(self, _value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }

    fn visit_borrowed_bytes<E>(self, _value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }

    fn visit_byte_buf<E>(self, _value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }
}

impl<'de> Deserialize<'de> for FuzzBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(FuzzByteVisitor)
    }
}

fn assert_serde_entrypoint_invariants(input: &[u8]) {
    assert_single_document_entrypoint(input);
    assert_document_stream_entrypoints(input);
    assert_reader_backed_entrypoints(input);
    assert_config_string_map_entrypoints(input);
    assert_numeric_map_entrypoints(input);
    assert_typed_reader_entrypoints(input);
    assert_yaml11_collection_tag_entrypoints(input);
    assert_borrowed_entrypoints(input);
    assert_byte_entrypoints(input);
}

fn assert_schema_mode_invariants(input: &[u8]) {
    for options in [
        LoadOptions::new(),
        LoadOptions::new().schema(Schema::Yaml11),
        LoadOptions::yaml_version_directive(),
    ] {
        match options.parse_bytes(input) {
            Ok(node) => assert_node_invariants(input, &node),
            Err(error) => assert_error_invariants(input, &error),
        }
        match options.from_slice::<Value>(input) {
            Ok(value) => {
                let node = options
                    .parse_bytes(input)
                    .expect("from_slice success must parse with same options");
                assert!(Value::from(&node).equivalent(&value));
            }
            Err(error) => assert_error_invariants(input, &error),
        }
        assert_yaml11_collection_tag_options(input, options);
    }
}

fn assert_lossless_graph_invariants(input: &[u8]) {
    let result = yaml::parse_lossless_bytes(input);
    if let Ok(input_str) = std::str::from_utf8(input)
        && yaml::parse_events(input_str).is_ok()
    {
        assert!(
            result.is_ok(),
            "parse_lossless rejected YAML accepted by parse_events: {result:?}"
        );
    }

    match result {
        Ok(stream) => assert_lossless_stream_invariants(input, &stream),
        Err(error) => {
            if let Some(location) = error.location() {
                assert!(location.index() <= input.len());
            }
        }
    }
}

const LOSSLESS_EDIT_REPLACEMENT_MARKER: &[u8] = b"=== yaml replacement ===\n";

#[derive(Clone, Copy)]
struct LosslessEditInput<'a> {
    mode: LosslessEditMode,
    selector: usize,
    source: &'a [u8],
    replacement: &'a str,
}

#[derive(Clone, Copy)]
enum LosslessEditMode {
    Node,
    Scalar,
    Source,
    Insert,
    Delete,
    MappingValue,
    MappingInsert,
    MappingDelete,
    SequenceItem,
    SequenceInsert,
    SequenceDelete,
}

#[derive(Clone, Copy)]
struct LosslessEditTarget {
    node: Option<NodeId>,
    span: Span,
}

fn assert_lossless_edit_invariants(input: &[u8]) {
    let Some(edit_input) = split_lossless_edit_input(input) else {
        return;
    };

    let stream = match yaml::parse_lossless_bytes(edit_input.source) {
        Ok(stream) => stream,
        Err(error) => {
            assert_error_invariants_allowing_unspanned(edit_input.source, &error);
            return;
        }
    };

    if assert_structural_lossless_edit_invariants(&stream, edit_input) {
        return;
    }

    let Some(target) = select_lossless_edit_target(&stream, edit_input) else {
        return;
    };
    let replacement = match edit_input.mode {
        LosslessEditMode::Delete => "",
        _ => edit_input.replacement,
    };
    let edited = build_lossless_edited_source(stream.as_source(), target.span, replacement)
        .expect("lossless node spans are valid source slices");

    let mut edit = stream.edit();
    let replace_result = match edit_input.mode {
        LosslessEditMode::Node => {
            edit.replace_node_source(target.node.expect("node target"), replacement)
        }
        LosslessEditMode::Scalar => {
            edit.replace_scalar_source(target.node.expect("scalar target"), replacement)
        }
        LosslessEditMode::Source => edit.replace_source_span(target.span, replacement),
        LosslessEditMode::Insert => edit.insert_source(target.span.start, replacement),
        LosslessEditMode::Delete => edit.delete_source_span(target.span),
        LosslessEditMode::MappingValue
        | LosslessEditMode::MappingInsert
        | LosslessEditMode::MappingDelete
        | LosslessEditMode::SequenceItem
        | LosslessEditMode::SequenceInsert
        | LosslessEditMode::SequenceDelete => {
            unreachable!("structural lossless edit modes are handled before raw edit dispatch")
        }
    };
    if let Err(error) = replace_result {
        assert_error_invariants_allowing_unspanned(edit_input.source, &error);
        return;
    }

    match edit.finish() {
        Ok(output) => {
            assert_eq!(output, edited);
            yaml::parse_lossless(&output).expect("successful edit output reparses losslessly");
        }
        Err(error) => assert_error_invariants_allowing_unspanned(edited.as_bytes(), &error),
    }
}

fn assert_structural_lossless_edit_invariants(
    stream: &LosslessStream,
    edit_input: LosslessEditInput<'_>,
) -> bool {
    let mut edit = stream.edit();
    let result = match edit_input.mode {
        LosslessEditMode::MappingValue => {
            let Some((mapping, key)) =
                select_scalar_keyed_mapping(stream, edit_input.selector, false)
            else {
                return true;
            };
            edit.replace_mapping_value_source(mapping, &key, edit_input.replacement)
        }
        LosslessEditMode::MappingInsert => {
            let Some(mapping) = select_mapping_insertion(stream, edit_input.selector) else {
                return true;
            };
            match mapping_style(stream, mapping) {
                Some(CollectionStyle::Block) => {
                    edit.insert_block_mapping_entry_source(mapping, edit_input.replacement)
                }
                Some(CollectionStyle::Flow) => {
                    edit.insert_flow_mapping_entry_source(mapping, edit_input.replacement)
                }
                None => return true,
            }
        }
        LosslessEditMode::MappingDelete => {
            let Some((mapping, key)) =
                select_scalar_keyed_mapping(stream, edit_input.selector, false)
            else {
                return true;
            };
            match mapping_style(stream, mapping) {
                Some(CollectionStyle::Block) => {
                    edit.delete_block_mapping_entry_source(mapping, &key)
                }
                Some(CollectionStyle::Flow) => edit.delete_flow_mapping_entry_source(mapping, &key),
                None => return true,
            }
        }
        LosslessEditMode::SequenceItem => {
            let Some((sequence, index)) = select_sequence_item(stream, edit_input.selector, false)
            else {
                return true;
            };
            edit.replace_sequence_item_source(sequence, index, edit_input.replacement)
        }
        LosslessEditMode::SequenceInsert => {
            let Some((sequence, index)) = select_sequence_insertion(stream, edit_input.selector)
            else {
                return true;
            };
            match sequence_style(stream, sequence) {
                Some(CollectionStyle::Block) => {
                    edit.insert_block_sequence_item_source(sequence, index, edit_input.replacement)
                }
                Some(CollectionStyle::Flow) => {
                    edit.insert_flow_sequence_item_source(sequence, index, edit_input.replacement)
                }
                None => return true,
            }
        }
        LosslessEditMode::SequenceDelete => {
            let Some((sequence, index)) = select_sequence_item(stream, edit_input.selector, false)
            else {
                return true;
            };
            match sequence_style(stream, sequence) {
                Some(CollectionStyle::Block) => {
                    edit.delete_block_sequence_item_source(sequence, index)
                }
                Some(CollectionStyle::Flow) => {
                    edit.delete_flow_sequence_item_source(sequence, index)
                }
                None => return true,
            }
        }
        _ => return false,
    };

    if let Err(error) = result {
        assert_error_invariants_allowing_unspanned(edit_input.source, &error);
        return true;
    }

    match edit.finish() {
        Ok(output) => {
            let edited = yaml::parse_lossless(&output).expect("structural edit output reparses");
            assert_lossless_stream_invariants(output.as_bytes(), &edited);
        }
        Err(error) => {
            assert!(!error.to_string().is_empty());
        }
    }
    true
}

fn split_lossless_edit_input(input: &[u8]) -> Option<LosslessEditInput<'_>> {
    let line_end = input.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&input[..line_end]).ok()?;
    let body = &input[line_end + 1..];
    let split = find_subslice(body, LOSSLESS_EDIT_REPLACEMENT_MARKER)?;
    let source = &body[..split];
    let replacement =
        std::str::from_utf8(&body[split + LOSSLESS_EDIT_REPLACEMENT_MARKER.len()..]).ok()?;

    Some(LosslessEditInput {
        mode: if header.contains("mode=scalar") {
            LosslessEditMode::Scalar
        } else if header.contains("mode=map-replace") {
            LosslessEditMode::MappingValue
        } else if header.contains("mode=map-insert") {
            LosslessEditMode::MappingInsert
        } else if header.contains("mode=map-delete") {
            LosslessEditMode::MappingDelete
        } else if header.contains("mode=seq-replace") {
            LosslessEditMode::SequenceItem
        } else if header.contains("mode=seq-insert") {
            LosslessEditMode::SequenceInsert
        } else if header.contains("mode=seq-delete") {
            LosslessEditMode::SequenceDelete
        } else if header.contains("mode=source") {
            LosslessEditMode::Source
        } else if header.contains("mode=insert") {
            LosslessEditMode::Insert
        } else if header.contains("mode=delete") {
            LosslessEditMode::Delete
        } else {
            LosslessEditMode::Node
        },
        selector: selector_from_lossless_edit_header(header),
        source,
        replacement,
    })
}

fn selector_from_lossless_edit_header(header: &str) -> usize {
    for field in header.split_whitespace() {
        if let Some(value) = field.strip_prefix("index=")
            && let Ok(index) = value.parse()
        {
            return index;
        }
    }

    header.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(33).wrapping_add(byte as usize)
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn select_lossless_edit_target(
    stream: &LosslessStream,
    input: LosslessEditInput<'_>,
) -> Option<LosslessEditTarget> {
    match input.mode {
        LosslessEditMode::Node | LosslessEditMode::Source | LosslessEditMode::Delete => {
            if stream.nodes().is_empty() {
                return None;
            }
            let node = stream.nodes().get(input.selector % stream.nodes().len())?;
            Some(LosslessEditTarget {
                node: Some(node.id()),
                span: node.span(),
            })
        }
        LosslessEditMode::Scalar => {
            let scalars = stream
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind(), LosslessNodeKind::Scalar { .. }))
                .collect::<Vec<_>>();
            if scalars.is_empty() {
                return None;
            }
            let node = scalars.get(input.selector % scalars.len())?;
            Some(LosslessEditTarget {
                node: Some(node.id()),
                span: node.span(),
            })
        }
        LosslessEditMode::Insert => {
            let offset = input.selector % (stream.as_source().len() + 1);
            let span = stream.source_span(offset, offset).ok()?;
            Some(LosslessEditTarget { node: None, span })
        }
        LosslessEditMode::MappingValue
        | LosslessEditMode::MappingInsert
        | LosslessEditMode::MappingDelete
        | LosslessEditMode::SequenceItem
        | LosslessEditMode::SequenceInsert
        | LosslessEditMode::SequenceDelete => None,
    }
}

fn select_scalar_keyed_mapping(
    stream: &LosslessStream,
    selector: usize,
    block_only: bool,
) -> Option<(NodeId, String)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Mapping { style, entries }
                if !block_only || *style == CollectionStyle::Block =>
            {
                Some((node.id(), entries))
            }
            _ => None,
        })
        .flat_map(|(mapping, entries)| {
            entries.iter().filter_map(move |(key, _)| {
                stream.node(*key).and_then(|node| match node.kind() {
                    LosslessNodeKind::Scalar { value, .. } => Some((mapping, value.clone())),
                    _ => None,
                })
            })
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    candidates.get(selector % candidates.len()).cloned()
}

fn select_mapping_insertion(stream: &LosslessStream, selector: usize) -> Option<NodeId> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Mapping { .. } => Some(node.id()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    candidates.get(selector % candidates.len()).copied()
}

fn select_sequence_item(
    stream: &LosslessStream,
    selector: usize,
    block_only: bool,
) -> Option<(NodeId, usize)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence { style, children }
                if (!block_only || *style == CollectionStyle::Block) && !children.is_empty() =>
            {
                Some((node.id(), children.len()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let (sequence, len) = candidates.get(selector % candidates.len()).copied()?;
    Some((sequence, selector % len))
}

fn select_sequence_insertion(stream: &LosslessStream, selector: usize) -> Option<(NodeId, usize)> {
    let candidates = stream
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            LosslessNodeKind::Sequence { children, .. } => Some((node.id(), children.len())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let (sequence, len) = candidates.get(selector % candidates.len()).copied()?;
    Some((sequence, selector % (len + 1)))
}

fn mapping_style(stream: &LosslessStream, mapping: NodeId) -> Option<CollectionStyle> {
    match stream.node(mapping)?.kind() {
        LosslessNodeKind::Mapping { style, .. } => Some(*style),
        _ => None,
    }
}

fn sequence_style(stream: &LosslessStream, sequence: NodeId) -> Option<CollectionStyle> {
    match stream.node(sequence)?.kind() {
        LosslessNodeKind::Sequence { style, .. } => Some(*style),
        _ => None,
    }
}

fn build_lossless_edited_source(source: &str, span: Span, replacement: &str) -> Option<String> {
    let prefix = source.get(..span.start)?;
    let suffix = source.get(span.end..)?;
    Some([prefix, replacement, suffix].concat())
}

fn assert_lossless_stream_invariants(input: &[u8], stream: &LosslessStream) {
    assert_eq!(stream.as_source().as_bytes(), input);
    assert_eq!(stream.to_string().as_bytes(), input);
    assert_lossless_yaml11_schema_probe(input, stream);

    for document in stream.documents() {
        assert_span_invariants(input, document.start_span());
        assert_span_invariants(input, document.end_span());
        if let Some(root) = document.root() {
            assert_lossless_node_id(stream, root);
        }
    }

    for node in stream.nodes() {
        assert_eq!(
            stream.node(node.id()).map(|node| node.id()),
            Some(node.id())
        );
        assert_span_invariants(input, node.span());
        assert!(stream.source_fragment(node.span()).is_some());
        if let Some(anchor) = node.anchor() {
            assert_lossless_anchor_id(stream, anchor);
            assert_eq!(
                stream.anchor(anchor).map(|anchor| anchor.node()),
                Some(node.id())
            );
        }
        if let Some(tag) = node.tag() {
            assert_span_invariants(input, tag.span);
        }
        match node.kind() {
            LosslessNodeKind::Scalar { .. } => {}
            LosslessNodeKind::Sequence { children, .. } => {
                for child in children {
                    assert_lossless_node_id(stream, *child);
                }
            }
            LosslessNodeKind::Mapping { entries, .. } => {
                for (key, value) in entries {
                    assert_lossless_node_id(stream, *key);
                    assert_lossless_node_id(stream, *value);
                }
            }
            LosslessNodeKind::Alias {
                name,
                alias,
                target,
            } => {
                let alias_ref = stream.alias(*alias).expect("alias id resolves");
                let target_ref = stream.anchor(*target).expect("alias target resolves");
                assert_eq!(alias_ref.name(), name);
                assert_eq!(target_ref.name(), name);
                assert_eq!(alias_ref.node(), node.id());
            }
        }
    }

    for anchor in stream.anchors() {
        assert_eq!(
            stream.anchor(anchor.id()).map(|anchor| anchor.id()),
            Some(anchor.id())
        );
        assert!(!anchor.name().is_empty());
        assert_span_invariants(input, anchor.span());
        assert_lossless_node_id(stream, anchor.node());
    }

    for alias in stream.aliases() {
        assert_eq!(
            stream.alias(alias.id()).map(|alias| alias.id()),
            Some(alias.id())
        );
        assert!(!alias.name().is_empty());
        assert_span_invariants(input, alias.span());
        assert_lossless_node_id(stream, alias.node());
        assert_lossless_anchor_id(stream, alias.target());
    }

    for trivia in stream.trivia() {
        assert_span_invariants(input, trivia.span());
        match trivia.kind() {
            LosslessTriviaKind::Comment => assert!(trivia.text().starts_with('#')),
            LosslessTriviaKind::BlankLine => assert!(trivia.text().trim().is_empty()),
        }
    }
}

fn assert_lossless_yaml11_schema_probe(input: &[u8], stream: &LosslessStream) {
    if !stream.as_source().contains("%YAML 1.1") {
        return;
    }
    match LoadOptions::yaml_version_directive().parse_documents(stream.as_source()) {
        Ok(_) => {}
        Err(error) => assert_error_invariants(input, &error),
    }
}

fn assert_lossless_node_id(stream: &LosslessStream, id: NodeId) {
    assert!(id.index() < stream.nodes().len());
}

fn assert_lossless_anchor_id(stream: &LosslessStream, id: AnchorId) {
    assert!(id.index() < stream.anchors().len());
}

fn assert_apply_merge_invariants(input: &[u8]) {
    let Ok(mut value) = yaml::from_slice::<Value>(input) else {
        return;
    };

    match value.apply_merge() {
        Ok(()) => {
            value
                .apply_merge()
                .expect("repeated apply_merge should keep succeeding");
        }
        Err(error) => {
            assert!(!error.to_string().is_empty());
            assert_eq!(error.location(), None);
        }
    }
}

fn assert_apply_merge_semantics(input: &[u8]) {
    let Ok(input) = std::str::from_utf8(input) else {
        return;
    };
    let Ok(reference_unmerged) = serde_yaml::from_str::<serde_yaml::Value>(input) else {
        return;
    };
    let Ok(mut value) = yaml::to_value(reference_unmerged.clone()) else {
        return;
    };
    let mut reference = reference_unmerged;

    match (value.apply_merge(), reference.apply_merge()) {
        (Ok(()), Ok(())) => {
            let reference =
                yaml::to_value(reference).expect("serde_yaml value converts to yaml::Value");
            assert!(value.equivalent(&reference));

            let mut repeated_value = value.clone();
            repeated_value
                .apply_merge()
                .expect("repeated yaml apply_merge should keep succeeding");
            let mut repeated_reference =
                serde_yaml::from_str::<serde_yaml::Value>(input).expect("reference reparses");
            repeated_reference
                .apply_merge()
                .expect("first serde_yaml merge should keep succeeding");
            repeated_reference
                .apply_merge()
                .expect("repeated serde_yaml apply_merge should keep succeeding");
            let repeated_reference = yaml::to_value(repeated_reference)
                .expect("repeated serde_yaml value converts to yaml::Value");
            assert!(repeated_value.equivalent(&repeated_reference));
        }
        (Err(error), Err(reference_error)) => {
            assert!(!error.to_string().is_empty());
            assert_eq!(error.location(), None);
            assert!(reference_error.location().is_none());
        }
        (Ok(()), Err(reference_error)) => {
            panic!("yaml applied merge but serde_yaml rejected it: {reference_error}");
        }
        (Err(error), Ok(())) => {
            panic!("yaml rejected merge but serde_yaml applied it: {error}");
        }
    }
}

fn assert_single_document_entrypoint(input: &[u8]) {
    match yaml::from_slice::<Value>(input) {
        Ok(value) => {
            let node = yaml::parse_bytes(input).expect("from_slice success must parse");
            assert!(Value::from(&node).equivalent(&value));
        }
        Err(error) => assert_error_invariants(input, &error),
    }
}

fn assert_document_stream_entrypoints(input: &[u8]) {
    let from_documents = yaml::from_documents_slice::<Value>(input);
    match &from_documents {
        Ok(values) => {
            let input_text = std::str::from_utf8(input).expect("document success must be UTF-8");
            let nodes = yaml::parse_documents(input_text).expect("document success must parse");
            assert_eq!(values.len(), nodes.len());
            for (value, node) in values.iter().zip(nodes.iter()) {
                assert!(Value::from(node).equivalent(value));
            }
        }
        Err(error) => assert_error_invariants(input, error),
    }

    let stream_results = yaml::Deserializer::from_slice(input)
        .map(Value::deserialize)
        .collect::<Vec<_>>();
    match from_documents {
        Ok(expected) => {
            assert_stream_results_match_document_values(stream_results, expected, "stream document")
        }
        Err(_) => {
            assert!(
                stream_results.iter().any(Result::is_err),
                "stream deserializer should surface parse errors"
            );
            for error in stream_results
                .iter()
                .filter_map(|result| result.as_ref().err())
            {
                assert_error_invariants(input, error);
            }
        }
    }
}

fn assert_reader_backed_entrypoints(input: &[u8]) {
    match (
        yaml::from_slice::<Value>(input),
        yaml::from_reader::<_, Value>(Cursor::new(input)),
    ) {
        (Ok(from_slice), Ok(from_reader)) => assert!(from_slice.equivalent(&from_reader)),
        (Err(slice_error), Err(reader_error)) => {
            assert_error_invariants(input, &slice_error);
            assert_error_invariants(input, &reader_error);
        }
        (from_slice, from_reader) => panic!(
            "from_reader drifted from from_slice: from_slice={from_slice:?}, from_reader={from_reader:?}"
        ),
    }

    let from_documents_slice = yaml::from_documents_slice::<Value>(input);
    let from_documents_reader = yaml::from_documents_reader::<Value, _>(Cursor::new(input));
    match (from_documents_slice, from_documents_reader) {
        (Ok(from_slice), Ok(from_reader)) => {
            assert_eq!(from_slice.len(), from_reader.len());
            for (from_slice, from_reader) in from_slice.iter().zip(from_reader.iter()) {
                assert!(from_slice.equivalent(from_reader));
            }
        }
        (Err(slice_error), Err(reader_error)) => {
            assert_error_invariants(input, &slice_error);
            assert_error_invariants(input, &reader_error);
        }
        (from_slice, from_reader) => panic!(
            "from_documents_reader drifted from from_documents_slice: from_slice={from_slice:?}, from_reader={from_reader:?}"
        ),
    }

    let reader_stream_results = yaml::Deserializer::from_reader(Cursor::new(input))
        .map(Value::deserialize)
        .collect::<Vec<_>>();
    match yaml::from_documents_reader::<Value, _>(Cursor::new(input)) {
        Ok(expected) => assert_stream_results_match_document_values(
            reader_stream_results,
            expected,
            "reader stream document",
        ),
        Err(_) => {
            assert!(
                reader_stream_results.iter().any(Result::is_err),
                "reader stream deserializer should surface parse errors"
            );
            for error in reader_stream_results
                .iter()
                .filter_map(|result| result.as_ref().err())
            {
                assert_error_invariants(input, error);
            }
        }
    }
}

fn assert_stream_results_match_document_values(
    stream_results: Vec<Result<Value, Error>>,
    expected: Vec<Value>,
    context: &str,
) {
    if expected.is_empty() {
        assert_eq!(stream_results.len(), 1);
        let actual = stream_results
            .into_iter()
            .next()
            .expect("empty stream should yield one document")
            .expect("empty stream document should deserialize");
        assert!(
            actual.is_null(),
            "{context} should be an empty null document"
        );
        return;
    }

    assert_eq!(stream_results.len(), expected.len());
    for (actual, expected) in stream_results.into_iter().zip(expected) {
        let actual = actual.expect("stream document should deserialize");
        assert!(actual.equivalent(&expected));
    }
}

fn assert_config_string_map_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<BTreeMap<String, String>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, Option<String>>>(input);
}

fn assert_numeric_map_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<BTreeMap<String, i128>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u128>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, i64>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u64>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, i8>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u8>>(input);
}

fn assert_typed_reader_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<OwnedReaderConfig>(input);
    assert_owned_reader_entrypoint::<TaggedAliasValues>(input);
    assert_owned_reader_entrypoint::<RootStringConfig>(input);
}

fn assert_yaml11_collection_tag_entrypoints(input: &[u8]) {
    for options in [
        LoadOptions::new(),
        LoadOptions::yaml_1_1(),
        LoadOptions::yaml_version_directive(),
    ] {
        assert_yaml11_collection_tag_options(input, options);
    }
}

fn assert_yaml11_collection_tag_options(input: &[u8], options: LoadOptions) {
    assert_load_options_reader_pair::<BTreeSet<String>>(input, options);
    assert_load_options_reader_pair::<Vec<(String, i64)>>(input, options);
    assert_load_options_reader_pair::<ValueStructuralTags>(input, options);
}

fn assert_load_options_reader_pair<T>(input: &[u8], options: LoadOptions)
where
    T: DeserializeOwned + std::fmt::Debug + PartialEq,
{
    assert_entrypoint_pair(
        input,
        "LoadOptions from_slice/from_reader",
        options.from_slice::<T>(input),
        options.from_reader::<_, T>(Cursor::new(input)),
    );
}

fn assert_owned_reader_entrypoint<T>(input: &[u8])
where
    T: DeserializeOwned + std::fmt::Debug + PartialEq,
{
    assert_entrypoint_pair(
        input,
        "from_slice/from_reader",
        yaml::from_slice::<T>(input),
        yaml::from_reader::<_, T>(Cursor::new(input)),
    );

    assert_entrypoint_pair(
        input,
        "direct slice/reader deserializer",
        T::deserialize(yaml::Deserializer::from_slice(input)),
        T::deserialize(yaml::Deserializer::from_reader(Cursor::new(input))),
    );

    assert_entrypoint_pair(
        input,
        "from_documents_slice/from_documents_reader",
        yaml::from_documents_slice::<T>(input),
        yaml::from_documents_reader::<T, _>(Cursor::new(input)),
    );

    assert_stream_results_match(
        input,
        yaml::Deserializer::from_slice(input)
            .map(T::deserialize)
            .collect::<Vec<_>>(),
        yaml::Deserializer::from_reader(Cursor::new(input))
            .map(T::deserialize)
            .collect::<Vec<_>>(),
    );
}

fn assert_entrypoint_pair<T>(
    input: &[u8],
    label: &str,
    left: yaml::Result<T>,
    right: yaml::Result<T>,
) where
    T: std::fmt::Debug + PartialEq,
{
    match (left, right) {
        (Ok(left), Ok(right)) => assert_eq!(left, right, "{label} drifted"),
        (Err(left), Err(right)) => {
            assert_error_invariants_allowing_unspanned(input, &left);
            assert_error_invariants_allowing_unspanned(input, &right);
        }
        (left, right) => panic!("{label} drifted: left={left:?}, right={right:?}"),
    }
}

fn assert_stream_results_match<T>(
    input: &[u8],
    left: Vec<yaml::Result<T>>,
    right: Vec<yaml::Result<T>>,
) where
    T: std::fmt::Debug + PartialEq,
{
    assert_eq!(left.len(), right.len(), "typed stream length drifted");
    for (left, right) in left.into_iter().zip(right) {
        assert_entrypoint_pair(input, "typed stream slice/reader", left, right);
    }
}

fn assert_borrowed_entrypoints(input: &[u8]) {
    match yaml::from_slice::<BorrowedConfig<'_>>(input) {
        Ok(config) => {
            assert_borrowed_from_input(input, config.name);
            assert_borrowed_from_input(input, config.path);
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match yaml::from_slice::<BorrowedVars<'_>>(input) {
        Ok(config) => {
            for (key, value) in config.vars {
                assert_borrowed_from_input(input, key);
                assert_borrowed_from_input(input, value);
            }
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match BorrowedConfig::deserialize(yaml::Deserializer::from_slice(input)) {
        Ok(config) => {
            assert_borrowed_from_input(input, config.name);
            assert_borrowed_from_input(input, config.path);
        }
        Err(error) => assert!(!error.to_string().is_empty()),
    }

    for result in yaml::Deserializer::from_slice(input).map(BorrowedConfig::deserialize) {
        match result {
            Ok(config) => {
                assert_borrowed_from_input(input, config.name);
                assert_borrowed_from_input(input, config.path);
            }
            Err(error) => assert_error_invariants(input, &error),
        }
    }
}

fn assert_byte_entrypoints(input: &[u8]) {
    if let Err(error) = yaml::from_slice::<FuzzBytes>(input) {
        assert_error_invariants(input, &error);
    }

    if let Err(error) = FuzzBytes::deserialize(yaml::Deserializer::from_slice(input)) {
        assert_error_invariants_allowing_unspanned(input, &error);
    }

    for result in yaml::Deserializer::from_slice(input).map(FuzzBytes::deserialize) {
        if let Err(error) = result {
            assert_error_invariants(input, &error);
        }
    }
}

fn assert_borrowed_from_input(input: &[u8], value: &str) {
    let input_start = input.as_ptr() as usize;
    let input_end = input_start + input.len();
    let value_start = value.as_ptr() as usize;
    let value_end = value_start + value.len();
    assert!(
        value_start >= input_start && value_end <= input_end,
        "borrowed value should point into input"
    );
    let offset = value_start - input_start;
    assert_eq!(&input[offset..offset + value.len()], value.as_bytes());
}

#[test]
fn event_stream_fuzz_regression_closes_blank_separated_implicit_documents() {
    let input = "a-r--\n\noa: b\0\0\0\0\0\0\0\n";
    let events = yaml::parse_events(input).expect("events");
    assert_event_invariants(input.as_bytes(), &events);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { .. }))
            .count(),
        2
    );
}

#[test]
fn event_stream_root_scalar_regressions_stay_structurally_valid() {
    for (name, input) in [
        (
            "M7A3",
            include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml"),
        ),
        ("root-literal-indented", "|\n  line\n"),
        (
            "root-literal-indent-zero-percent",
            "|\n%!PS-Adobe-2.0 # Not the first line\n",
        ),
        ("blank-separated-plain", "@\n\ng\n"),
    ] {
        let result = yaml::parse_events(input);
        assert_event_result_invariants(input.as_bytes(), &result);
        result.unwrap_or_else(|error| panic!("event stream regression {name} failed: {error}"));
    }
}

fn assert_parse_result_invariants(input: &[u8], result: &yaml::Result<Node>) {
    match result {
        Ok(node) => {
            assert_node_invariants(input, node);
            assert_success_invariants(node);
        }
        Err(error) => assert_error_invariants(input, error),
    }
}

fn assert_event_result_invariants(input: &[u8], result: &yaml::Result<Vec<Event>>) {
    match result {
        Ok(events) => assert_event_invariants(input, events),
        Err(error) => assert_error_invariants(input, error),
    }
}

fn assert_event_invariants(input: &[u8], events: &[Event]) {
    assert!(matches!(events.first(), Some(Event::StreamStart)));
    assert!(matches!(events.last(), Some(Event::StreamEnd)));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::StreamStart))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::StreamEnd))
            .count(),
        1
    );

    let mut docs = 0i32;
    let mut collections = Vec::new();
    for event in events {
        match event {
            Event::StreamStart | Event::StreamEnd => {}
            Event::DocumentStart {
                explicit,
                directives,
                span,
            } => {
                assert_eq!(docs, 0, "nested document start at {event:?}");
                assert!(
                    collections.is_empty(),
                    "document started while collections were open: {collections:?}"
                );
                assert_span_invariants(input, *span);
                if *explicit {
                    assert_eq!(source_slice(input, *span), b"---");
                }
                assert_document_directives_invariants(input, directives);
                docs += 1;
            }
            Event::DocumentEnd { explicit, span } => {
                assert!(
                    collections.is_empty(),
                    "document ended while collections were open: {collections:?}"
                );
                assert_span_invariants(input, *span);
                if *explicit {
                    assert_eq!(source_slice(input, *span), b"...");
                }
                docs -= 1;
            }
            Event::SequenceStart { meta, span, .. } => {
                assert!(docs > 0, "sequence start outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("sequence");
            }
            Event::SequenceEnd { span } => {
                assert!(docs > 0, "sequence end outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("sequence"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::MappingStart { meta, span, .. } => {
                assert!(docs > 0, "mapping start outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("mapping");
            }
            Event::MappingEnd { span } => {
                assert!(docs > 0, "mapping end outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("mapping"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::Alias { anchor } => {
                assert!(docs > 0, "alias outside document: {event:?}");
                assert_span_invariants(input, anchor.span);
                assert!(!anchor.name.is_empty());
                assert_eq!(
                    source_slice(input, anchor.span),
                    format!("*{}", anchor.name).as_bytes()
                );
            }
            Event::Scalar { meta, span, .. } => {
                assert!(docs > 0, "scalar outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
            }
        }
        assert!(docs >= 0, "document stack went negative at {event:?}");
    }
    assert_eq!(docs, 0);
    assert!(
        collections.is_empty(),
        "unclosed collections: {collections:?}"
    );
}

fn assert_event_meta_invariants(input: &[u8], meta: &EventMeta) {
    if let Some(anchor) = &meta.anchor {
        assert_span_invariants(input, anchor.span);
        assert!(!anchor.name.is_empty());
        assert_eq!(
            source_slice(input, anchor.span),
            format!("&{}", anchor.name).as_bytes()
        );
    }
    if let Some(tag) = &meta.tag {
        assert_span_invariants(input, tag.span);
        assert!(
            source_slice(input, tag.span).starts_with(b"!"),
            "tag span should point at tag token: {:?}",
            tag.span
        );
    }
}

fn assert_document_directives_invariants(input: &[u8], directives: &EventDocumentDirectives) {
    if let Some(version) = &directives.yaml_version {
        assert_span_invariants(input, version.span);
        assert!(version.major > 0);
        assert_eq!(
            source_slice(input, version.span),
            format!("{}.{}", version.major, version.minor).as_bytes()
        );
    }
    for directive in &directives.tag_directives {
        assert!(!directive.handle.is_empty());
        assert!(!directive.prefix.is_empty());
        assert_span_invariants(input, directive.span);
        assert_span_invariants(input, directive.handle_span);
        assert_span_invariants(input, directive.prefix_span);
        assert!(
            source_slice(input, directive.span).starts_with(b"%TAG"),
            "TAG directive span should point at directive token: {:?}",
            directive.span
        );
        assert_eq!(
            source_slice(input, directive.handle_span),
            directive.handle.as_bytes()
        );
        assert_eq!(
            source_slice(input, directive.prefix_span),
            directive.prefix.as_bytes()
        );
    }
}

fn assert_success_invariants(node: &Node) {
    assert_emit_roundtrip_invariants(node);

    let direct = Value::from(node);
    let from_node: Value = yaml::from_node(node).expect("from_node value");
    let from_value: Value = yaml::from_value(direct.clone()).expect("from_value value");
    assert!(direct.equivalent(&from_node));
    assert!(direct.equivalent(&from_value));
}

fn assert_emit_roundtrip_invariants(node: &Node) {
    let emitted = yaml::to_string(node).expect("emit parsed tree");
    let mut written = Vec::new();
    yaml::to_writer(&mut written, node).expect("write parsed tree");
    assert_eq!(written, emitted.as_bytes());

    let reparsed = yaml::parse_str(&emitted).expect("parse emitted tree");
    assert!(
        reparsed.equivalent(node),
        "emitted YAML did not parse equivalently:\n{emitted}"
    );

    let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed tree");
    assert_eq!(emitted_again, emitted);

    let value = Value::from(node);
    let value_emitted = yaml::to_string(&value).expect("emit parsed value");
    let mut value_written = Vec::new();
    yaml::to_writer(&mut value_written, &value).expect("write parsed value");
    assert_eq!(value_written, value_emitted.as_bytes());

    let reparsed_value: Value = yaml::from_str(&value_emitted).expect("parse emitted value");
    assert!(reparsed_value.equivalent(&value));

    let mut stream = yaml::Serializer::new(Vec::new());
    value.serialize(&mut stream).expect("stream first value");
    reparsed_value
        .serialize(&mut stream)
        .expect("stream second value");
    let stream_output = String::from_utf8(stream.into_inner().expect("stream into inner"))
        .expect("stream output is utf8");
    let stream_values =
        yaml::from_documents_str::<Value>(&stream_output).expect("parse streamed values");
    assert_eq!(stream_values.len(), 2);
    assert!(stream_values[0].equivalent(&value));
    assert!(stream_values[1].equivalent(&reparsed_value));
}

fn assert_node_invariants(input: &[u8], node: &Node) {
    assert_span_invariants(input, node.span);
    match &node.value {
        NodeValue::Sequence(items) => {
            for item in items {
                assert_node_invariants(input, item);
            }
        }
        NodeValue::Mapping(entries) => {
            for (key, value) in entries {
                assert_node_invariants(input, key);
                assert_node_invariants(input, value);
            }
        }
        NodeValue::Tagged(tagged) => {
            assert_span_invariants(input, tagged.tag_span);
            assert_node_invariants(input, &tagged.value);
        }
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) | NodeValue::String(_) => {}
    }
}

fn assert_error_invariants(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    assert_span_invariants(input, diagnostic.span);
    for related in &diagnostic.related {
        assert!(!related.message.is_empty());
        assert_span_invariants(input, related.span);
    }
}

fn assert_error_invariants_allowing_unspanned(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    if error.location().is_some() {
        assert_span_invariants(input, diagnostic.span);
    } else {
        assert_eq!(diagnostic.span, Span::default());
    }
    for related in &diagnostic.related {
        assert!(!related.message.is_empty());
        assert_span_invariants(input, related.span);
    }
}

fn assert_span_invariants(input: &[u8], span: Span) {
    assert!(
        span.start <= span.end,
        "span starts after it ends: {span:?}"
    );
    assert!(
        span.end <= input.len(),
        "span exceeds input length {}: {span:?}",
        input.len()
    );
    assert!(span.line >= 1, "span line must be one-based: {span:?}");
    assert!(span.column >= 1, "span column must be one-based: {span:?}");
    let (line, column) = byte_location(input, span.start);
    assert_eq!(
        (span.line, span.column),
        (line, column),
        "span location does not match byte offset for {span:?}"
    );
}

fn source_slice(input: &[u8], span: Span) -> &[u8] {
    &input[span.start..span.end]
}

fn byte_location(input: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for byte in &input[..offset] {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn has_non_roundtrippable_explicit_core_scalar_tag(node: &Node) -> bool {
    match &node.value {
        NodeValue::Tagged(tagged) => {
            if is_explicit_core_scalar_tag(&tagged.tag)
                && !matches!(tagged.value.value, NodeValue::String(_))
            {
                return true;
            }
            has_non_roundtrippable_explicit_core_scalar_tag(&tagged.value)
        }
        NodeValue::Sequence(items) => items
            .iter()
            .any(has_non_roundtrippable_explicit_core_scalar_tag),
        NodeValue::Mapping(entries) => entries.iter().any(|(key, value)| {
            has_non_roundtrippable_explicit_core_scalar_tag(key)
                || has_non_roundtrippable_explicit_core_scalar_tag(value)
        }),
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) | NodeValue::String(_) => false,
    }
}

fn is_explicit_core_scalar_tag(tag: &Tag) -> bool {
    let short = tag.handle == "!!"
        && matches!(
            tag.suffix.as_str(),
            "binary" | "bool" | "float" | "int" | "null" | "str" | "timestamp"
        );
    let canonical = tag.handle == "!"
        && tag
            .suffix
            .strip_prefix("tag:yaml.org,2002:")
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "binary" | "bool" | "float" | "int" | "null" | "str" | "timestamp"
                )
            });
    short || canonical
}

fn arb_node(max_depth: u32) -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
        Just(Node::new(NodeValue::Null, Span::default())),
        any::<bool>().prop_map(|value| Node::new(NodeValue::Bool(value), Span::default())),
        (-1_000_000i64..=1_000_000).prop_map(|value| Node::new(
            NodeValue::Number(Number::Integer(i128::from(value))),
            Span::default()
        )),
        (i64::MAX as u64 + 1..=i64::MAX as u64 + 1_000_000).prop_map(|value| Node::new(
            NodeValue::Number(Number::Unsigned(u128::from(value))),
            Span::default()
        )),
        finite_float()
            .prop_map(|value| Node::new(NodeValue::Number(Number::Float(value)), Span::default())),
        prop_oneof![Just(f64::NAN), Just(f64::INFINITY), Just(f64::NEG_INFINITY),]
            .prop_map(|value| Node::new(NodeValue::Number(Number::Float(value)), Span::default())),
        safe_string().prop_map(|value| Node::new(NodeValue::String(value), Span::default())),
    ];

    leaf.prop_recursive(max_depth, 64, 8, |inner| {
        prop_oneof![
            (arb_tag(), inner.clone()).prop_map(|(tag, value)| {
                Node::new(
                    NodeValue::Tagged(Box::new(TaggedNode {
                        tag,
                        tag_span: Span::default(),
                        value,
                    })),
                    Span::default(),
                )
            }),
            prop::collection::vec(inner.clone(), 0..8)
                .prop_map(|items| Node::new(NodeValue::Sequence(items), Span::default())),
            prop::collection::btree_map(safe_key(), inner.clone(), 0..8).prop_map(|entries| {
                Node::new(
                    NodeValue::Mapping(
                        entries
                            .into_iter()
                            .map(|(key, value)| {
                                (Node::new(NodeValue::String(key), Span::default()), value)
                            })
                            .collect(),
                    ),
                    Span::default(),
                )
            }),
            (
                arb_complex_key(),
                inner.clone(),
                prop::collection::btree_map(safe_key(), inner, 0..4)
            )
                .prop_map(|(key, value, entries)| {
                    let mut entries = entries
                        .into_iter()
                        .map(|(key, value)| {
                            (Node::new(NodeValue::String(key), Span::default()), value)
                        })
                        .collect::<Vec<_>>();
                    entries.push((key, value));
                    Node::new(NodeValue::Mapping(entries), Span::default())
                }),
        ]
    })
}

fn arb_tag() -> impl Strategy<Value = Tag> {
    prop_oneof![
        Just(Tag::new("Thing")),
        Just(Tag::new("!<tag:example.com,2026:Thing>")),
        Just(Tag::new("!<:example.com,2026:Thing>")),
        Just(Tag::new("!<<abc>")),
        Just(Tag::new("!<abc:>")),
        Just(Tag::new("!<a!b>")),
        Just(Tag::new("!<comma,tag>")),
        Just(Tag::new("!<flow[bracket]>")),
        Just(Tag::new("!<space tag>")),
        Just(Tag::new("!<brace{tag}>")),
    ]
}

fn arb_complex_key() -> impl Strategy<Value = Node> {
    prop_oneof![
        prop::collection::vec(safe_string(), 0..4).prop_map(|items| {
            Node::new(
                NodeValue::Sequence(
                    items
                        .into_iter()
                        .map(|value| Node::new(NodeValue::String(value), Span::default()))
                        .collect(),
                ),
                Span::default(),
            )
        }),
        (arb_tag(), safe_key()).prop_map(|(tag, value)| {
            Node::new(
                NodeValue::Tagged(Box::new(TaggedNode {
                    tag,
                    tag_span: Span::default(),
                    value: Node::new(NodeValue::String(value), Span::default()),
                })),
                Span::default(),
            )
        }),
    ]
}

fn finite_float() -> impl Strategy<Value = f64> {
    (-1_000_000f64..1_000_000f64).prop_filter("finite non-integer float", |value| {
        value.is_finite() && value.fract() != 0.0
    })
}

fn safe_key() -> impl Strategy<Value = String> {
    "[A-Za-z_][A-Za-z0-9_.-]{0,16}".prop_map(|value| value)
}

fn safe_string() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[A-Za-z0-9 _./:@-]{0,32}".prop_map(|value| value),
        Just("true".to_string()),
        Just("null".to_string()),
        Just(".nan".to_string()),
        Just(".NaN".to_string()),
        Just(".inf".to_string()),
        Just("+.inf".to_string()),
        Just("+.INF".to_string()),
        Just("001".to_string()),
        Just("a: b".to_string()),
        Just("...".to_string()),
        Just("... marker".to_string()),
        Just("%YAML 1.2".to_string()),
        Just("a:\tb".to_string()),
        Just("a\t#b".to_string()),
        Just("line\n".to_string()),
        Just("first\nsecond\n".to_string()),
        Just(" leading\nregular\n".to_string()),
        Just("quote \" slash \\\\".to_string()),
        Just("tab\t nul\0".to_string()),
        Just("a]b".to_string()),
        Just("a[b".to_string()),
        Just("a}b".to_string()),
        Just("a{b".to_string()),
    ]
}
