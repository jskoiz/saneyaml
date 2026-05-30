#![allow(non_snake_case)]

use serde::Deserialize;
use std::{collections::HashSet, fs, path::PathBuf};
use yaml::{
    Event, NodeValue as Value, Number, ScalarStyle, parse_documents, parse_events, parse_str,
};

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
    name: String,
    expected: ExpectedOutcome,
    source: String,
    policy: String,
    features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExpectedOutcome {
    Accept,
    SyntaxError,
    TreeError,
}

impl SuiteCase {
    fn fixture_dir(&self) -> String {
        self.id.replace('/', "-")
    }

    fn fixture_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/yaml-test-suite/data")
            .join(self.fixture_dir())
            .join("in.yaml")
    }

    fn is_error_case(&self) -> bool {
        self.expected == ExpectedOutcome::SyntaxError
    }

    fn is_tree_only_rejection(&self) -> bool {
        self.expected == ExpectedOutcome::TreeError
    }
}

fn selected_suite_manifest() -> SuiteManifest {
    toml::from_str(include_str!("fixtures/yaml-test-suite/manifest.toml"))
        .expect("selected YAML test-suite manifest is valid TOML")
}

fn selected_suite_input(case: &SuiteCase) -> String {
    fs::read_to_string(case.fixture_path()).unwrap_or_else(|error| {
        panic!(
            "read YAML-suite fixture {} ({}): {error}",
            case.id, case.name
        )
    })
}

fn assert_manifest_error(case: &SuiteCase, entrypoint: &str, error: &yaml::Error) {
    let diagnostic = error.diagnostic();
    assert!(
        !diagnostic.message.trim().is_empty(),
        "{} via {entrypoint} reports an empty diagnostic",
        case.id
    );
    assert!(
        error.location().is_some(),
        "{} via {entrypoint} reports no source location: {error}",
        case.id
    );
}

fn event_source(input: &str, span: yaml::Span) -> &str {
    &input[span.start..span.end]
}

#[test]
fn yts_manifest_selected_cases_have_fixture_inputs_and_unique_ids() {
    let manifest = selected_suite_manifest();
    let mut seen = HashSet::new();
    let mut accepted = 0usize;
    let mut error_cases = 0usize;
    let mut tree_only_rejections = 0usize;

    for case in &manifest.case {
        assert!(
            seen.insert(case.id.as_str()),
            "duplicate YAML-suite manifest id {}",
            case.id
        );
        assert!(
            case.fixture_path().exists(),
            "missing YAML-suite fixture input for {} ({})",
            case.id,
            case.name
        );
        assert_eq!(
            case.source, "selected-yaml-test-suite-fixture",
            "{} ({}) records an unsupported source policy",
            case.id, case.name
        );
        assert!(
            !case.policy.trim().is_empty(),
            "{} ({}) must record an expected-outcome policy",
            case.id,
            case.name
        );
        match case.expected {
            ExpectedOutcome::Accept => {
                assert_eq!(
                    case.policy, "raw-events-tree-serde-accept",
                    "{} ({}) accept case records wrong policy",
                    case.id, case.name
                );
                accepted += 1;
            }
            ExpectedOutcome::SyntaxError => {
                assert_eq!(
                    case.policy, "raw-events-tree-serde-reject",
                    "{} ({}) syntax-error case records wrong policy",
                    case.id, case.name
                );
                assert!(
                    case.features.iter().any(|feature| feature == "error"),
                    "{} ({}) syntax-error case should keep the error feature for searchability",
                    case.id,
                    case.name
                );
                error_cases += 1;
            }
            ExpectedOutcome::TreeError => {
                assert_eq!(
                    case.policy, "raw-events-accept-tree-serde-reject",
                    "{} ({}) tree-error case records wrong policy",
                    case.id, case.name
                );
                assert!(
                    !case.features.iter().any(|feature| feature == "error"),
                    "{} ({}) tree-error case is not a syntax-error fixture",
                    case.id,
                    case.name
                );
                tree_only_rejections += 1;
            }
        }
    }

    assert_eq!(manifest.case.len(), 163);
    assert_eq!(accepted, 108);
    assert_eq!(error_cases, 53);
    assert_eq!(tree_only_rejections, 2);
}

#[test]
fn yts_manifest_acceptance_policy_matches_parser_event_and_serde_entrypoints() {
    let manifest = selected_suite_manifest();
    let mut accepted = 0usize;
    let mut syntax_rejections = 0usize;
    let mut tree_only_rejections = 0usize;

    for case in &manifest.case {
        let input = selected_suite_input(case);

        if case.is_error_case() {
            let error = match parse_documents(&input) {
                Err(error) => error,
                Ok(_) => panic!(
                    "{} ({}) should be rejected by document loading",
                    case.id, case.name
                ),
            };
            assert_manifest_error(case, "parse_documents", &error);

            let error = match parse_events(&input) {
                Err(error) => error,
                Ok(_) => panic!(
                    "{} ({}) should be rejected by raw event parsing",
                    case.id, case.name
                ),
            };
            assert_manifest_error(case, "parse_events", &error);

            let error = match yaml::from_documents_str::<yaml::Value>(&input) {
                Err(error) => error,
                Ok(_) => panic!(
                    "{} ({}) should be rejected by Serde document loading",
                    case.id, case.name
                ),
            };
            assert_manifest_error(case, "from_documents_str", &error);
            syntax_rejections += 1;
            continue;
        }

        if case.is_tree_only_rejection() {
            parse_events(&input).unwrap_or_else(|error| {
                panic!(
                    "{} ({}) should remain accepted by raw event parsing: {error}",
                    case.id, case.name
                )
            });

            let error = match parse_documents(&input) {
                Err(error) => error,
                Ok(_) => panic!(
                    "{} ({}) should be rejected by tree loading",
                    case.id, case.name
                ),
            };
            assert_manifest_error(case, "parse_documents", &error);

            let error = match yaml::from_documents_str::<yaml::Value>(&input) {
                Err(error) => error,
                Ok(_) => panic!(
                    "{} ({}) should be rejected by Serde document loading",
                    case.id, case.name
                ),
            };
            assert_manifest_error(case, "from_documents_str", &error);
            tree_only_rejections += 1;
            continue;
        }

        parse_events(&input).unwrap_or_else(|error| {
            panic!(
                "{} ({}) should be accepted by raw event parsing: {error}",
                case.id, case.name
            )
        });
        parse_documents(&input).unwrap_or_else(|error| {
            panic!(
                "{} ({}) should be accepted by document loading: {error}",
                case.id, case.name
            )
        });
        yaml::from_documents_str::<yaml::Value>(&input).unwrap_or_else(|error| {
            panic!(
                "{} ({}) should be accepted by Serde document loading: {error}",
                case.id, case.name
            )
        });
        accepted += 1;
    }

    assert_eq!(accepted, 108);
    assert_eq!(syntax_rejections, 53);
    assert_eq!(tree_only_rejections, 2);
}

#[test]
fn yts_parse_dhp8__flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/DHP8/in.yaml");
    let doc = parse_str(input).expect("parse selected flow fixture");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0].value, Value::Sequence(_)));
    assert!(matches!(items[1].value, Value::Mapping(_)));
}

#[test]
fn yts_load_7w2p__block_mapping_missing_values() {
    let input = include_str!("fixtures/yaml-test-suite/data/7W2P/in.yaml");
    let doc = parse_str(input).expect("parse selected null fixture");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 3);
    assert!(matches!(entries[0].1.value, Value::Null));
    assert!(matches!(entries[1].1.value, Value::Null));
    let Value::Sequence(items) = &entries[2].1.value else {
        panic!("expected sequence");
    };
    assert!(matches!(items[0].value, Value::Null));
    assert_eq!(items[1].as_str(), Some("present"));
}

#[test]
fn yts_reject_2jqs__duplicate_missing_block_mapping_keys() {
    let input = include_str!("fixtures/yaml-test-suite/data/2JQS/in.yaml");
    let error = parse_str(input).expect_err("duplicate missing block mapping keys rejected");
    let display = error.to_string();
    assert!(display.contains("duplicate mapping key"));
    assert!(display.contains("null"));
    assert!(
        !error.diagnostic().related.is_empty(),
        "duplicate missing key reports the previous key span"
    );

    yaml::from_documents_str::<yaml::Value>(input)
        .expect_err("Serde document loading rejects duplicate missing keys");
}

#[test]
fn yts_events_2jqs__duplicate_missing_block_mapping_keys_remain_raw_events() {
    let input = include_str!("fixtures/yaml-test-suite/data/2JQS/in.yaml");
    let events = parse_events(input).expect("raw events preserve duplicate missing keys");
    let null_scalars = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                    ..
                } if value == "null"
            )
        })
        .count();

    assert_eq!(null_scalars, 2);
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, .. } if value == "a"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, .. } if value == "b"
        )
    }));
}

#[test]
fn yts_parse_5we3__explicit_block_mapping_entries() {
    let input = include_str!("fixtures/yaml-test-suite/data/5WE3/in.yaml");
    let doc = parse_str(input).expect("parse explicit block mapping entries");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("explicit key"));
    assert!(matches!(entries[0].1.value, Value::Null));
    assert_eq!(entries[1].0.as_str(), Some("block key\n"));
    let Value::Sequence(items) = &entries[1].1.value else {
        panic!("expected explicit compact sequence value");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_str(), Some("one"));
    assert_eq!(items[1].as_str(), Some("two"));
}

#[test]
fn yts_parse_reduced_explicit_block_mapping_key_as_mapping() {
    let doc = parse_str("? explicit key\n").expect("parse reduced explicit mapping");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("explicit key"));
    assert!(matches!(entries[0].1.value, Value::Null));
}

#[test]
fn yts_parse_v9d5__compact_block_mappings() {
    let input = include_str!("fixtures/yaml-test-suite/data/V9D5/in.yaml");
    let doc = parse_str(input).expect("parse compact block mappings");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("sun"));
    assert_eq!(first[0].1.as_str(), Some("yellow"));

    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    let Value::Mapping(compact_key) = &second[0].0.value else {
        panic!("expected compact mapping key");
    };
    let Value::Mapping(compact_value) = &second[0].1.value else {
        panic!("expected compact mapping value");
    };
    assert_eq!(compact_key[0].0.as_str(), Some("earth"));
    assert_eq!(compact_key[0].1.as_str(), Some("blue"));
    assert_eq!(compact_value[0].0.as_str(), Some("moon"));
    assert_eq!(compact_value[0].1.as_str(), Some("white"));
}

#[test]
fn yts_parse_reduced_compact_block_mapping_key_value_pair() {
    let doc = parse_str("- ? earth: blue\n  : moon: white\n")
        .expect("parse reduced compact block mapping");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    let Value::Mapping(item) = &items[0].value else {
        panic!("expected item mapping");
    };
    assert!(matches!(item[0].0.value, Value::Mapping(_)));
    assert!(matches!(item[0].1.value, Value::Mapping(_)));
}

#[test]
fn yts_parse_s3pd__implicit_block_mapping_entries() {
    let input = include_str!("fixtures/yaml-test-suite/data/S3PD/in.yaml");
    let doc = parse_str(input).expect("parse implicit block mapping entries");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.as_str(), Some("plain key"));
    assert_eq!(entries[0].1.as_str(), Some("in-line value"));
    assert!(matches!(entries[1].0.value, Value::Null));
    assert!(matches!(entries[1].1.value, Value::Null));
    assert_eq!(entries[2].0.as_str(), Some("quoted key"));
    let Value::Sequence(items) = &entries[2].1.value else {
        panic!("expected indentless sequence value");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_str(), Some("entry"));
}

#[test]
fn yts_parse_cfd4__empty_implicit_key_in_single_pair_flow_sequences() {
    let input = include_str!("fixtures/yaml-test-suite/data/CFD4/in.yaml");
    let doc = parse_str(input).expect("parse empty implicit flow sequence keys");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);

    for (item, expected) in items.iter().zip(["empty key", "another empty key"]) {
        let Value::Sequence(nested) = &item.value else {
            panic!("expected nested flow sequence");
        };
        let Value::Mapping(mapping) = &nested[0].value else {
            panic!("expected single-pair mapping item");
        };
        assert!(matches!(mapping[0].0.value, Value::Null));
        assert_eq!(mapping[0].1.as_str(), Some(expected));
    }
}

#[test]
fn yts_parse_m2n8_00__question_mark_edge_empty_compact_mapping_key() {
    let input = include_str!("fixtures/yaml-test-suite/data/M2N8-00/in.yaml");
    let doc = parse_str(input).expect("parse question mark edge case");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    let Value::Mapping(item) = &items[0].value else {
        panic!("expected sequence item mapping");
    };
    let Value::Mapping(key_mapping) = &item[0].0.value else {
        panic!("expected compact mapping key");
    };
    assert!(matches!(key_mapping[0].0.value, Value::Null));
    assert_eq!(key_mapping[0].1.as_str(), Some("x"));
    assert!(matches!(item[0].1.value, Value::Null));
}

#[test]
fn yts_parse_ukk6_00__colon_only_compact_sequence_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/UKK6-00/in.yaml");
    let doc = parse_str(input).expect("parse colon-only sequence mapping");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    let Value::Mapping(mapping) = &items[0].value else {
        panic!("expected mapping sequence item");
    };
    assert!(matches!(mapping[0].0.value, Value::Null));
    assert!(matches!(mapping[0].1.value, Value::Null));
}

#[test]
fn yts_parse_ukk6_02__bare_explicit_non_specific_tag() {
    let input = include_str!("fixtures/yaml-test-suite/data/UKK6-02/in.yaml");
    let doc = parse_str(input).expect("parse bare non-specific tag");
    assert_eq!(doc.as_str(), Some(""));

    let events = parse_events(input).expect("parse bare tag events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                meta,
                ..
            } if meta.tag.as_ref().is_some_and(|tag| tag.tag == yaml::Tag::new("!"))
        )
    }));
}

#[test]
fn yts_parse_2ebw__allowed_plain_key_characters() {
    let input = include_str!("fixtures/yaml-test-suite/data/2EBW/in.yaml");
    let doc = parse_str(input).expect("parse allowed plain key characters");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 5);
    assert_eq!(
        entries[0].0.as_str(),
        Some("a!\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~")
    );
    assert_eq!(entries[0].1.as_str(), Some("safe"));
    assert_eq!(entries[1].0.as_str(), Some("?foo"));
    assert_eq!(entries[1].1.as_str(), Some("safe question mark"));
    assert_eq!(entries[2].0.as_str(), Some(":foo"));
    assert_eq!(entries[2].1.as_str(), Some("safe colon"));
    assert_eq!(entries[3].0.as_str(), Some("-foo"));
    assert_eq!(entries[3].1.as_str(), Some("safe dash"));
    assert_eq!(entries[4].0.as_str(), Some("this is#not"));
    assert_eq!(entries[4].1.as_str(), Some("a comment"));
}

#[test]
fn yts_parse_fbc9__allowed_characters_in_plain_scalars() {
    let input = include_str!("fixtures/yaml-test-suite/data/FBC9/in.yaml");
    let doc = parse_str(input).expect("parse allowed plain scalar characters");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].0.as_str(), Some("safe"));
    assert_eq!(
        entries[0].1.as_str(),
        Some("a!\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~ !\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~")
    );
    assert_eq!(entries[1].0.as_str(), Some("safe question mark"));
    assert_eq!(entries[1].1.as_str(), Some("?foo"));
    assert_eq!(entries[2].0.as_str(), Some("safe colon"));
    assert_eq!(entries[2].1.as_str(), Some(":foo"));
    assert_eq!(entries[3].0.as_str(), Some("safe dash"));
    assert_eq!(entries[3].1.as_str(), Some("-foo"));
}

#[test]
fn yts_parse_xlq9__plain_scalar_continuation_can_look_like_directive() {
    let input = include_str!("fixtures/yaml-test-suite/data/XLQ9/in.yaml");
    let doc = parse_str(input).expect("parse directive-looking plain scalar continuation");
    assert_eq!(doc.as_str(), Some("scalar %YAML 1.2"));

    let events = parse_events(input).expect("events for directive-looking continuation");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                span,
                ..
            } if value == "scalar %YAML 1.2"
                && (span.line, span.column) == (2, 1)
                && event_source(input, *span) == "scalar\n%YAML 1.2"
        )
    }));
}

#[test]
fn yts_parse_xw4d_rzp5__various_trailing_comments() {
    for (name, input) in [
        (
            "XW4D",
            include_str!("fixtures/yaml-test-suite/data/XW4D/in.yaml"),
        ),
        (
            "RZP5",
            include_str!("fixtures/yaml-test-suite/data/RZP5/in.yaml"),
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        assert_various_trailing_comments_tree(name, &doc);

        parse_events(input).unwrap_or_else(|error| panic!("{name} events parse: {error}"));
    }
}

fn assert_various_trailing_comments_tree(name: &str, doc: &yaml::Node) {
    let Value::Mapping(entries) = &doc.value else {
        panic!("{name}: expected top-level mapping");
    };
    assert_eq!(entries.len(), 6, "{name}");
    assert_eq!(entries[0].0.as_str(), Some("a"), "{name}");
    assert_eq!(entries[0].1.as_str(), Some("double quotes"), "{name}");
    assert_eq!(entries[1].0.as_str(), Some("b"), "{name}");
    assert_eq!(entries[1].1.as_str(), Some("plain value"), "{name}");
    assert_eq!(entries[2].0.as_str(), Some("c"), "{name}");
    assert_eq!(entries[2].1.as_str(), Some("d"), "{name}");

    let Value::Sequence(key_items) = &entries[3].0.value else {
        panic!("{name}: expected explicit sequence key");
    };
    assert_eq!(key_items.len(), 1, "{name}");
    assert_eq!(key_items[0].as_str(), Some("seq1"), "{name}");
    let Value::Sequence(value_items) = &entries[3].1.value else {
        panic!("{name}: expected explicit sequence value");
    };
    assert_eq!(value_items.len(), 1, "{name}");
    assert_eq!(value_items[0].as_str(), Some("seq2"), "{name}");

    assert_eq!(entries[4].0.as_str(), Some("e"), "{name}");
    let Value::Sequence(e_items) = &entries[4].1.value else {
        panic!("{name}: expected anchored sequence value");
    };
    assert_eq!(e_items.len(), 1, "{name}");
    let Value::Mapping(e_mapping) = &e_items[0].value else {
        panic!("{name}: expected mapping item in anchored sequence");
    };
    assert_eq!(e_mapping[0].0.as_str(), Some("x"), "{name}");
    assert_eq!(e_mapping[0].1.as_str(), Some("y"), "{name}");

    assert_eq!(entries[5].0.as_str(), Some("block"), "{name}");
    assert_eq!(entries[5].1.as_str(), Some("abcde\n"), "{name}");
}

#[test]
fn yts_parse_ab8u__sequence_entry_looking_continuation() {
    let input = include_str!("fixtures/yaml-test-suite/data/AB8U/in.yaml");
    let doc = parse_str(input).expect("parse continuation-looking sequence entry");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_str(), Some("single multiline - sequence entry"));
    assert_eq!(items[0].span.start, 2);
    assert_eq!(items[0].span.end, 36);
    assert_eq!(items[0].span.line, 1);
    assert_eq!(items[0].span.column, 3);

    let events = parse_events(input).expect("parse AB8U events");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, style, .. } => Some((value.as_str(), *style)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        scalars,
        [("single multiline - sequence entry", ScalarStyle::Plain)]
    );
}

#[test]
fn yts_parse_sequence_siblings_remain_structural_after_ab8u() {
    let doc = parse_str("- one\n- two\n").expect("parse sibling sequence");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_str(), Some("one"));
    assert_eq!(items[1].as_str(), Some("two"));
}

#[test]
fn yts_parse_reduced_indentless_sequence_mapping_value() {
    let doc = parse_str("items:\n- one\n- two\nnext: done\n")
        .expect("parse indentless sequence mapping value");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("items"));
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected indentless sequence value");
    };
    assert_eq!(items[0].as_str(), Some("one"));
    assert_eq!(items[1].as_str(), Some("two"));
    assert_eq!(entries[1].0.as_str(), Some("next"));
    assert_eq!(entries[1].1.as_str(), Some("done"));
}

#[test]
fn yts_parse_3gzx__alias_nodes() {
    let input = include_str!("fixtures/yaml-test-suite/data/3GZX/in.yaml");
    let doc = parse_str(input).expect("parse alias nodes");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].0.as_str(), Some("First occurrence"));
    assert_eq!(entries[0].1.as_str(), Some("Foo"));
    assert_eq!(entries[1].0.as_str(), Some("Second occurrence"));
    assert_eq!(entries[1].1.as_str(), Some("Foo"));
    assert_eq!(entries[2].0.as_str(), Some("Override anchor"));
    assert_eq!(entries[2].1.as_str(), Some("Bar"));
    assert_eq!(entries[3].0.as_str(), Some("Reuse anchor"));
    assert_eq!(entries[3].1.as_str(), Some("Bar"));
}

#[test]
fn yts_events_3gzx__preserve_anchor_defs_and_alias_events() {
    let input = include_str!("fixtures/yaml-test-suite/data/3GZX/in.yaml");
    let events = parse_events(input).expect("events");
    let anchored_scalars = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::Scalar { meta, .. }
                    if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "anchor")
            )
        })
        .count();
    let aliases = events
        .iter()
        .filter(|event| matches!(event, Event::Alias { anchor } if anchor.name == "anchor"))
        .count();

    assert_eq!(anchored_scalars, 2);
    assert_eq!(aliases, 2);
}

#[test]
fn yts_parse_u3xv__node_and_mapping_key_anchors() {
    let input = include_str!("fixtures/yaml-test-suite/data/U3XV/in.yaml");
    let doc = parse_str(input).expect("parse mapping key anchor");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("top"));
    let Value::Mapping(nested) = &entries[0].1.value else {
        panic!("expected nested mapping");
    };
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].0.as_str(), Some("key"));
    assert_eq!(nested[0].1.as_str(), Some("value"));
}

#[test]
fn yts_parse_2sxe__anchors_with_colon_in_name() {
    let input = include_str!("fixtures/yaml-test-suite/data/2SXE/in.yaml");
    let doc = parse_str(input).expect("parse colon-bearing anchor names");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("key"));
    assert_eq!(entries[0].1.as_str(), Some("value"));
    assert_eq!(entries[1].0.as_str(), Some("foo"));
    assert_eq!(entries[1].1.as_str(), Some("key"));
}

#[test]
fn yts_events_2sxe__preserve_block_colon_anchor_and_alias_names() {
    let input = include_str!("fixtures/yaml-test-suite/data/2SXE/in.yaml");
    let events = parse_events(input).expect("events for colon-bearing block anchor names");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, meta, .. }
                if value == "key"
                    && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a:")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, meta, .. }
                if value == "value"
                    && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a")
        )
    }));
    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some(anchor.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a:"]);
}

#[test]
fn yts_parse_pw8x__anchors_on_empty_scalars() {
    let input = include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml");
    let doc = parse_str(input).expect("parse anchors on empty scalars");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 6);
    assert!(matches!(items[0].value, Value::Null));
    assert_eq!(items[1].as_str(), Some("a"));

    let Value::Mapping(third) = &items[2].value else {
        panic!("expected mapping with anchored empty scalar key");
    };
    assert_eq!(third.len(), 2);
    assert!(matches!(third[0].0.value, Value::Null));
    assert_eq!(third[0].1.as_str(), Some("a"));
    assert_eq!(third[1].0.as_str(), Some("b"));
    assert!(matches!(third[1].1.value, Value::Null));

    for item in &items[3..] {
        let Value::Mapping(entries) = &item.value else {
            panic!("expected mapping with anchored empty scalar key");
        };
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].0.value, Value::Null));
        assert!(matches!(entries[0].1.value, Value::Null));
    }
}

#[test]
fn yts_events_pw8x__preserve_anchors_on_empty_scalars() {
    let input = include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml");
    let events = parse_events(input).expect("events for anchors on empty scalars");
    let anchored_scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { meta, .. } => meta.anchor.as_ref().map(|anchor| anchor.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(anchored_scalars, ["a", "a", "b", "c", "a", "d", "e", "a"]);
}

#[test]
fn yts_parse_jhb9__two_documents() {
    let input = include_str!("fixtures/yaml-test-suite/data/JHB9/in.yaml");
    let docs = parse_documents(input).expect("parse stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].span.line, 3);
    assert_eq!(docs[1].span.line, 9);
    assert!(
        parse_str(input).is_err(),
        "single-document API rejects streams"
    );
}

#[test]
fn yts_parse_comments_are_ignored_around_documents() {
    let doc = parse_str("# header\nkey: value\n# footer\n").expect("parse comments");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("key"));
    assert_eq!(entries[0].1.as_str(), Some("value"));

    let docs = parse_documents("---\n- a\n# between\n---\n- b\n").expect("parse commented stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].as_str(), None);
    assert_eq!(docs[0].span.line, 2);
    assert_eq!(docs[1].span.line, 5);
}

#[test]
fn yts_events_jhb9__stream_boundaries() {
    let input = include_str!("fixtures/yaml-test-suite/data/JHB9/in.yaml");
    let events = parse_events(input).expect("events");
    assert!(matches!(events.first(), Some(Event::StreamStart)));
    assert!(matches!(events.last(), Some(Event::StreamEnd)));
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
            .filter(|event| matches!(event, Event::DocumentStart { explicit: true, .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        0
    );
}

#[test]
fn yts_parse_explicit_empty_documents_as_null() {
    let input = "---\n---\nname: second\n...\n---\n";
    let docs = parse_documents(input).expect("parse stream with explicit empty docs");
    assert_eq!(docs.len(), 3);
    assert!(matches!(docs[0].value, Value::Null));
    assert!(matches!(docs[2].value, Value::Null));
    assert_eq!(docs[0].span.line, 1);
    assert_eq!(docs[2].span.line, 5);

    let events = parse_events(input).expect("events");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { .. }))
            .count(),
        3
    );
}

#[test]
fn yts_events_mzx3__preserve_scalar_styles() {
    let input = include_str!("fixtures/yaml-test-suite/data/MZX3/in.yaml");
    let events = parse_events(input).expect("events");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, style, .. } => Some((value.as_str(), *style)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        scalars,
        [
            ("plain", ScalarStyle::Plain),
            ("double quoted", ScalarStyle::DoubleQuoted),
            ("single quoted", ScalarStyle::SingleQuoted),
            ("block\n", ScalarStyle::Folded),
            ("plain again", ScalarStyle::Plain),
        ]
    );
}

#[test]
fn yts_events_s4jq__preserve_explicit_non_specific_tag() {
    let input = include_str!("fixtures/yaml-test-suite/data/S4JQ/in.yaml");
    let doc = parse_str(input).expect("parse explicit non-specific tag");
    let Value::Sequence(items) = &doc.value else {
        panic!("expected sequence");
    };
    assert_eq!(items[0].as_str(), Some("12"));
    assert!(matches!(items[1].value, Value::Number(Number::Integer(12))));
    assert_eq!(items[2].as_str(), Some("12"));

    let events = parse_events(input).expect("events");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar {
                value, style, meta, ..
            } => Some((
                value.as_str(),
                *style,
                meta.tag.as_ref().map(|tag| &tag.tag),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(scalars[0], ("12", ScalarStyle::DoubleQuoted, None));
    assert_eq!(scalars[1], ("12", ScalarStyle::Plain, None));
    assert_eq!(scalars[2].0, "12");
    assert_eq!(scalars[2].1, ScalarStyle::Plain);
    assert_eq!(scalars[2].2, Some(&yaml::Tag::new("!")));
}

#[test]
fn yts_events_6m2f__aliases_in_explicit_block_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/6M2F/in.yaml");
    let events = parse_events(input).expect("events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, meta, .. }
                if value == "a"
                    && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, meta, .. }
                if value == "b"
                    && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "b")
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, Event::Alias { anchor } if anchor.name == "a") })
    );
}

#[test]
fn yts_events_bu8l__collection_anchor_and_tag_on_separate_lines() {
    let input = include_str!("fixtures/yaml-test-suite/data/BU8L/in.yaml");
    let events = parse_events(input).expect("events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| {
                    anchor.name == "anchor" && anchor.span.line == 1
                })
                    && meta.tag.as_ref().is_some_and(|tag| {
                        tag.tag == yaml::Tag::new("!!map") && tag.span.line == 2
                    })
        )
    }));
}

#[test]
fn yts_parse_w4tn__yaml_directive_and_explicit_boundaries() {
    let input = include_str!("fixtures/yaml-test-suite/data/W4TN/in.yaml");
    let docs = parse_documents(input).expect("directive stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].as_str(), Some("%!PS-Adobe-2.0\n"));
    assert!(matches!(docs[1].value, Value::Null));

    let events = parse_events(input).expect("events");
    let starts = events
        .iter()
        .filter_map(|event| match event {
            Event::DocumentStart {
                explicit,
                directives,
                span,
            } => Some((*explicit, directives, *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    for ((explicit, directives, span), (document_line, version_line)) in
        starts.iter().zip([(2, 1), (6, 5)])
    {
        assert!(*explicit);
        assert_eq!((span.line, span.column), (document_line, 1));
        assert_eq!(event_source(input, *span), "---");
        let version = directives.yaml_version.as_ref().expect("YAML directive");
        assert_eq!((version.major, version.minor), (1, 2));
        assert_eq!((version.span.line, version.span.column), (version_line, 7));
        assert_eq!(event_source(input, version.span), "1.2");
        assert!(directives.tag_directives.is_empty());
    }
    let ends = events
        .iter()
        .filter_map(|event| match event {
            Event::DocumentEnd { explicit, span } => Some((*explicit, *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2);
    for ((explicit, span), line) in ends.iter().zip([4, 8]) {
        assert!(*explicit);
        assert_eq!((span.line, span.column), (line, 1));
        assert_eq!(event_source(input, *span), "...");
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { explicit: true, .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                Event::DocumentEnd {
                    explicit: false,
                    ..
                }
            ))
            .count(),
        0
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value == "%!PS-Adobe-2.0\n"
        )
    }));
}

#[test]
fn yts_parse_bec7_mus6__yaml_version_directive_variants_are_syntax_only() {
    for (name, input, version) in [
        (
            "BEC7",
            include_str!("fixtures/yaml-test-suite/data/BEC7/in.yaml"),
            (1, 3, "1.3"),
        ),
        (
            "MUS6/02",
            include_str!("fixtures/yaml-test-suite/data/MUS6-02/in.yaml"),
            (1, 1, "1.1"),
        ),
        (
            "MUS6/03",
            include_str!("fixtures/yaml-test-suite/data/MUS6-03/in.yaml"),
            (1, 1, "1.1"),
        ),
        (
            "MUS6/04",
            include_str!("fixtures/yaml-test-suite/data/MUS6-04/in.yaml"),
            (1, 1, "1.1"),
        ),
    ] {
        let docs = parse_documents(input).unwrap_or_else(|error| panic!("parse {name}: {error}"));
        assert_eq!(docs.len(), 1, "{name}");
        let values: Vec<yaml::Value> = yaml::from_documents_str(input)
            .unwrap_or_else(|error| panic!("Serde documents for {name}: {error}"));
        assert_eq!(values.len(), 1, "{name}");

        let events =
            parse_events(input).unwrap_or_else(|error| panic!("events for {name}: {error}"));
        let version_meta = events.iter().find_map(|event| match event {
            Event::DocumentStart { directives, .. } => directives.yaml_version.as_ref(),
            _ => None,
        });
        let version_meta = version_meta.unwrap_or_else(|| panic!("{name} YAML directive metadata"));
        assert_eq!(
            (version_meta.major, version_meta.minor),
            (version.0, version.1)
        );
        assert_eq!(event_source(input, version_meta.span), version.2);
    }

    let doc = parse_str("%YAML 1.1\n---\non: off\nyes: no\n")
        .expect("version directive remains schema-neutral");
    let yaml::NodeValue::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("off"));
    assert_eq!(entries[1].0.as_str(), Some("yes"));
    assert_eq!(entries[1].1.as_str(), Some("no"));

    let error = yaml::LoadOptions::yaml_version_directive()
        .parse_str("%YAML 1.1\n---\non: off\nyes: no\n")
        .expect_err("directive-driven YAML 1.1 resolves colliding boolean keys");
    assert!(error.to_string().contains("duplicate mapping key `true`"));
}

#[test]
fn yts_parse_fp8r__zero_indented_folded_block_scalar_after_document_start() {
    let input = include_str!("fixtures/yaml-test-suite/data/FP8R/in.yaml");
    let doc = parse_str(input).expect("parse zero-indented folded scalar after document start");
    assert_eq!(doc.as_str(), Some("line1 line2 line3\n"));

    let events = parse_events(input).expect("events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Folded,
                span,
                ..
            } if value == "line1 line2 line3\n"
                && (span.line, span.column) == (1, 5)
                && event_source(input, *span) == ">\nline1\nline2\nline3"
        )
    }));
}

#[test]
fn yts_parse_dk3j__zero_indented_folded_block_scalar_keeps_comment_looking_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/DK3J/in.yaml");
    let doc = parse_str(input).expect("parse zero-indented folded scalar with comment-like line");
    assert_eq!(doc.as_str(), Some("line1 # no comment line3\n"));

    let events = parse_events(input).expect("events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Folded,
                span,
                ..
            } if value == "line1 # no comment line3\n"
                && (span.line, span.column) == (1, 5)
                && event_source(input, *span) == ">\nline1\n# no comment\nline3"
        )
    }));
}

#[test]
fn yts_parse_6lvf__reserved_directive_is_ignored() {
    let input = include_str!("fixtures/yaml-test-suite/data/6LVF/in.yaml");
    let doc = parse_str(input).expect("parse reserved directive fixture");
    assert_eq!(doc.as_str(), Some("foo"));

    let events = parse_events(input).expect("events for reserved directive fixture");
    let directives = events.iter().find_map(|event| match event {
        Event::DocumentStart { directives, .. } => Some(directives),
        _ => None,
    });
    let directives = directives.expect("document start directives");
    assert!(directives.yaml_version.is_none());
    assert!(directives.tag_directives.is_empty());
}

#[test]
fn yts_parse_m7a3__bare_documents_after_document_end() {
    let input = include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml");
    let docs = parse_documents(input).expect("bare document stream");

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].as_str(), Some("Bare document"));
    assert_eq!(
        docs[1].as_str(),
        Some("%!PS-Adobe-2.0 # Not the first line\n")
    );
}

#[test]
fn yts_events_m7a3__bare_documents_and_explicit_end_markers() {
    let input = include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml");
    let events = parse_events(input).expect("bare document events");

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
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                Event::DocumentEnd {
                    explicit: false,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "Bare document"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value == "%!PS-Adobe-2.0 # Not the first line\n"
        )
    }));
}

#[test]
fn yts_parse_ut92__directive_looking_flow_mapping_key_in_explicit_documents() {
    let input = include_str!("fixtures/yaml-test-suite/data/UT92/in.yaml");
    let docs = parse_documents(input).expect("parse explicit document stream");

    assert_eq!(docs.len(), 2);
    let Value::Mapping(entries) = &docs[0].value else {
        panic!("expected first document mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("matches %"));
    assert!(matches!(
        entries[0].1.value,
        Value::Number(Number::Integer(20))
    ));
    assert!(matches!(docs[1].value, Value::Null));
}

#[test]
fn yts_events_ut92__directive_looking_line_remains_flow_content() {
    let input = include_str!("fixtures/yaml-test-suite/data/UT92/in.yaml");
    let events = parse_events(input).expect("events for explicit document stream");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { explicit: true, .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        2
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                span,
                ..
            } if value == "matches %"
                && (span.line, span.column) == (2, 3)
                && event_source(input, *span).contains('%')
        )
    }));
}

#[test]
fn yts_parse_reduced_root_multiline_plain_scalar() {
    let doc = parse_str("Bare\ndocument\n").expect("parse root multiline plain scalar");
    assert_eq!(doc.as_str(), Some("Bare document"));

    let with_blank = parse_str("a\n\nb\n").expect("parse root plain scalar with blank line");
    assert_eq!(with_blank.as_str(), Some("a\nb"));
}

#[test]
fn yts_parse_reduced_root_literal_allows_indent_zero_content() {
    let doc = parse_str("|\n%!PS-Adobe-2.0 # Not the first line\n")
        .expect("parse root literal scalar with indent-zero content");
    assert_eq!(doc.as_str(), Some("%!PS-Adobe-2.0 # Not the first line\n"));
}

#[test]
fn yts_parse_reduced_root_block_indicators_win_over_plain_continuation() {
    let literal = parse_str("|\n  line\n").expect("parse root literal scalar");
    assert_eq!(literal.as_str(), Some("line\n"));

    let folded = parse_str(">\n  first\n  second\n").expect("parse root folded scalar");
    assert_eq!(folded.as_str(), Some("first second\n"));

    let events = parse_events("|\n  line\n").expect("root literal events");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value == "line\n"
        )
    }));
}

#[test]
fn yts_events_u3c3__tag_directive_resolves_handle() {
    let input = include_str!("fixtures/yaml-test-suite/data/U3C3/in.yaml");
    let node = parse_str(input).expect("tag directive tree");
    let Value::Tagged(tagged) = &node.value else {
        panic!("expected tagged root");
    };
    assert_eq!(tagged.tag.handle, "!");
    assert_eq!(tagged.tag.suffix, "tag:yaml.org,2002:str");
    assert_eq!(tagged.value.as_str(), Some("foo"));

    let events = parse_events(input).expect("tag directive events");
    let Some(Event::DocumentStart {
        directives, span, ..
    }) = events.get(1)
    else {
        panic!("expected document start");
    };
    assert_eq!((span.line, span.column), (2, 1));
    assert_eq!(event_source(input, *span), "---");
    assert_eq!(directives.tag_directives.len(), 1);
    let directive = &directives.tag_directives[0];
    assert_eq!(directive.handle, "!yaml!");
    assert_eq!(directive.prefix, "tag:yaml.org,2002:");
    assert_eq!((directive.span.line, directive.span.column), (1, 1));
    assert_eq!(
        event_source(input, directive.span),
        "%TAG !yaml! tag:yaml.org,2002:"
    );
    assert_eq!(
        (directive.handle_span.line, directive.handle_span.column),
        (1, 6)
    );
    assert_eq!(event_source(input, directive.handle_span), "!yaml!");
    assert_eq!(
        (directive.prefix_span.line, directive.prefix_span.column),
        (1, 13)
    );
    assert_eq!(
        event_source(input, directive.prefix_span),
        "tag:yaml.org,2002:"
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::DoubleQuoted,
                meta,
                ..
            } if value == "foo"
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.handle == "!" && tag.tag.suffix == "tag:yaml.org,2002:str"
                        && tag.span.line == 3
                        && tag.span.column == 1
                        && event_source(input, tag.span) == "!yaml!str"
                })
        )
    }));
    let foo = events
        .iter()
        .find_map(|event| match event {
            Event::Scalar { value, span, .. } if value == "foo" => Some(*span),
            _ => None,
        })
        .expect("foo scalar event");
    assert_eq!((foo.line, foo.column), (3, 11));
    assert_eq!(event_source(input, foo), "\"foo\"");
}

#[test]
fn yts_parse_6ck3__tag_shorthands_decode_suffix() {
    let input = include_str!("fixtures/yaml-test-suite/data/6CK3/in.yaml");
    let node = parse_str(input).expect("tag shorthand tree");
    let Value::Sequence(items) = &node.value else {
        panic!("expected root sequence");
    };
    assert_eq!(items.len(), 3);

    let Value::Tagged(local) = &items[0].value else {
        panic!("expected local tagged scalar");
    };
    assert_eq!(local.tag, yaml::Tag::new("!local"));
    assert_eq!(local.value.as_str(), Some("foo"));

    let Value::Tagged(core) = &items[1].value else {
        panic!("expected core tagged scalar");
    };
    assert_eq!(core.tag, yaml::Tag::new("!!str"));
    assert_eq!(core.value.as_str(), Some("bar"));

    let Value::Tagged(shorthand) = &items[2].value else {
        panic!("expected shorthand tagged scalar");
    };
    assert_eq!(shorthand.tag.handle, "!");
    assert_eq!(shorthand.tag.suffix, "tag:example.com,2000:app/tag!");
    assert_eq!(shorthand.value.as_str(), Some("baz"));

    let events = parse_events(input).expect("tag shorthand events");
    let Some(Event::DocumentStart { directives, .. }) = events.get(1) else {
        panic!("expected document start");
    };
    assert_eq!(directives.tag_directives.len(), 1);
    assert_eq!(directives.tag_directives[0].handle, "!e!");
    assert_eq!(
        directives.tag_directives[0].prefix,
        "tag:example.com,2000:app/"
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                ..
            } if value == "baz"
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag.handle == "!"
                        && tag.tag.suffix == "tag:example.com,2000:app/tag!"
                        && event_source(input, tag.span) == "!e!tag%21"
                })
        )
    }));
}

#[test]
fn yts_parse_9kax__tag_anchor_property_combinations() {
    let input = include_str!("fixtures/yaml-test-suite/data/9KAX/in.yaml");
    let docs = parse_documents(input).expect("tag and anchor property combinations");
    assert_eq!(docs.len(), 8);

    let assert_tagged_scalar = |idx: usize, tag: &str, value: &str| {
        let Value::Tagged(tagged) = &docs[idx].value else {
            panic!("expected document {idx} to be tagged");
        };
        assert_eq!(tagged.tag, yaml::Tag::new(tag));
        assert_eq!(tagged.value.as_str(), Some(value));
    };
    assert_tagged_scalar(0, "!!str", "scalar1");
    assert_tagged_scalar(1, "!!str", "scalar2");
    assert_tagged_scalar(2, "!!str", "scalar3");
    assert_tagged_scalar(7, "!!str", "value11");

    let Value::Tagged(tagged_map) = &docs[3].value else {
        panic!("expected fourth document to be a tagged mapping");
    };
    assert_eq!(tagged_map.tag, yaml::Tag::new("!!map"));
    let Value::Mapping(entries) = &tagged_map.value.value else {
        panic!("expected fourth document tagged value to be a mapping");
    };
    let Value::Tagged(tagged_key) = &entries[0].0.value else {
        panic!("expected key5 to retain its tag");
    };
    assert_eq!(tagged_key.tag, yaml::Tag::new("!!str"));
    assert_eq!(tagged_key.value.as_str(), Some("key5"));
    assert_eq!(entries[0].1.as_str(), Some("value4"));

    let Value::Mapping(entries) = &docs[4].value else {
        panic!("expected fifth document to be a mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("a6"));
    let Value::Number(first_value) = &entries[0].1.value else {
        panic!("expected a6 value to be a number");
    };
    assert_eq!(first_value.as_i64(), Some(1));
    assert_eq!(entries[1].0.as_str(), Some("b6"));
    let Value::Number(second_value) = &entries[1].1.value else {
        panic!("expected b6 value to be a number");
    };
    assert_eq!(second_value.as_i64(), Some(2));

    for (idx, key, value) in [(5, "key8", "value7"), (6, "key10", "value9")] {
        let Value::Tagged(tagged_map) = &docs[idx].value else {
            panic!("expected document {idx} to be a tagged mapping");
        };
        assert_eq!(tagged_map.tag, yaml::Tag::new("!!map"));
        let Value::Mapping(entries) = &tagged_map.value.value else {
            panic!("expected document {idx} tagged value to be a mapping");
        };
        let Value::Tagged(tagged_key) = &entries[0].0.value else {
            panic!("expected tagged mapping key in document {idx}");
        };
        assert_eq!(tagged_key.tag, yaml::Tag::new("!!str"));
        assert_eq!(tagged_key.value.as_str(), Some(key));
        assert_eq!(entries[0].1.as_str(), Some(value));
    }

    let events = parse_events(input).expect("tag and anchor property events");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { .. }))
            .count(),
        8
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a4")
                    && meta
                        .tag
                        .as_ref()
                        .is_some_and(|tag| tag.tag == yaml::Tag::new("!!map"))
        )
    }));

    let has_tagged_scalar = |expected_value: &str, expected_anchor: &str, expected_tag: &str| {
        events.iter().any(|event| {
            matches!(
                event,
                Event::Scalar { value, meta, .. }
                    if value == expected_value
                        && meta
                            .anchor
                            .as_ref()
                            .is_some_and(|anchor| anchor.name == expected_anchor)
                        && meta
                            .tag
                            .as_ref()
                            .is_some_and(|tag| tag.tag == yaml::Tag::new(expected_tag))
            )
        })
    };
    assert!(has_tagged_scalar("key5", "a5", "!!str"));
    assert!(has_tagged_scalar("key8", "a8", "!!str"));
    assert!(has_tagged_scalar("key10", "a10", "!!str"));
    assert!(has_tagged_scalar("value11", "a11", "!!str"));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, meta, .. }
                if value == "b6"
                    && meta
                        .anchor
                        .as_ref()
                        .is_some_and(|anchor| anchor.name == "anchor6")
        )
    }));
}

#[test]
fn yts_events_fta2__document_start_anchor_applies_to_root_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/FTA2/in.yaml");
    let events = parse_events(input).expect("events");

    assert!(matches!(
        events.get(1),
        Some(Event::DocumentStart { explicit: true, .. })
    ));
    let Some(Event::DocumentStart { span, .. }) = events.get(1) else {
        panic!("expected document start");
    };
    assert_eq!((span.line, span.column), (1, 1));
    assert_eq!(event_source(input, *span), "---");
    let Some(Event::SequenceStart { meta, span, .. }) = events
        .iter()
        .find(|event| matches!(event, Event::SequenceStart { .. }))
    else {
        panic!("expected sequence start");
    };
    assert_eq!((span.line, span.column), (2, 1));
    let anchor = meta.anchor.as_ref().expect("sequence anchor");
    assert_eq!(anchor.name, "sequence");
    assert_eq!((anchor.span.line, anchor.span.column), (1, 5));
    assert_eq!(event_source(input, anchor.span), "&sequence");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, span, .. }
                if value == "a"
                    && span.line == 2
                    && span.column == 3
                    && event_source(input, *span) == "a"
        )
    }));
}

#[test]
fn yts_reject_4ejs__tab_indentation() {
    let input = include_str!("fixtures/yaml-test-suite/data/4EJS/in.yaml");
    let error = parse_str(input).expect_err("tabs must be rejected");
    assert!(error.to_string().contains("tabs are not allowed"));
    assert_eq!(error.span().line, 2);
}

#[test]
fn yts_reject_y79y__tab_only_block_scalar_content_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y/in.yaml");
    let error = parse_str(input).expect_err("tab-starting block scalar content rejected");
    assert!(
        error
            .to_string()
            .contains("block scalar content cannot start with a tab")
    );
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 1);

    let events_error =
        parse_events(input).expect_err("event parser rejects tab-starting block scalar content");
    assert!(
        events_error
            .to_string()
            .contains("block scalar content cannot start with a tab")
    );
    assert_eq!(events_error.span().line, 2);
    assert_eq!(events_error.span().column, 1);
}

#[test]
fn yts_parse_y79y_001__space_tab_block_scalar_content_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-001/in.yaml");
    let doc = parse_str(input).expect("parse space-tab block scalar content");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("foo"));
    assert_eq!(entries[0].1.as_str(), Some("\t\n"));
    assert_eq!(entries[1].0.as_str(), Some("bar"));
    assert!(matches!(
        entries[1].1.value,
        Value::Number(Number::Integer(1))
    ));

    let events = parse_events(input).expect("event parser accepts space-tab block scalar content");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, .. } if value == "\t\n"
        )
    }));
}

#[test]
fn yts_parse_y79y_002__tab_only_flow_sequence_separation_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-002/in.yaml");
    let doc = parse_str(input).expect("parse tab-only flow sequence separation line");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    let Value::Sequence(nested) = &items[0].value else {
        panic!("expected nested flow sequence");
    };
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].as_str(), Some("foo"));

    let events = parse_events(input).expect("event parser accepts tab-only flow separation line");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, .. } if value == "foo"
        )
    }));
}

#[test]
fn yts_parse_6ca3__tab_before_root_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/6CA3/in.yaml");
    let doc = parse_str(input).expect("parse tab-separated root flow sequence");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert!(items.is_empty());

    let events =
        parse_events(input).expect("event parser accepts tab-separated root flow sequence");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart {
                style: yaml::CollectionStyle::Flow,
                ..
            }
        )
    }));
}

#[test]
fn yts_parse_q5mg__tab_before_root_flow_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/Q5MG/in.yaml");
    let doc = parse_str(input).expect("parse tab-separated root flow mapping");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert!(entries.is_empty());

    let events = parse_events(input).expect("event parser accepts tab-separated root flow mapping");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart {
                style: yaml::CollectionStyle::Flow,
                ..
            }
        )
    }));
}

#[test]
fn yts_parse_6bct__separation_spaces() {
    let input = include_str!("fixtures/yaml-test-suite/data/6BCT/in.yaml");
    let doc = parse_str(input).expect("parse tabs used as separation spaces");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);
    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("foo"));
    assert_eq!(first[0].1.as_str(), Some("bar"));
    let Value::Sequence(second) = &items[1].value else {
        panic!("expected nested sequence");
    };
    assert_eq!(second[0].as_str(), Some("baz"));
    assert_eq!(second[1].as_str(), Some("baz"));

    let events = parse_events(input).expect("event parser accepts tabs used as separation spaces");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(scalars, ["foo", "bar", "baz", "baz"]);
}

#[test]
fn yts_parse_y79y_valid_negative_scalar_after_tab_separator() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-010/in.yaml");
    let doc = parse_str(input).expect("parse tab-separated negative scalar");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].value, Value::Number(Number::Integer(-1))));

    let events = parse_events(input).expect("event parser accepts tab-separated negative scalar");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar { value, .. } if value == "-1"
        )
    }));
}

#[test]
fn yts_parse_double_quoted_tabs_and_same_indent_continuations() {
    for (name, input, expected) in [
        (
            "3RLN-001",
            include_str!("fixtures/yaml-test-suite/data/3RLN-001/in.yaml"),
            "2 leading \ttab",
        ),
        (
            "3RLN-002",
            include_str!("fixtures/yaml-test-suite/data/3RLN-002/in.yaml"),
            "3 leading tab",
        ),
        (
            "DE56/00",
            include_str!("fixtures/yaml-test-suite/data/DE56-00/in.yaml"),
            "1 trailing\t tab",
        ),
        (
            "DE56/01",
            include_str!("fixtures/yaml-test-suite/data/DE56-01/in.yaml"),
            "2 trailing\t tab",
        ),
        (
            "DE56/02",
            include_str!("fixtures/yaml-test-suite/data/DE56-02/in.yaml"),
            "3 trailing\t tab",
        ),
        (
            "DE56/03",
            include_str!("fixtures/yaml-test-suite/data/DE56-03/in.yaml"),
            "4 trailing\t tab",
        ),
        (
            "DE56/04",
            include_str!("fixtures/yaml-test-suite/data/DE56-04/in.yaml"),
            "5 trailing tab",
        ),
        (
            "DE56/05",
            include_str!("fixtures/yaml-test-suite/data/DE56-05/in.yaml"),
            "6 trailing tab",
        ),
        (
            "KH5V-001",
            include_str!("fixtures/yaml-test-suite/data/KH5V-001/in.yaml"),
            "2 inline\ttab",
        ),
        (
            "6WPF",
            include_str!("fixtures/yaml-test-suite/data/6WPF/in.yaml"),
            " foo\nbar\nbaz ",
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        assert_eq!(doc.as_str(), Some(expected), "{name}");

        let events = parse_events(input)
            .unwrap_or_else(|error| panic!("{name} event parser accepts: {error}"));
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    Event::Scalar {
                        value,
                        style: ScalarStyle::DoubleQuoted,
                        ..
                    } if value == expected
                )
            }),
            "{name} exposes expected double-quoted scalar event"
        );
    }
}

#[test]
fn yts_parse_dk95__tab_looking_indentation_variants() {
    for (name, input, expected) in [
        (
            "DK95/00",
            include_str!("fixtures/yaml-test-suite/data/DK95-00/in.yaml"),
            "bar",
        ),
        (
            "DK95/02",
            include_str!("fixtures/yaml-test-suite/data/DK95-02/in.yaml"),
            "bar baz",
        ),
        (
            "DK95/08",
            include_str!("fixtures/yaml-test-suite/data/DK95-08/in.yaml"),
            "bar baz \t \t ",
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        let Value::Mapping(entries) = &doc.value else {
            panic!("{name} should load as a mapping");
        };
        assert_eq!(entries.len(), 1, "{name}");
        assert_eq!(entries[0].0.as_str(), Some("foo"), "{name}");
        assert_eq!(entries[0].1.as_str(), Some(expected), "{name}");

        let events = parse_events(input)
            .unwrap_or_else(|error| panic!("{name} event parser accepts: {error}"));
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    Event::Scalar { value, .. } if value == expected
                )
            }),
            "{name} exposes the expected folded scalar"
        );
    }

    for (name, input, expected_len) in [
        (
            "DK95/03",
            include_str!("fixtures/yaml-test-suite/data/DK95-03/in.yaml"),
            1,
        ),
        (
            "DK95/04",
            include_str!("fixtures/yaml-test-suite/data/DK95-04/in.yaml"),
            2,
        ),
        (
            "DK95/05",
            include_str!("fixtures/yaml-test-suite/data/DK95-05/in.yaml"),
            2,
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        let Value::Mapping(entries) = &doc.value else {
            panic!("{name} should load as a mapping");
        };
        assert_eq!(entries.len(), expected_len, "{name}");
        assert_eq!(entries[0].0.as_str(), Some("foo"), "{name}");
        assert!(matches!(
            entries[0].1.value,
            Value::Number(Number::Integer(1))
        ));
        if expected_len == 2 {
            assert_eq!(entries[1].0.as_str(), Some("bar"), "{name}");
            assert!(matches!(
                entries[1].1.value,
                Value::Number(Number::Integer(2))
            ));
        }

        parse_events(input).unwrap_or_else(|error| panic!("{name} event parser accepts: {error}"));
    }

    let doc = parse_str(include_str!(
        "fixtures/yaml-test-suite/data/DK95-07/in.yaml"
    ))
    .expect("DK95/07 parses tab-only line before explicit document start");
    assert!(matches!(doc.value, Value::Null));
    parse_events(include_str!(
        "fixtures/yaml-test-suite/data/DK95-07/in.yaml"
    ))
    .expect("DK95/07 event parser accepts tab-only line before document start");
}

#[test]
fn yts_parse_double_quoted_even_backslash_fold_preserves_literal_backslash() {
    let cases = [
        ("even-two-backslashes", "value: \"a\\\\\n  b\"\n", "a\\ b"),
        (
            "even-four-backslashes",
            "value: \"a\\\\\\\\\n  b\"\n",
            "a\\\\ b",
        ),
        ("odd-one-backslash", "value: \"a\\\n  b\"\n", "ab"),
    ];

    for (name, input, expected) in cases {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        let Value::Mapping(entries) = doc.value else {
            panic!("{name}: expected mapping");
        };
        assert_eq!(entries[0].1.as_str(), Some(expected), "{name}");

        let events = parse_events(input)
            .unwrap_or_else(|error| panic!("{name} event parser accepts: {error}"));
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    Event::Scalar {
                        value,
                        style: ScalarStyle::DoubleQuoted,
                        ..
                    } if value == expected
                )
            }),
            "{name} exposes expected double-quoted scalar event"
        );
    }
}

#[test]
fn yts_parse_kss4__same_indent_double_quoted_stream_scalar() {
    let input = include_str!("fixtures/yaml-test-suite/data/KSS4/in.yaml");
    let docs = parse_documents(input).expect("parse KSS4 stream");
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].as_str(), Some("quoted string"));
    assert_eq!(docs[1].as_str(), Some("foo"));

    let events = parse_events(input).expect("event parser accepts KSS4 stream");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, style, .. } => Some((value.as_str(), *style)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(scalars.contains(&("quoted string", ScalarStyle::DoubleQuoted)));
    assert!(scalars.contains(&("foo", ScalarStyle::Plain)));
}

#[test]
fn yts_reject_y79y_remaining_tab_indicator_forms() {
    for (name, input, column) in [
        (
            "Y79Y-004",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-004/in.yaml"),
            2,
        ),
        (
            "Y79Y-005",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-005/in.yaml"),
            3,
        ),
        (
            "Y79Y-006",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-006/in.yaml"),
            2,
        ),
        (
            "Y79Y-008",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-008/in.yaml"),
            2,
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("{name} tab separation rejected"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("tabs are not allowed as separation after block indicators"),
            "{name}: {error}"
        );
        assert_eq!(error.span().line, 1, "{name}");
        assert_eq!(error.span().column, column, "{name}");

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} event parser rejects tab"),
            Err(error) => error,
        };
        assert!(
            events_error
                .to_string()
                .contains("tabs are not allowed as separation after block indicators"),
            "{name}: {events_error}"
        );
        assert_eq!(events_error.span().line, 1, "{name}");
        assert_eq!(events_error.span().column, column, "{name}");
    }
}

#[test]
fn yts_reject_6jtt__unclosed_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/6JTT/in.yaml");
    let error = parse_str(input).expect_err("unclosed flow sequence");
    assert!(error.to_string().contains("expected `]`"));

    let events_error = parse_events(input).expect_err("event parser rejects unclosed sequence");
    assert!(events_error.to_string().contains("expected `]`"));
}

#[test]
fn yts_reject_ctn5__extra_flow_comma_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/CTN5/in.yaml");
    let error = parse_str(input).expect_err("extra flow comma rejected");
    assert!(error.to_string().contains("unexpected comma"));

    let events_error = parse_events(input).expect_err("event parser rejects extra comma");
    assert!(events_error.to_string().contains("unexpected comma"));
}

#[test]
fn yts_reject_yjv2__dash_in_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/YJV2/in.yaml");
    let error = parse_str(input).expect_err("dash flow entry rejected");
    assert!(error.to_string().contains("plain scalar cannot start"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 2);

    let events_error = parse_events(input).expect_err("event parser rejects dash flow entry");
    assert!(
        events_error
            .to_string()
            .contains("plain scalar cannot start")
    );
    assert_eq!(events_error.span().line, 1);
    assert_eq!(events_error.span().column, 2);
}

#[test]
fn yts_reject_jy7z_q4cl__trailing_content_after_double_quoted_mapping_values() {
    for (name, input) in [
        (
            "JY7Z",
            include_str!("fixtures/yaml-test-suite/data/JY7Z/in.yaml"),
        ),
        (
            "Q4CL",
            include_str!("fixtures/yaml-test-suite/data/Q4CL/in.yaml"),
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("{name} trailing quoted scalar content rejected"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("unexpected trailing characters after quoted scalar"),
            "{name}: {error}"
        );
        assert_eq!(error.span().line, 2, "{name}");
        assert_eq!(error.span().column, 17, "{name}");

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} event parser rejects trailing content"),
            Err(error) => error,
        };
        assert!(
            events_error
                .to_string()
                .contains("unexpected trailing characters after quoted scalar"),
            "{name}: {events_error}"
        );
        assert_eq!(events_error.span().line, 2, "{name}");
        assert_eq!(events_error.span().column, 17, "{name}");
    }
}

#[test]
fn yts_reject_qb6e_dk95__wrong_indented_multiline_double_quoted_scalars() {
    for (name, input, expected, line, column) in [
        (
            "QB6E",
            include_str!("fixtures/yaml-test-suite/data/QB6E/in.yaml"),
            "multiline quoted scalar continuation is not sufficiently indented",
            3,
            1,
        ),
        (
            "DK95/01",
            include_str!("fixtures/yaml-test-suite/data/DK95-01/in.yaml"),
            "tabs are not allowed for indentation",
            2,
            1,
        ),
        (
            "DK95/06",
            include_str!("fixtures/yaml-test-suite/data/DK95-06/in.yaml"),
            "tabs are not allowed for indentation",
            3,
            3,
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("{name} wrong-indented multiline quoted scalar rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(expected), "{name}: {error}");
        assert_eq!(error.span().line, line, "{name}");
        assert_eq!(error.span().column, column, "{name}");

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} event parser rejects wrong-indented multiline quoted scalar"),
            Err(error) => error,
        };
        assert!(
            events_error.to_string().contains(expected),
            "{name}: {events_error}"
        );
        assert_eq!(events_error.span().line, line, "{name}");
        assert_eq!(events_error.span().column, column, "{name}");
    }
}

#[test]
fn yts_reject_9jba__comment_after_flow_sequence_requires_separation() {
    let input = include_str!("fixtures/yaml-test-suite/data/9JBA/in.yaml");
    let error = parse_str(input).expect_err("adjacent comment after flow sequence rejected");
    assert!(error.to_string().contains("unexpected trailing characters"));
    assert_eq!(error.span().line, 2);

    let events_error =
        parse_events(input).expect_err("event parser rejects adjacent flow sequence comment");
    assert!(
        events_error
            .to_string()
            .contains("unexpected trailing characters")
    );
    assert_eq!(events_error.span().line, 2);
}

#[test]
fn yts_reject_cvw2__comment_looking_flow_sequence_entry_requires_separation() {
    let input = include_str!("fixtures/yaml-test-suite/data/CVW2/in.yaml");
    let error = parse_str(input).expect_err("comment-looking flow sequence entry rejected");
    assert!(
        error
            .to_string()
            .contains("comments must be separated from other tokens")
    );
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 11);

    let events_error =
        parse_events(input).expect_err("event parser rejects comment-looking flow entry");
    assert!(
        events_error
            .to_string()
            .contains("comments must be separated from other tokens")
    );
    assert_eq!(events_error.span().line, 2);
    assert_eq!(events_error.span().column, 11);
}

#[test]
fn yts_reject_9c9n__wrong_indented_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/9C9N/in.yaml");
    let error = parse_str(input).expect_err("wrong indented flow sequence rejected");
    assert!(error.to_string().contains("sufficiently indented"));
    assert_eq!(error.span().line, 3);
    assert_eq!(error.span().column, 1);

    let events_error =
        parse_events(input).expect_err("event parser rejects wrong indented flow sequence");
    assert!(events_error.to_string().contains("sufficiently indented"));
    assert_eq!(events_error.span().line, 3);
    assert_eq!(events_error.span().column, 1);
}

#[test]
fn yts_reject_dk4h__implicit_flow_sequence_key_before_newline() {
    let input = include_str!("fixtures/yaml-test-suite/data/DK4H/in.yaml");
    let error = parse_str(input).expect_err("implicit flow sequence key newline rejected");
    assert!(error.to_string().contains("expected `,`"));
    assert_eq!(error.span().line, 3);

    let events_error = parse_events(input).expect_err("event parser rejects implicit key newline");
    assert!(events_error.to_string().contains("expected `,`"));
    assert_eq!(events_error.span().line, 3);
}

#[test]
fn yts_reject_zxt5__implicit_flow_sequence_key_before_adjacent_value() {
    let input = include_str!("fixtures/yaml-test-suite/data/ZXT5/in.yaml");
    let error = parse_str(input).expect_err("implicit flow sequence adjacent value rejected");
    assert!(error.to_string().contains("expected `]`"));
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 3);

    let events_error =
        parse_events(input).expect_err("event parser rejects implicit adjacent value newline");
    assert!(events_error.to_string().contains("expected `]`"));
    assert_eq!(events_error.span().line, 2);
    assert_eq!(events_error.span().column, 3);
}

#[test]
fn yts_reject_236b__invalid_value_after_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/236B/in.yaml");
    let error = parse_str(input).expect_err("invalid value after mapping rejected");
    assert_eq!(error.span().line, 3);
    assert_eq!(error.span().column, 1);

    let events_error = parse_events(input).expect_err("event parser rejects invalid mapping value");
    assert_eq!(events_error.span().line, 3);
    assert_eq!(events_error.span().column, 1);
}

#[test]
fn yts_reject_5llu__block_scalar_bad_leading_blank_indentation() {
    let input = include_str!("fixtures/yaml-test-suite/data/5LLU/in.yaml");
    let error = parse_str(input).expect_err("bad block scalar indentation rejected");
    assert!(
        error
            .to_string()
            .contains("block scalar content is less indented")
    );
    assert_eq!(error.span().line, 5);
    assert_eq!(error.span().column, 2);

    let events_error =
        parse_events(input).expect_err("event parser rejects bad block scalar indentation");
    assert!(
        events_error
            .to_string()
            .contains("block scalar content is less indented")
    );
    assert_eq!(events_error.span().line, 5);
    assert_eq!(events_error.span().column, 2);
}

#[test]
fn yts_parse_ske5__anchor_before_zero_indented_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/SKE5/in.yaml");
    let doc = parse_str(input).expect("parse anchored zero-indented sequence value");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("seq"));
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected anchored sequence value");
    };
    assert_eq!(items[0].as_str(), Some("a"));
    assert_eq!(items[1].as_str(), Some("b"));
}

#[test]
fn yts_events_ske5__anchor_applies_to_zero_indented_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/SKE5/in.yaml");
    let events = parse_events(input).expect("events for anchored zero-indented sequence value");
    let Some(Event::SequenceStart { meta, span, .. }) = events
        .iter()
        .find(|event| matches!(event, Event::SequenceStart { .. }))
    else {
        panic!("expected sequence start");
    };
    assert_eq!((span.line, span.column), (4, 1));
    assert_eq!(event_source(input, *span), "- a");
    let anchor = meta.anchor.as_ref().expect("sequence anchor");
    assert_eq!(anchor.name, "anchor");
    assert_eq!((anchor.span.line, anchor.span.column), (3, 2));
    assert_eq!(event_source(input, anchor.span), "&anchor");
}

#[test]
fn yts_reject_sy6v__anchor_before_sequence_entry_on_same_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/SY6V/in.yaml");
    let error = parse_str(input).expect_err("same-line block sequence after anchor rejected");
    assert!(
        error
            .to_string()
            .contains("block sequence entries are not allowed")
    );
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 9);

    let events_error =
        parse_events(input).expect_err("event parser rejects same-line sequence after anchor");
    assert!(
        events_error
            .to_string()
            .contains("block sequence entries are not allowed")
    );
    assert_eq!(events_error.span().line, 1);
    assert_eq!(events_error.span().column, 9);
}

#[test]
fn yts_reject_5u3a__sequence_on_same_line_as_mapping_key() {
    let input = include_str!("fixtures/yaml-test-suite/data/5U3A/in.yaml");
    let error = parse_str(input).expect_err("same-line block sequence mapping value rejected");
    assert!(
        error
            .to_string()
            .contains("block sequence entries are not allowed")
    );
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 6);

    let events_error =
        parse_events(input).expect_err("event parser rejects same-line sequence mapping value");
    assert!(
        events_error
            .to_string()
            .contains("block sequence entries are not allowed")
    );
    assert_eq!(events_error.span().line, 1);
    assert_eq!(events_error.span().column, 6);
}

#[test]
fn yts_reject_zcz6__nested_mapping_in_plain_single_line_value() {
    let input = include_str!("fixtures/yaml-test-suite/data/ZCZ6/in.yaml");
    let error = parse_str(input).expect_err("nested mapping in plain value rejected");
    assert!(error.to_string().contains("mapping values are not allowed"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 5);

    let events_error =
        parse_events(input).expect_err("event parser rejects nested mapping in plain value");
    assert!(
        events_error
            .to_string()
            .contains("mapping values are not allowed")
    );
    assert_eq!(events_error.span().line, 1);
    assert_eq!(events_error.span().column, 5);
}

#[test]
fn yts_reject_8xdj_bf9h_bs4k__comments_do_not_join_plain_scalars() {
    for (name, input) in [
        (
            "8XDJ",
            include_str!("fixtures/yaml-test-suite/data/8XDJ/in.yaml"),
        ),
        (
            "BF9H",
            include_str!("fixtures/yaml-test-suite/data/BF9H/in.yaml"),
        ),
        (
            "BS4K",
            include_str!("fixtures/yaml-test-suite/data/BS4K/in.yaml"),
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("{name} must be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("unexpected content"),
            "{name} error was {error}"
        );

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} events must be rejected"),
            Err(error) => error,
        };
        assert!(
            events_error.to_string().contains("unexpected content"),
            "{name} event error was {events_error}"
        );
    }
}

#[test]
fn yts_reject_g9hc_and_gt5m__invalid_standalone_anchor_sequence_forms() {
    for (name, input) in [
        (
            "G9HC",
            include_str!("fixtures/yaml-test-suite/data/G9HC/in.yaml"),
        ),
        (
            "GT5M",
            include_str!("fixtures/yaml-test-suite/data/GT5M/in.yaml"),
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("{name} must be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("unexpected content"),
            "{name} error was {error}"
        );
        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} events must be rejected"),
            Err(error) => error,
        };
        assert!(
            events_error.to_string().contains("unexpected content"),
            "{name} event error was {events_error}"
        );
    }
}

#[test]
fn reject_extra_flow_commas_in_reduced_sequences_and_mappings() {
    for input in [
        "items: [, one]\n",
        "items: [one,, two]\n",
        "items: {, a: b}\n",
        "items: {a: b,, c: d}\n",
    ] {
        let error = parse_str(input).expect_err("extra flow comma rejected");
        assert!(error.to_string().contains("unexpected comma"));
    }
}

#[test]
fn yts_reject_bad_document_markers_and_directives() {
    for input in ["... trailing\n", "%FOO bar\nkey: value\n"] {
        let error = parse_str(input).expect_err("marker/directive rejected");
        assert!(
            error.to_string().contains("document end markers")
                || error.to_string().contains("unsupported YAML directive")
                || error.to_string().contains("explicit document start")
        );
    }
}

#[test]
fn yts_reject_9hcy_eb22_rhx7_9mma_b63p__invalid_directive_lifecycle() {
    for (name, input, line, message) in [
        (
            "9HCY",
            include_str!("fixtures/yaml-test-suite/data/9HCY/in.yaml"),
            2,
            "directives must appear before the document start marker",
        ),
        (
            "EB22",
            include_str!("fixtures/yaml-test-suite/data/EB22/in.yaml"),
            3,
            "directives must appear before the document start marker",
        ),
        (
            "RHX7",
            include_str!("fixtures/yaml-test-suite/data/RHX7/in.yaml"),
            3,
            "directives must appear before the document start marker",
        ),
        (
            "9MMA",
            include_str!("fixtures/yaml-test-suite/data/9MMA/in.yaml"),
            1,
            "directives must be followed by an explicit document start marker",
        ),
        (
            "B63P",
            include_str!("fixtures/yaml-test-suite/data/B63P/in.yaml"),
            2,
            "directives must be followed by an explicit document start marker",
        ),
    ] {
        let docs_error = match parse_documents(input) {
            Ok(_) => panic!("{name} document parser must reject invalid directive lifecycle"),
            Err(error) => error,
        };
        assert!(
            docs_error.to_string().contains(message),
            "{name} document error was {docs_error}"
        );
        assert_eq!(docs_error.span().line, line, "{name} document line");

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} event parser must reject invalid directive lifecycle"),
            Err(error) => error,
        };
        assert!(
            events_error.to_string().contains(message),
            "{name} event error was {events_error}"
        );
        assert_eq!(events_error.span().line, line, "{name} event line");
    }
}

#[test]
fn yts_reject_9kbc_cxx2__block_mapping_on_document_start_line() {
    for (name, input, column) in [
        (
            "9KBC",
            include_str!("fixtures/yaml-test-suite/data/9KBC/in.yaml"),
            9,
        ),
        (
            "CXX2",
            include_str!("fixtures/yaml-test-suite/data/CXX2/in.yaml"),
            14,
        ),
    ] {
        let error = match parse_documents(input) {
            Ok(_) => panic!("{name} document parser must reject document-start block mapping"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("mapping values are not allowed"),
            "{name} document error was {error}"
        );
        assert_eq!(error.span().line, 1, "{name} document line");
        assert_eq!(error.span().column, column, "{name} document column");

        let events_error = match parse_events(input) {
            Ok(_) => panic!("{name} event parser must reject document-start block mapping"),
            Err(error) => error,
        };
        assert!(
            events_error
                .to_string()
                .contains("mapping values are not allowed"),
            "{name} event error was {events_error}"
        );
        assert_eq!(events_error.span().line, 1, "{name} event line");
        assert_eq!(events_error.span().column, column, "{name} event column");
    }
}

#[test]
fn yts_reject_4jvg__duplicate_anchor_property_on_same_node() {
    for (name, input, line, column) in [
        (
            "4JVG",
            include_str!("fixtures/yaml-test-suite/data/4JVG/in.yaml"),
            4,
            3,
        ),
        ("same-line", "key: &a &b value\n", 1, 9),
    ] {
        let error = parse_documents(input).expect_err("duplicate anchor property rejected");
        assert!(
            error.to_string().contains("duplicate anchor property"),
            "{name} document error was {error}"
        );
        assert_eq!(error.span().line, line, "{name} document line");
        assert_eq!(error.span().column, column, "{name} document column");

        let events_error =
            parse_events(input).expect_err("event parser rejects duplicate anchor property");
        assert!(
            events_error
                .to_string()
                .contains("duplicate anchor property"),
            "{name} event error was {events_error}"
        );
        assert_eq!(events_error.span().line, line, "{name} event line");
        assert_eq!(events_error.span().column, column, "{name} event column");
    }
}

#[test]
fn yts_reject_2g84_and_reduced_malformed_block_scalar_headers() {
    for (name, input, line, column) in [
        (
            "2G84/00",
            include_str!("fixtures/yaml-test-suite/data/2G84-00/in.yaml"),
            1,
            6,
        ),
        (
            "2G84/01",
            include_str!("fixtures/yaml-test-suite/data/2G84-01/in.yaml"),
            1,
            7,
        ),
        ("mapping-alpha", "key: |x\n", 1, 7),
        ("mapping-zero", "key: |0\n", 1, 7),
        ("mapping-chomping", "key: |+-\n", 1, 8),
        ("mapping-space", "key: | x\n", 1, 7),
        ("root-alpha", "|x\n", 1, 2),
        ("sequence-chomping", "- |+-\n", 1, 5),
    ] {
        let error = parse_documents(input).expect_err("malformed block scalar header rejected");
        assert!(
            error.to_string().contains("invalid block scalar header"),
            "{name} document error was {error}"
        );
        assert_eq!(error.span().line, line, "{name} document line");
        assert_eq!(error.span().column, column, "{name} document column");

        let events_error =
            parse_events(input).expect_err("event parser rejects malformed block scalar header");
        assert!(
            events_error
                .to_string()
                .contains("invalid block scalar header"),
            "{name} event error was {events_error}"
        );
        assert_eq!(events_error.span().line, line, "{name} event line");
        assert_eq!(events_error.span().column, column, "{name} event column");
    }
}

#[test]
fn yts_parse_reduced_document_start_inline_nodes_remain_valid() {
    parse_documents("--- scalar\n").expect("document-start plain scalar");
    parse_documents("--- &root scalar\n").expect("document-start anchored scalar");
    parse_documents("--- !Thing scalar\n").expect("document-start tagged scalar");
    parse_documents("--- &root !Thing scalar\n").expect("document-start anchored tagged scalar");
    parse_documents("--- [a: b]\n").expect("document-start flow sequence");
    parse_documents("--- {a: b}\n").expect("document-start flow mapping");
}

#[test]
fn yts_reject_sr86__anchor_plus_alias() {
    let input = include_str!("fixtures/yaml-test-suite/data/SR86/in.yaml");
    let error = parse_str(input).expect_err("anchor plus alias rejected");
    assert!(
        error
            .to_string()
            .contains("alias nodes cannot have anchor or tag properties")
    );
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 10);

    let events_error = parse_events(input).expect_err("event parser rejects anchor plus alias");
    assert!(
        events_error
            .to_string()
            .contains("alias nodes cannot have anchor or tag properties")
    );
}

#[test]
fn yts_reject_unknown_alias() {
    let error = parse_str("value: *missing").expect_err("undefined alias rejected");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 8);
}

#[test]
fn yts_reject_cml9__missing_comma_in_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/CML9/in.yaml");
    let error = parse_str(input).expect_err("flow sequence missing comma rejected");
    assert!(error.to_string().contains("expected `,`"));
    assert_eq!(error.span().line, 3);

    let events_error =
        parse_events(input).expect_err("event parser rejects flow sequence missing comma");
    assert!(events_error.to_string().contains("expected `,`"));
}

#[test]
fn yts_reject_t833__missing_comma_in_flow_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/T833/in.yaml");
    let error = parse_str(input).expect_err("flow mapping missing comma rejected");
    assert!(error.to_string().contains("expected `,`"));
    assert_eq!(error.span().line, 4);
    assert_eq!(error.span().column, 5);

    let events_error =
        parse_events(input).expect_err("event parser rejects flow mapping missing comma");
    assert!(events_error.to_string().contains("expected `,`"));
}

#[test]
fn yts_reject_duplicate_scalar_keys() {
    let error = parse_str("a: 1\na: 2\n").expect_err("duplicate keys rejected");
    assert!(error.to_string().contains("duplicate mapping key"));
    assert_eq!(error.diagnostic().related.len(), 1);
}

#[test]
fn yts_reject_duplicate_resolved_scalar_keys() {
    for input in ["1: a\n1: b\n", "true: a\ntrue: b\n", "null: a\n~: b\n"] {
        let error = parse_str(input).expect_err("duplicate resolved scalar keys rejected");
        assert!(error.to_string().contains("duplicate mapping key"));
        assert_eq!(error.diagnostic().related.len(), 1);
    }
}

#[test]
fn core_schema_keeps_yaml_1_1_bools_as_strings() {
    let doc = parse_str("on: yes\noff: no\ny: n\ntruth: true\ncount: 3\n").expect("parse");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("yes"));
    assert_eq!(entries[1].0.as_str(), Some("off"));
    assert_eq!(entries[1].1.as_str(), Some("no"));
    assert_eq!(entries[2].0.as_str(), Some("y"));
    assert_eq!(entries[2].1.as_str(), Some("n"));
    assert!(matches!(entries[3].1.value, Value::Bool(true)));
    assert!(matches!(
        entries[4].1.value,
        Value::Number(Number::Integer(3))
    ));
}

#[test]
fn core_schema_keeps_timestamps_and_sexagesimal_as_strings() {
    let doc = parse_str("date: 2026-05-24\ntimestamp: 2026-05-24T12:30:00Z\nduration: 1:20\n")
        .expect("parse");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].1.as_str(), Some("2026-05-24"));
    assert_eq!(entries[1].1.as_str(), Some("2026-05-24T12:30:00Z"));
    assert_eq!(entries[2].1.as_str(), Some("1:20"));
}

#[test]
fn yts_parse_block_and_flow_conformance_reductions() {
    for input in [
        "- one\n- two\n- three\n",
        "a: 1\nb:\n  - 2\n  - 3\n",
        "nested: {a: [1, 2, 3], b: {c: d}, e:}\n",
        "literal: |-\n  alpha\n  beta\nfolded: >-\n  alpha\n  beta\n",
        "quoted: \"slash \\/ ok\"\nsingle: 'colon: ok'\nutf8: café\n",
    ] {
        parse_str(input).expect("selected conformance reduction parses");
    }
}

#[test]
fn yts_parse_yaml_double_quoted_escape_set() {
    let input = "space: \"\\ \"\nalarm: \"\\a\"\nescape: \"\\e\"\nvertical: \"\\v\"\nnbsp: \"\\_\"\nnel: \"\\N\"\nline_sep: \"\\L\"\npara_sep: \"\\P\"\nflow: [\"\\e\", \"\\N\", \"\\L\", \"\\P\"]\n";
    let doc = parse_str(input).expect("parse YAML double-quoted escapes");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries[0].1.as_str(), Some(" "));
    assert_eq!(entries[1].1.as_str(), Some("\u{0007}"));
    assert_eq!(entries[2].1.as_str(), Some("\u{001B}"));
    assert_eq!(entries[3].1.as_str(), Some("\u{000B}"));
    assert_eq!(entries[4].1.as_str(), Some("\u{00A0}"));
    assert_eq!(entries[5].1.as_str(), Some("\u{0085}"));
    assert_eq!(entries[6].1.as_str(), Some("\u{2028}"));
    assert_eq!(entries[7].1.as_str(), Some("\u{2029}"));

    let Value::Sequence(flow) = &entries[8].1.value else {
        panic!("expected flow sequence");
    };
    assert_eq!(flow[0].as_str(), Some("\u{001B}"));
    assert_eq!(flow[1].as_str(), Some("\u{0085}"));
    assert_eq!(flow[2].as_str(), Some("\u{2028}"));
    assert_eq!(flow[3].as_str(), Some("\u{2029}"));
}

#[test]
fn yts_parse_flow_sequence_implicit_mapping_entries() {
    let doc = parse_str("root: [a: b, c: d, empty:]\n")
        .expect("parse flow sequence with implicit mappings");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected sequence");
    };
    assert_eq!(items.len(), 3);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first sequence item to be mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("a"));
    assert_eq!(first[0].1.as_str(), Some("b"));

    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second sequence item to be mapping");
    };
    assert_eq!(second[0].0.as_str(), Some("c"));
    assert_eq!(second[0].1.as_str(), Some("d"));

    let Value::Mapping(third) = &items[2].value else {
        panic!("expected third sequence item to be mapping");
    };
    assert_eq!(third[0].0.as_str(), Some("empty"));
    assert!(matches!(third[0].1.value, Value::Null));
}

#[test]
fn yts_parse_9mmw__single_pair_implicit_entries() {
    let input = include_str!("fixtures/yaml-test-suite/data/9MMW/in.yaml");
    let doc = parse_str(input).expect("parse single pair implicit entries");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 3);

    let Value::Sequence(first_sequence) = &items[0].value else {
        panic!("expected first nested sequence");
    };
    let Value::Mapping(first) = &first_sequence[0].value else {
        panic!("expected first implicit mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("YAML"));
    assert_eq!(first[0].1.as_str(), Some("separate"));

    let Value::Sequence(second_sequence) = &items[1].value else {
        panic!("expected second nested sequence");
    };
    let Value::Mapping(second) = &second_sequence[0].value else {
        panic!("expected second implicit mapping");
    };
    assert_eq!(second[0].0.as_str(), Some("JSON like"));
    assert_eq!(second[0].1.as_str(), Some("adjacent"));

    let Value::Sequence(third_sequence) = &items[2].value else {
        panic!("expected third nested sequence");
    };
    let Value::Mapping(third) = &third_sequence[0].value else {
        panic!("expected third implicit mapping");
    };
    let Value::Mapping(third_key) = &third[0].0.value else {
        panic!("expected mapping key");
    };
    assert_eq!(third_key[0].0.as_str(), Some("JSON"));
    assert_eq!(third_key[0].1.as_str(), Some("like"));
    assert_eq!(third[0].1.as_str(), Some("adjacent"));
}

#[test]
fn yts_parse_qf4y__multiline_single_pair_flow_mapping_in_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/QF4Y/in.yaml");
    let doc = parse_str(input).expect("parse multiline single-pair flow mapping");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    let Value::Mapping(entries) = &items[0].value else {
        panic!("expected sequence item mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("foo"));
    assert_eq!(entries[0].1.as_str(), Some("bar"));

    let events = parse_events(input).expect("events for multiline single-pair flow mapping");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(scalars, ["foo", "bar"]);
}

#[test]
fn yts_parse_ct4q__multiline_explicit_key_single_pair_flow_mapping_in_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/CT4Q/in.yaml");
    let doc = parse_str(input).expect("parse multiline explicit-key flow mapping");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 1);
    let Value::Mapping(entries) = &items[0].value else {
        panic!("expected sequence item mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("foo bar"));
    assert_eq!(entries[0].1.as_str(), Some("baz"));

    let events = parse_events(input).expect("events for multiline explicit-key flow mapping");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(scalars, ["foo bar", "baz"]);
}

#[test]
fn yts_parse_c2dt__flow_mapping_adjacent_values() {
    let input = include_str!("fixtures/yaml-test-suite/data/C2DT/in.yaml");
    let doc = parse_str(input).expect("parse flow mapping adjacent values");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.as_str(), Some("adjacent"));
    assert_eq!(entries[0].1.as_str(), Some("value"));
    assert_eq!(entries[1].0.as_str(), Some("readable"));
    assert_eq!(entries[1].1.as_str(), Some("value"));
    assert_eq!(entries[2].0.as_str(), Some("empty"));
    assert!(matches!(entries[2].1.value, Value::Null));
}

#[test]
fn yts_parse_5mud__flow_mapping_adjacent_value_next_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/5MUD/in.yaml");
    let doc = parse_str(input).expect("parse adjacent value on next line");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("foo"));
    assert_eq!(entries[0].1.as_str(), Some("bar"));
}

#[test]
fn yts_parse_5t43__flow_mapping_adjacent_colon_prefixed_scalar() {
    let input = include_str!("fixtures/yaml-test-suite/data/5T43/in.yaml");
    let doc = parse_str(input).expect("parse adjacent colon-prefixed flow scalar");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("key"));
    assert_eq!(first[0].1.as_str(), Some("value"));
    assert_eq!(second[0].0.as_str(), Some("key"));
    assert_eq!(second[0].1.as_str(), Some(":value"));
}

#[test]
fn yts_parse_58mp__flow_mapping_adjacent_colon_prefixed_value() {
    let input = include_str!("fixtures/yaml-test-suite/data/58MP/in.yaml");
    let doc = parse_str(input).expect("parse adjacent colon-prefixed flow value");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected top-level mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("x"));
    assert_eq!(entries[0].1.as_str(), Some(":x"));
}

#[test]
fn yts_parse_8kb6__multiline_plain_flow_mapping_key_without_value() {
    let input = include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml");
    let doc = parse_str(input).expect("parse multiline plain flow key without value");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("single line"));
    assert!(matches!(first[0].1.value, Value::Null));
    assert_eq!(first[1].0.as_str(), Some("a"));
    assert_eq!(first[1].1.as_str(), Some("b"));
    assert_eq!(second[0].0.as_str(), Some("multi line"));
    assert!(matches!(second[0].1.value, Value::Null));
    assert_eq!(second[1].0.as_str(), Some("a"));
    assert_eq!(second[1].1.as_str(), Some("b"));
}

#[test]
fn yts_events_8kb6__fold_multiline_plain_flow_mapping_key() {
    let input = include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml");
    let events = parse_events(input).expect("events for multiline plain flow key");
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(scalars.contains(&"multi line"));
    assert!(!scalars.contains(&"multi\nline"));
}

#[test]
fn yts_parse_7tmg__comment_in_multiline_flow_sequence() {
    let input = include_str!("fixtures/yaml-test-suite/data/7TMG/in.yaml");
    let doc = parse_str(input).expect("parse multiline flow sequence with comment");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_str(), Some("word1"));
    assert_eq!(items[1].as_str(), Some("word2"));
    assert_eq!(items[0].span.line, 2);
    assert_eq!(items[1].span.line, 4);
}

#[test]
fn yts_parse_9sa2__multiline_double_quoted_flow_mapping_key() {
    let input = include_str!("fixtures/yaml-test-suite/data/9SA2/in.yaml");
    let doc = parse_str(input).expect("parse multiline double-quoted flow key");
    let Value::Sequence(items) = doc.value else {
        panic!("expected top-level sequence");
    };
    assert_eq!(items.len(), 2);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("single line"));
    assert_eq!(first[0].1.as_str(), Some("value"));
    assert_eq!(second[0].0.as_str(), Some("multi line"));
    assert_eq!(second[0].1.as_str(), Some("value"));
}

#[test]
fn yts_parse_multiline_flow_collection_reductions() {
    for input in [
        "[ word1\n, word2]\n",
        "[ word1,\n# comment\n  word2]\n",
        "{ a: b\n, c: d }\n",
        "{ a: multi\n  line, c: d }\n",
        "[ a: b\n, c: d ]\n",
    ] {
        parse_str(input).expect("parse multiline flow collection reduction");
    }
}

#[test]
fn yts_parse_flow_mapping_plain_keys_without_values_and_url_keys() {
    let doc = parse_str("root: {a, b: c, http://example.com: value}\n")
        .expect("parse flow mapping plain key edge cases");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root.len(), 3);
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert!(matches!(root[0].1.value, Value::Null));
    assert_eq!(root[1].0.as_str(), Some("b"));
    assert_eq!(root[1].1.as_str(), Some("c"));
    assert_eq!(root[2].0.as_str(), Some("http://example.com"));
    assert_eq!(root[2].1.as_str(), Some("value"));
}

#[test]
fn yts_parse_flow_mapping_explicit_scalar_keys() {
    let doc =
        parse_str("root: {? a: b, ? c, d: e}\n").expect("parse flow mapping explicit scalar keys");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root.len(), 3);
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert_eq!(root[0].1.as_str(), Some("b"));
    assert_eq!(root[1].0.as_str(), Some("c"));
    assert!(matches!(root[1].1.value, Value::Null));
    assert_eq!(root[2].0.as_str(), Some("d"));
    assert_eq!(root[2].1.as_str(), Some("e"));
}

#[test]
fn yts_parse_flow_mapping_collection_keys() {
    let doc = parse_str("root: {? [a, b]: c, ? {d: e}: f}\n")
        .expect("parse flow mapping collection keys");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    let Value::Sequence(first_key) = &root[0].0.value else {
        panic!("expected sequence key");
    };
    assert_eq!(first_key[0].as_str(), Some("a"));
    assert_eq!(first_key[1].as_str(), Some("b"));
    assert_eq!(root[0].1.as_str(), Some("c"));

    let Value::Mapping(second_key) = &root[1].0.value else {
        panic!("expected mapping key");
    };
    assert_eq!(second_key[0].0.as_str(), Some("d"));
    assert_eq!(second_key[0].1.as_str(), Some("e"));
    assert_eq!(root[1].1.as_str(), Some("f"));
}

#[test]
fn yts_reject_x38w__alias_expanded_duplicate_sequence_key() {
    let input = include_str!("fixtures/yaml-test-suite/data/X38W/in.yaml");
    let error = parse_str(input).expect_err("reject alias-expanded duplicate sequence key");
    let display = error.to_string();
    assert!(display.contains("duplicate mapping key"));
    assert!(display.contains("[a, b]"));
    assert!(
        !error.diagnostic().related.is_empty(),
        "duplicate collection key reports the previous key span"
    );
}

#[test]
fn yts_reject_reduced_duplicate_collection_keys() {
    for (name, input, label) in [
        (
            "sequence-key",
            "root: {? [a, b]: first, ? [a, b]: second}\n",
            "[a, b]",
        ),
        (
            "mapping-key",
            "root: {? {x: y}: first, ? {x: y}: second}\n",
            "{x: y}",
        ),
    ] {
        let error = parse_str(input).expect_err("duplicate collection key rejected");
        let display = error.to_string();
        assert!(
            display.contains("duplicate mapping key"),
            "{name} reports duplicate mapping key: {display}"
        );
        assert!(display.contains(label), "{name} label in {display}");
        assert!(
            !error.diagnostic().related.is_empty(),
            "{name} reports previous key span"
        );
        parse_events(input).expect("raw events preserve duplicate collection keys");
    }
}

#[test]
fn yts_events_x38w__flow_key_aliases_remain_raw_events() {
    let input = include_str!("fixtures/yaml-test-suite/data/X38W/in.yaml");
    let events = parse_events(input).expect("events for aliases in flow objects");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "a")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                ..
            } if value == "b"
                && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "b")
        )
    }));

    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some(anchor.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["b", "a", "b"]);
}

#[test]
fn yts_parse_graph_property_tranche() {
    for (name, input) in [
        (
            "3R3P",
            include_str!("fixtures/yaml-test-suite/data/3R3P/in.yaml"),
        ),
        (
            "6KGN",
            include_str!("fixtures/yaml-test-suite/data/6KGN/in.yaml"),
        ),
        (
            "7BMT",
            include_str!("fixtures/yaml-test-suite/data/7BMT/in.yaml"),
        ),
        (
            "7BUB",
            include_str!("fixtures/yaml-test-suite/data/7BUB/in.yaml"),
        ),
        (
            "CN3R",
            include_str!("fixtures/yaml-test-suite/data/CN3R/in.yaml"),
        ),
        (
            "CUP7",
            include_str!("fixtures/yaml-test-suite/data/CUP7/in.yaml"),
        ),
        (
            "E76Z",
            include_str!("fixtures/yaml-test-suite/data/E76Z/in.yaml"),
        ),
        (
            "Y2GN",
            include_str!("fixtures/yaml-test-suite/data/Y2GN/in.yaml"),
        ),
        (
            "ZWK4",
            include_str!("fixtures/yaml-test-suite/data/ZWK4/in.yaml"),
        ),
    ] {
        parse_str(input).unwrap_or_else(|error| panic!("{name} tree parses: {error}"));
        parse_events(input).unwrap_or_else(|error| panic!("{name} events parse: {error}"));
    }
}

#[test]
fn yts_parse_6bfj__mapping_key_and_flow_sequence_item_anchors() {
    let input = include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml");
    let docs = parse_documents(input).expect("parse mapping key and sequence item anchors");
    assert_eq!(docs.len(), 1);
    let Value::Mapping(entries) = &docs[0].value else {
        panic!("expected root mapping");
    };
    assert_eq!(entries.len(), 1);
    let Value::Sequence(key) = &entries[0].0.value else {
        panic!("expected sequence key");
    };
    assert_eq!(key[0].as_str(), Some("a"));
    assert_eq!(key[1].as_str(), Some("b"));
    assert_eq!(key[2].as_str(), Some("c"));
    assert_eq!(entries[0].1.as_str(), Some("value"));
}

#[test]
fn yts_parse_flow_anchor_only_null_nodes() {
    let doc = parse_str("root: [&empty, *empty]\nkeyed: {? &key : value}\n")
        .expect("parse anchor-only flow nodes");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected root mapping");
    };

    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected anchored null sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0].value, Value::Null));
    assert!(matches!(items[1].value, Value::Null));

    let Value::Mapping(keyed) = &entries[1].1.value else {
        panic!("expected keyed mapping");
    };
    assert!(matches!(keyed[0].0.value, Value::Null));
    assert_eq!(keyed[0].1.as_str(), Some("value"));
}

#[test]
fn yts_parse_57h4__block_collection_nodes() {
    let input = include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml");
    let doc = parse_str(input).expect("parse tagged block collection nodes");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected root mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("sequence"));
    let Value::Tagged(sequence) = &entries[0].1.value else {
        panic!("expected tagged sequence");
    };
    assert_eq!(sequence.tag, yaml::Tag::new("!!seq"));
    let Value::Sequence(items) = &sequence.value.value else {
        panic!("expected sequence value");
    };
    assert_eq!(items[0].as_str(), Some("entry"));
    let Value::Tagged(nested) = &items[1].value else {
        panic!("expected nested tagged sequence");
    };
    assert_eq!(nested.tag, yaml::Tag::new("!!seq"));
    let Value::Sequence(nested_items) = &nested.value.value else {
        panic!("expected nested sequence value");
    };
    assert_eq!(nested_items[0].as_str(), Some("nested"));

    assert_eq!(entries[1].0.as_str(), Some("mapping"));
    let Value::Tagged(mapping) = &entries[1].1.value else {
        panic!("expected tagged mapping");
    };
    assert_eq!(mapping.tag, yaml::Tag::new("!!map"));
    let Value::Mapping(mapping_entries) = &mapping.value.value else {
        panic!("expected mapping value");
    };
    assert_eq!(mapping_entries[0].0.as_str(), Some("foo"));
    assert_eq!(mapping_entries[0].1.as_str(), Some("bar"));
}

#[test]
fn yts_events_57h4__block_collection_node_tags() {
    let input = include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml");
    let events = parse_events(input).expect("events for tagged block collection nodes");

    let tagged_sequences = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::SequenceStart { meta, .. }
                    if meta.tag.as_ref().is_some_and(|tag| tag.tag == yaml::Tag::new("!!seq"))
            )
        })
        .count();
    assert_eq!(tagged_sequences, 2);
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.tag.as_ref().is_some_and(|tag| tag.tag == yaml::Tag::new("!!map"))
        )
    }));
}

#[test]
fn yts_events_6bfj__mapping_key_and_flow_sequence_item_anchors() {
    let input = include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml");
    let events = parse_events(input).expect("events for mapping key anchors");

    assert!(matches!(
        events.get(1),
        Some(Event::DocumentStart { explicit: true, .. })
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "mapping")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::SequenceStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "key")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                meta,
                ..
            } if value == "a"
                && meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "item")
        )
    }));
}

#[test]
fn yts_parse_6pbe__zero_indented_sequences_in_explicit_mapping_keys() {
    let input = include_str!("fixtures/yaml-test-suite/data/6PBE/in.yaml");
    let doc = parse_str(input).expect("parse zero-indented explicit sequence key");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 1);
    let Value::Sequence(key) = &entries[0].0.value else {
        panic!("expected sequence key");
    };
    assert_eq!(key[0].as_str(), Some("a"));
    assert_eq!(key[1].as_str(), Some("b"));
    let Value::Sequence(value) = &entries[0].1.value else {
        panic!("expected sequence value");
    };
    assert_eq!(value[0].as_str(), Some("c"));
    assert_eq!(value[1].as_str(), Some("d"));
}

#[test]
fn yts_events_6pbe__zero_indented_sequence_key_and_value() {
    let input = include_str!("fixtures/yaml-test-suite/data/6PBE/in.yaml");
    let events = parse_events(input).expect("events for zero-indented explicit sequence key");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::SequenceStart { .. }))
            .count(),
        2
    );
    let scalars = events
        .iter()
        .filter_map(|event| match event {
            Event::Scalar { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(scalars, ["a", "b", "c", "d"]);
}

#[test]
fn yts_parse_flow_sequence_implicit_mapping_explicit_and_collection_keys() {
    let doc = parse_str("root: [? a: b, ? [c, d]: e, [f, g]: h]\n")
        .expect("parse flow sequence implicit mapping keys");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected root sequence");
    };
    assert_eq!(items.len(), 3);

    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("a"));
    assert_eq!(first[0].1.as_str(), Some("b"));

    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    let Value::Sequence(second_key) = &second[0].0.value else {
        panic!("expected second key sequence");
    };
    assert_eq!(second_key[0].as_str(), Some("c"));
    assert_eq!(second_key[1].as_str(), Some("d"));
    assert_eq!(second[0].1.as_str(), Some("e"));

    let Value::Mapping(third) = &items[2].value else {
        panic!("expected third item mapping");
    };
    let Value::Sequence(third_key) = &third[0].0.value else {
        panic!("expected third key sequence");
    };
    assert_eq!(third_key[0].as_str(), Some("f"));
    assert_eq!(third_key[1].as_str(), Some("g"));
    assert_eq!(third[0].1.as_str(), Some("h"));
}

#[test]
fn yts_parse_a984__multiline_scalar_in_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/A984/in.yaml");
    let doc = parse_str(input).expect("parse multiline plain scalars");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("a"));
    assert_eq!(entries[0].1.as_str(), Some("b c"));
    assert_eq!(entries[1].0.as_str(), Some("d"));
    assert_eq!(entries[1].1.as_str(), Some("e f"));
}

#[test]
fn yts_parse_36f6__multiline_plain_scalar_with_empty_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/36F6/in.yaml");
    let doc = parse_str(input).expect("parse multiline plain scalar with empty line");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.as_str(), Some("plain"));
    assert_eq!(entries[0].1.as_str(), Some("a b\nc"));
}

#[test]
fn yts_parse_5gbf__empty_lines_in_flow_and_block_scalars() {
    let input = include_str!("fixtures/yaml-test-suite/data/5GBF/in.yaml");
    let doc = parse_str(input).expect("parse empty lines in flow and block scalars");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("Folding"));
    assert_eq!(entries[0].1.as_str(), Some("Empty line\nas a line feed"));
    assert_eq!(entries[1].0.as_str(), Some("Chomping"));
    assert_eq!(entries[1].1.as_str(), Some("Clipped empty lines\n"));
}

#[test]
fn yts_parse_4cqq__multi_line_flow_scalars() {
    let input = include_str!("fixtures/yaml-test-suite/data/4CQQ/in.yaml");
    let doc = parse_str(input).expect("parse multi-line flow scalars");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].1.as_str(),
        Some("This unquoted scalar spans many lines.")
    );
    assert_eq!(entries[1].1.as_str(), Some("So does this quoted scalar.\n"));
}

#[test]
fn yts_parse_p2ad__block_scalar_header() {
    let input = include_str!("fixtures/yaml-test-suite/data/P2AD/in.yaml");
    let doc = parse_str(input).expect("parse block scalar headers");
    let Value::Sequence(items) = doc.value else {
        panic!("expected sequence");
    };
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].as_str(), Some("literal\n"));
    assert_eq!(items[1].as_str(), Some(" folded\n"));
    assert_eq!(items[2].as_str(), Some("keep\n\n"));
    assert_eq!(items[3].as_str(), Some(" strip"));
}

#[test]
fn yts_parse_f8f9__block_scalar_chomping_trailing_lines() {
    let input = include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml");
    let doc = parse_str(input).expect("parse block scalar chomping trailing lines");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.as_str(), Some("strip"));
    assert_eq!(entries[0].1.as_str(), Some("# text"));
    assert_eq!(entries[1].0.as_str(), Some("clip"));
    assert_eq!(entries[1].1.as_str(), Some("# text\n"));
    assert_eq!(entries[2].0.as_str(), Some("keep"));
    assert_eq!(entries[2].1.as_str(), Some("# text\n\n"));
}

#[test]
fn yts_parse_k858__empty_block_scalar_chomping() {
    let input = include_str!("fixtures/yaml-test-suite/data/K858/in.yaml");
    let doc = parse_str(input).expect("parse empty block scalar chomping");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.as_str(), Some("strip"));
    assert_eq!(entries[0].1.as_str(), Some(""));
    assert_eq!(entries[1].0.as_str(), Some("clip"));
    assert_eq!(entries[1].1.as_str(), Some(""));
    assert_eq!(entries[2].0.as_str(), Some("keep"));
    assert_eq!(entries[2].1.as_str(), Some("\n"));
}

#[test]
fn yts_parse_6vjk__folded_block_preserves_blank_paragraphs_and_more_indented_lines() {
    let input = include_str!("fixtures/yaml-test-suite/data/6VJK/in.yaml");
    let doc = parse_str(input).expect("parse folded block scalar with paragraphs");
    assert_eq!(
        doc.as_str(),
        Some(
            "Sammy Sosa completed another fine season with great stats.\n\n  63 Home Runs\n  0.288 Batting Average\n\nWhat a year!\n"
        )
    );
}

#[test]
fn yts_parse_6fwr__literal_keep_preserves_spaces_only_line() {
    let input = include_str!("fixtures/yaml-test-suite/data/6FWR/in.yaml");
    let doc = parse_str(input).expect("parse literal block scalar with spaces-only line");
    assert_eq!(doc.as_str(), Some("ab\n\n \n"));
}

#[test]
fn yts_parse_folded_block_blank_runs_match_upstream_values() {
    for (id, input, expected) in [
        (
            "4Q9F",
            include_str!("fixtures/yaml-test-suite/data/4Q9F/in.yaml"),
            "ab cd\nef\n\ngh\n",
        ),
        (
            "TS54",
            include_str!("fixtures/yaml-test-suite/data/TS54/in.yaml"),
            "ab cd\nef\n\ngh\n",
        ),
        (
            "7T8X",
            include_str!("fixtures/yaml-test-suite/data/7T8X/in.yaml"),
            "\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n",
        ),
        (
            "93WF",
            include_str!("fixtures/yaml-test-suite/data/93WF/in.yaml"),
            "trimmed\n\n\nas space",
        ),
        (
            "K527",
            include_str!("fixtures/yaml-test-suite/data/K527/in.yaml"),
            "trimmed\n\n\nas space",
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("parse {id}: {error}"));
        assert_eq!(doc.as_str(), Some(expected), "{id}");
    }
}

#[test]
fn yts_parse_r4yg__block_scalar_detected_indentation_values() {
    let input = include_str!("fixtures/yaml-test-suite/data/R4YG/in.yaml");
    let doc = parse_str(input).expect("parse detected block scalar indentation");
    let Value::Sequence(items) = doc.value else {
        panic!("expected sequence");
    };
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].as_str(), Some("detected\n"));
    assert_eq!(items[1].as_str(), Some("\n\n# detected\n"));
    assert_eq!(items[2].as_str(), Some(" explicit\n"));
    assert_eq!(items[3].as_str(), Some("\t\ndetected\n"));
}

#[test]
fn yts_parse_f6mc__folded_block_more_indented_leading_lines() {
    let input = include_str!("fixtures/yaml-test-suite/data/F6MC/in.yaml");
    let doc = parse_str(input).expect("parse folded scalar with more-indented lines");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("a"));
    assert_eq!(entries[0].1.as_str(), Some(" more indented\nregular\n"));
    assert_eq!(entries[1].0.as_str(), Some("b"));
    assert_eq!(entries[1].1.as_str(), Some("\n\n more indented\nregular\n"));
}

#[test]
fn yts_parse_m5c3__block_scalar_nodes() {
    let input = include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml");
    let doc = parse_str(input).expect("parse block scalar nodes");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("literal"));
    assert_eq!(entries[0].1.as_str(), Some("value\n"));
    assert_eq!(entries[1].0.as_str(), Some("folded"));
    let Value::Tagged(tagged) = &entries[1].1.value else {
        panic!("expected tagged folded scalar");
    };
    assert_eq!(tagged.tag, yaml::Tag::new("foo"));
    assert_eq!(tagged.value.as_str(), Some("value\n"));
}

#[test]
fn yts_events_m5c3__separated_tag_on_block_scalar_spans() {
    let input = include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml");
    let events = parse_events(input).expect("events");

    let folded = events
        .iter()
        .find_map(|event| match event {
            Event::Scalar {
                value,
                style: ScalarStyle::Folded,
                meta,
                span,
            } if value == "value\n" => Some((meta, *span)),
            _ => None,
        })
        .expect("folded scalar");

    let tag = folded.0.tag.as_ref().expect("folded scalar tag");
    assert_eq!(tag.tag, yaml::Tag::new("foo"));
    assert_eq!((tag.span.line, tag.span.column), (4, 4));
    assert_eq!(event_source(input, tag.span), "!foo");
    assert_eq!((folded.1.line, folded.1.column), (5, 3));
    assert_eq!(event_source(input, folded.1), ">1\n value");
}

#[test]
fn yts_events_reduced_verbatim_tag_span_points_at_tag_token() {
    let input = "--- !<tag:example.com,2026:Thing> value\n";
    let events = parse_events(input).expect("events");
    let Some(Event::DocumentStart { span, .. }) = events.get(1) else {
        panic!("expected document start");
    };
    assert_eq!((span.line, span.column), (1, 1));
    assert_eq!(event_source(input, *span), "---");

    let Some(Event::Scalar {
        value, meta, span, ..
    }) = events
        .iter()
        .find(|event| matches!(event, Event::Scalar { value, .. } if value == "value"))
    else {
        panic!("expected tagged scalar");
    };
    assert_eq!(value, "value");
    assert_eq!(event_source(input, *span), "value");
    let tag = meta.tag.as_ref().expect("verbatim tag");
    assert_eq!(tag.tag.suffix, "tag:example.com,2026:Thing");
    assert_eq!((tag.span.line, tag.span.column), (1, 5));
    assert_eq!(
        event_source(input, tag.span),
        "!<tag:example.com,2026:Thing>"
    );
}

#[test]
fn yts_parse_block_scalar_blank_and_comment_content() {
    let doc = parse_str("body: |-\n\n  # content, not YAML comment\n  line\nnext: ok\n")
        .expect("parse block scalar with data comment");
    let Value::Mapping(entries) = doc.value else {
        panic!("expected mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("body"));
    assert_eq!(
        entries[0].1.as_str(),
        Some("\n# content, not YAML comment\nline")
    );
    assert_eq!(entries[1].0.as_str(), Some("next"));
    assert_eq!(entries[1].1.as_str(), Some("ok"));
}
