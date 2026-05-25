use saphyr::LoadableYamlNode;

#[derive(Clone, Copy)]
enum TagPolicy {
    Preserve,
    Strip,
}

#[derive(Debug, PartialEq, Eq)]
enum NormTree {
    Null,
    Bool(bool),
    Int(i128),
    Float(String),
    String(String),
    Seq(Vec<NormTree>),
    Map(Vec<(NormTree, NormTree)>),
    Tagged(String, Box<NormTree>),
    Alias(usize),
    BadValue,
}

struct TreeCase {
    name: &'static str,
    input: &'static str,
}

const VALUE_SHAPE_CASES: &[TreeCase] = &[
    TreeCase {
        name: "core_scalars",
        input: "nulls: [null, ~]\nbools: [true, false]\nstrings: [\"true\", \"001\", \"2026-05-23\"]\nints: [0, 42, -7]\nfloats: [3.14, -0.5]\n",
    },
    TreeCase {
        name: "anchor_redefinition_last_wins",
        input: "a: &x 1\nb: &x {n: 2}\nc: *x\n",
    },
    TreeCase {
        name: "literal_merge_key_alias_value",
        input: "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
    },
    TreeCase {
        name: "literal_merge_list_alias_values",
        input: "base1: &base1 {a: 1, b: 1, shared: first}\nbase2: &base2 {b: 2, c: 2, shared: second}\nmerged:\n  <<: [*base1, *base2]\n  b: explicit\n",
    },
    TreeCase {
        name: "flow_mapping_key_metadata",
        input: "key: &key alias-key\nroot: {&direct direct-key: v, ? *key : alias-v, ? &seq [a, b] : seq-v, !Thing tagged-key: tagged-v}\n",
    },
    TreeCase {
        name: "yts_qf4y_multiline_single_pair_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/QF4Y/in.yaml"),
    },
    TreeCase {
        name: "yts_6vjk_folded_block_scalar_paragraphs",
        input: include_str!("fixtures/yaml-test-suite/data/6VJK/in.yaml"),
    },
    TreeCase {
        name: "yts_6fwr_literal_block_scalar_spaces_only_line",
        input: include_str!("fixtures/yaml-test-suite/data/6FWR/in.yaml"),
    },
    TreeCase {
        name: "yts_4q9f_folded_block_scalar_empty_lines_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/4Q9F/in.yaml"),
    },
    TreeCase {
        name: "yts_ts54_folded_block_scalar_empty_lines",
        input: include_str!("fixtures/yaml-test-suite/data/TS54/in.yaml"),
    },
    TreeCase {
        name: "yts_7t8x_folded_block_scalar_list_like_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/7T8X/in.yaml"),
    },
    TreeCase {
        name: "yts_93wf_folded_block_scalar_strip_spaces_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/93WF/in.yaml"),
    },
    TreeCase {
        name: "yts_k527_folded_block_scalar_strip_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/K527/in.yaml"),
    },
    TreeCase {
        name: "yts_r4yg_block_scalar_detected_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/R4YG/in.yaml"),
    },
    TreeCase {
        name: "yts_ct4q_multiline_explicit_key_single_pair_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/CT4Q/in.yaml"),
    },
    TreeCase {
        name: "yts_6pbe_zero_indented_explicit_sequence_key",
        input: include_str!("fixtures/yaml-test-suite/data/6PBE/in.yaml"),
    },
    TreeCase {
        name: "yts_ske5_anchor_before_zero_indented_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/SKE5/in.yaml"),
    },
    TreeCase {
        name: "yts_9sa2_multiline_double_quoted_flow_key",
        input: include_str!("fixtures/yaml-test-suite/data/9SA2/in.yaml"),
    },
    TreeCase {
        name: "yts_c2dt_flow_mapping_adjacent_values",
        input: include_str!("fixtures/yaml-test-suite/data/C2DT/in.yaml"),
    },
    TreeCase {
        name: "yts_5mud_adjacent_flow_value_next_line",
        input: include_str!("fixtures/yaml-test-suite/data/5MUD/in.yaml"),
    },
    TreeCase {
        name: "yts_5t43_adjacent_colon_prefixed_flow_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/5T43/in.yaml"),
    },
    TreeCase {
        name: "yts_58mp_adjacent_colon_prefixed_flow_value",
        input: include_str!("fixtures/yaml-test-suite/data/58MP/in.yaml"),
    },
    TreeCase {
        name: "yts_w4tn_zero_indented_literal_after_document_start",
        input: include_str!("fixtures/yaml-test-suite/data/W4TN/in.yaml"),
    },
    TreeCase {
        name: "yts_ut92_directive_looking_flow_content",
        input: include_str!("fixtures/yaml-test-suite/data/UT92/in.yaml"),
    },
    TreeCase {
        name: "yts_fp8r_zero_indented_folded_after_document_start",
        input: include_str!("fixtures/yaml-test-suite/data/FP8R/in.yaml"),
    },
    TreeCase {
        name: "yts_dk3j_zero_indented_folded_comment_like_line",
        input: include_str!("fixtures/yaml-test-suite/data/DK3J/in.yaml"),
    },
    TreeCase {
        name: "yts_57h4_tagged_block_collections",
        input: include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml"),
    },
];

const TAGGED_SAPHYR_CASES: &[TreeCase] = &[TreeCase {
    name: "custom_tagged_flow_mapping_key",
    input: "root: {!Thing tagged-key: tagged-v, value: !Other scalar}\n",
}];

const REAL_WORLD_TREE_CASES: &[TreeCase] = &[
    TreeCase {
        name: "github_actions_matrix",
        input: include_str!("fixtures/real-world/github-actions/matrix-ci.yaml"),
    },
    TreeCase {
        name: "github_actions_minimal",
        input: include_str!("fixtures/real-world/github-actions/minimal-ci.yaml"),
    },
    TreeCase {
        name: "github_actions_polymorphic",
        input: include_str!("fixtures/real-world/github-actions/polymorphic-workflow.yaml"),
    },
    TreeCase {
        name: "docker_compose",
        input: include_str!("fixtures/real-world/docker-compose/compose.yaml"),
    },
    TreeCase {
        name: "docker_compose_anchors",
        input: include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml"),
    },
    TreeCase {
        name: "docker_compose_polymorphic",
        input: include_str!("fixtures/real-world/docker-compose/compose-polymorphic.yaml"),
    },
    TreeCase {
        name: "docker_compose_platform_resources",
        input: include_str!("fixtures/real-world/docker-compose/compose-platform-resources.yaml"),
    },
    TreeCase {
        name: "kubernetes_multidoc",
        input: include_str!("fixtures/real-world/kubernetes/multi-doc.yaml"),
    },
    TreeCase {
        name: "kubernetes_deployment",
        input: include_str!("fixtures/real-world/kubernetes/deployment.yaml"),
    },
    TreeCase {
        name: "kubernetes_configmap_block_scalars",
        input: include_str!("fixtures/real-world/kubernetes/configmap-block-scalars.yaml"),
    },
    TreeCase {
        name: "kubernetes_helm_rendered_stream",
        input: include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml"),
    },
    TreeCase {
        name: "kubernetes_crd_openapi_stream",
        input: include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml"),
    },
    TreeCase {
        name: "helm_values",
        input: include_str!("fixtures/real-world/helm/values.yaml"),
    },
    TreeCase {
        name: "helm_chart_metadata",
        input: include_str!("fixtures/real-world/helm/Chart.yaml"),
    },
    TreeCase {
        name: "openapi_petstore_fragment",
        input: include_str!("fixtures/real-world/openapi/petstore-fragment.yaml"),
    },
    TreeCase {
        name: "openapi_operations_and_polymorphism",
        input: include_str!("fixtures/real-world/openapi/operations-and-polymorphism.yaml"),
    },
    TreeCase {
        name: "wrangler_yaml",
        input: include_str!("fixtures/real-world/cloudflare/wrangler.yaml"),
    },
    TreeCase {
        name: "ansible_playbook",
        input: include_str!("fixtures/real-world/ansible/playbook.yaml"),
    },
    TreeCase {
        name: "ansible_vault_and_unsafe_tags",
        input: include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml"),
    },
];

#[test]
fn loaded_tree_value_shape_matches_yaml_rust2_and_saphyr_for_selected_cases() {
    for case in VALUE_SHAPE_CASES {
        let ours = normalize_ours_documents(case.input, TagPolicy::Strip)
            .unwrap_or_else(|error| panic!("ours tree failed {}: {error}", case.name));
        let yaml_rust2 = normalize_yaml_rust2_documents(case.input)
            .unwrap_or_else(|error| panic!("yaml-rust2 tree failed {}: {error}", case.name));
        let saphyr = normalize_saphyr_documents(case.input, TagPolicy::Strip)
            .unwrap_or_else(|error| panic!("saphyr tree failed {}: {error}", case.name));

        assert_eq!(
            ours, yaml_rust2,
            "normalized yaml-rust2 loaded-tree parity for {}",
            case.name
        );
        assert_eq!(
            ours, saphyr,
            "normalized saphyr loaded-tree parity for {}",
            case.name
        );
    }
}

#[test]
fn loaded_tree_value_shape_matches_references_for_real_world_configs() {
    for case in REAL_WORLD_TREE_CASES {
        let ours = normalize_ours_documents(case.input, TagPolicy::Strip)
            .unwrap_or_else(|error| panic!("ours tree failed {}: {error}", case.name));
        let yaml_rust2 = normalize_yaml_rust2_documents(case.input)
            .unwrap_or_else(|error| panic!("yaml-rust2 tree failed {}: {error}", case.name));
        let saphyr = normalize_saphyr_documents(case.input, TagPolicy::Strip)
            .unwrap_or_else(|error| panic!("saphyr tree failed {}: {error}", case.name));

        assert_eq!(
            ours, yaml_rust2,
            "normalized yaml-rust2 real-world loaded-tree parity for {}",
            case.name
        );
        assert_eq!(
            ours, saphyr,
            "normalized saphyr real-world loaded-tree parity for {}",
            case.name
        );
    }
}

#[test]
fn custom_tagged_tree_shape_matches_saphyr_when_tags_are_preserved() {
    for case in TAGGED_SAPHYR_CASES {
        let ours = normalize_ours_documents(case.input, TagPolicy::Preserve)
            .unwrap_or_else(|error| panic!("ours tree failed {}: {error}", case.name));
        let saphyr = normalize_saphyr_documents(case.input, TagPolicy::Preserve)
            .unwrap_or_else(|error| panic!("saphyr tree failed {}: {error}", case.name));

        assert_eq!(
            ours, saphyr,
            "normalized saphyr tagged loaded-tree parity for {}",
            case.name
        );
    }
}

fn normalize_ours_documents(input: &str, tags: TagPolicy) -> yaml::Result<Vec<NormTree>> {
    yaml::parse_documents(input).map(|docs| {
        docs.iter()
            .map(|document| normalize_ours_node(document, tags))
            .collect()
    })
}

fn normalize_yaml_rust2_documents(input: &str) -> Result<Vec<NormTree>, yaml_rust2::ScanError> {
    yaml_rust2::YamlLoader::load_from_str(input).map(|docs| {
        docs.iter()
            .map(normalize_yaml_rust2_node)
            .collect::<Vec<_>>()
    })
}

fn normalize_saphyr_documents(
    input: &str,
    tags: TagPolicy,
) -> Result<Vec<NormTree>, saphyr::ScanError> {
    saphyr::Yaml::load_from_str(input).map(|docs| {
        docs.iter()
            .map(|document| normalize_saphyr_node(document, tags))
            .collect()
    })
}

fn normalize_ours_node(node: &yaml::Node, tags: TagPolicy) -> NormTree {
    match &node.value {
        yaml::NodeValue::Null => NormTree::Null,
        yaml::NodeValue::Bool(value) => NormTree::Bool(*value),
        yaml::NodeValue::Number(number) => normalize_ours_number(*number),
        yaml::NodeValue::String(value) => NormTree::String(value.clone()),
        yaml::NodeValue::Sequence(items) => NormTree::Seq(
            items
                .iter()
                .map(|item| normalize_ours_node(item, tags))
                .collect(),
        ),
        yaml::NodeValue::Mapping(entries) => NormTree::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        normalize_ours_node(key, tags),
                        normalize_ours_node(value, tags),
                    )
                })
                .collect(),
        ),
        yaml::NodeValue::Tagged(tagged) => match tags {
            TagPolicy::Preserve => NormTree::Tagged(
                normalize_tag(&tagged.tag.to_string()),
                Box::new(normalize_ours_node(&tagged.value, tags)),
            ),
            TagPolicy::Strip => normalize_ours_node(&tagged.value, tags),
        },
    }
}

fn normalize_ours_number(number: yaml::Number) -> NormTree {
    match number {
        yaml::Number::Integer(value) => NormTree::Int(value),
        yaml::Number::Unsigned(value) => {
            NormTree::Int(i128::try_from(value).expect("selected parity integer fits i128"))
        }
        yaml::Number::Float(value) => NormTree::Float(normalize_float(value)),
    }
}

fn normalize_yaml_rust2_node(node: &yaml_rust2::Yaml) -> NormTree {
    match node {
        yaml_rust2::Yaml::Real(value) => normalize_float_text(value),
        yaml_rust2::Yaml::Integer(value) => NormTree::Int(i128::from(*value)),
        yaml_rust2::Yaml::String(value) => NormTree::String(value.clone()),
        yaml_rust2::Yaml::Boolean(value) => NormTree::Bool(*value),
        yaml_rust2::Yaml::Array(items) => {
            NormTree::Seq(items.iter().map(normalize_yaml_rust2_node).collect())
        }
        yaml_rust2::Yaml::Hash(entries) => NormTree::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        normalize_yaml_rust2_node(key),
                        normalize_yaml_rust2_node(value),
                    )
                })
                .collect(),
        ),
        yaml_rust2::Yaml::Alias(id) => NormTree::Alias(*id),
        yaml_rust2::Yaml::Null => NormTree::Null,
        yaml_rust2::Yaml::BadValue => NormTree::BadValue,
    }
}

fn normalize_saphyr_node(node: &saphyr::Yaml<'_>, tags: TagPolicy) -> NormTree {
    match node {
        saphyr::Yaml::Representation(value, _, tag) => {
            let value = NormTree::String(value.to_string());
            match (tags, tag) {
                (TagPolicy::Preserve, Some(tag)) => {
                    NormTree::Tagged(normalize_tag(&tag.to_string()), Box::new(value))
                }
                _ => value,
            }
        }
        saphyr::Yaml::Value(value) => normalize_saphyr_scalar(value),
        saphyr::Yaml::Sequence(items) => NormTree::Seq(
            items
                .iter()
                .map(|item| normalize_saphyr_node(item, tags))
                .collect(),
        ),
        saphyr::Yaml::Mapping(entries) => NormTree::Map(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        normalize_saphyr_node(key, tags),
                        normalize_saphyr_node(value, tags),
                    )
                })
                .collect(),
        ),
        saphyr::Yaml::Tagged(tag, value) => match tags {
            TagPolicy::Preserve => NormTree::Tagged(
                normalize_tag(&tag.to_string()),
                Box::new(normalize_saphyr_node(value, tags)),
            ),
            TagPolicy::Strip => normalize_saphyr_node(value, tags),
        },
        saphyr::Yaml::Alias(id) => NormTree::Alias(*id),
        saphyr::Yaml::BadValue => NormTree::BadValue,
    }
}

fn normalize_saphyr_scalar(scalar: &saphyr::Scalar<'_>) -> NormTree {
    match scalar {
        saphyr::Scalar::Null => NormTree::Null,
        saphyr::Scalar::Boolean(value) => NormTree::Bool(*value),
        saphyr::Scalar::Integer(value) => NormTree::Int(i128::from(*value)),
        saphyr::Scalar::FloatingPoint(value) => NormTree::Float(normalize_float(value.0)),
        saphyr::Scalar::String(value) => NormTree::String(value.to_string()),
    }
}

fn normalize_float_text(value: &str) -> NormTree {
    value
        .parse::<f64>()
        .map(|value| NormTree::Float(normalize_float(value)))
        .unwrap_or_else(|_| NormTree::Float(value.to_string()))
}

fn normalize_float(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_string()
    } else if value == f64::INFINITY {
        ".inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-.inf".to_string()
    } else {
        value.to_string()
    }
}

fn normalize_tag(tag: &str) -> String {
    tag.strip_prefix("tag:yaml.org,2002:!")
        .map(|suffix| format!("!!{suffix}"))
        .unwrap_or_else(|| tag.to_string())
}
