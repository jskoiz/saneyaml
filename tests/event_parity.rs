use saphyr::LoadableYamlNode;
use std::collections::BTreeMap;
use yaml::{CollectionStyle, Event, ScalarStyle};
use yaml_rust2::parser::MarkedEventReceiver;

#[derive(Clone, Debug, PartialEq, Eq)]
enum NormEvent {
    StreamStart,
    StreamEnd,
    DocumentStart {
        explicit: Option<bool>,
    },
    DocumentEnd,
    SequenceStart {
        style: NormCollectionStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
    SequenceEnd,
    MappingStart {
        style: NormCollectionStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
    MappingEnd,
    Alias {
        anchor: String,
    },
    Scalar {
        value: String,
        style: NormStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NormStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NormCollectionStyle {
    Block,
    Flow,
}

#[derive(Clone, Copy)]
enum DocumentStartMode {
    Preserve,
    Strip,
}

#[derive(Default)]
struct AnchorNormalizer {
    next: usize,
    names: BTreeMap<String, String>,
    ids: BTreeMap<usize, String>,
}

impl AnchorNormalizer {
    fn reset(&mut self) {
        self.next = 0;
        self.names.clear();
        self.ids.clear();
    }

    fn define_name(&mut self, name: &str) -> String {
        let normalized = self.next_anchor();
        self.names.insert(name.to_string(), normalized.clone());
        normalized
    }

    fn define_id(&mut self, id: usize) -> Option<String> {
        if id == 0 {
            return None;
        }
        let normalized = self.next_anchor();
        self.ids.insert(id, normalized.clone());
        Some(normalized)
    }

    fn alias_name(&self, name: &str) -> String {
        self.names
            .get(name)
            .cloned()
            .unwrap_or_else(|| format!("unknown:{name}"))
    }

    fn alias_id(&self, id: usize) -> String {
        self.ids
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("unknown:{id}"))
    }

    fn next_anchor(&mut self) -> String {
        self.next += 1;
        format!("a{}", self.next)
    }
}

struct YamlRust2Sink {
    events: Vec<(yaml_rust2::parser::Event, yaml_rust2::scanner::Marker)>,
}

impl MarkedEventReceiver for YamlRust2Sink {
    fn on_event(&mut self, event: yaml_rust2::parser::Event, marker: yaml_rust2::scanner::Marker) {
        self.events.push((event, marker));
    }
}

struct Case {
    name: &'static str,
    input: &'static str,
    docs: usize,
}

const CASES: &[Case] = &[
    Case {
        name: "basic_block_tree",
        input: "root:\n  items:\n    - one\n    - two\n  enabled: true\n",
        docs: 1,
    },
    Case {
        name: "scalar_styles",
        input: "- plain\n- 'single'\n- \"double\"\n- |-\n  literal\n- >-\n  folded\n",
        docs: 1,
    },
    Case {
        name: "anchors_aliases",
        input: "root: &root\n  child: 1\nref: *root\n",
        docs: 1,
    },
    Case {
        name: "flow_key_metadata",
        input: "%TAG !e! tag:example.com,2026:\n---\nscalar: &scalar scalar-key\nroot: {&direct direct-key: v, ? *scalar : alias-v, ? &seq [a, b] : seq-v, !e!Thing tagged-key: tagged-v}\n",
        docs: 1,
    },
    Case {
        name: "yts_9kax_tag_anchor_property_combinations",
        input: include_str!("fixtures/yaml-test-suite/data/9KAX/in.yaml"),
        docs: 8,
    },
    Case {
        name: "multidoc_boundaries",
        input: "---\nkind: first\n---\nkind: second\n...\n",
        docs: 2,
    },
    Case {
        name: "yts_5we3_explicit_block_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/5WE3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_v9d5_compact_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/V9D5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9mmw_single_pair_implicit_entries",
        input: include_str!("fixtures/yaml-test-suite/data/9MMW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6vjk_folded_block_scalar_paragraphs",
        input: include_str!("fixtures/yaml-test-suite/data/6VJK/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6fwr_literal_block_scalar_spaces_only_line",
        input: include_str!("fixtures/yaml-test-suite/data/6FWR/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_k858_empty_block_scalar_chomping",
        input: include_str!("fixtures/yaml-test-suite/data/K858/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4q9f_folded_block_scalar_empty_lines_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/4Q9F/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ts54_folded_block_scalar_empty_lines",
        input: include_str!("fixtures/yaml-test-suite/data/TS54/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7t8x_folded_block_scalar_list_like_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/7T8X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_93wf_folded_block_scalar_strip_spaces_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/93WF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_k527_folded_block_scalar_strip_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/K527/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_r4yg_block_scalar_detected_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/R4YG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_qf4y_multiline_single_pair_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/QF4Y/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ct4q_multiline_explicit_key_single_pair_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/CT4Q/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_c2dt_flow_mapping_adjacent_values",
        input: include_str!("fixtures/yaml-test-suite/data/C2DT/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5mud_adjacent_flow_value_next_line",
        input: include_str!("fixtures/yaml-test-suite/data/5MUD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5t43_adjacent_colon_prefixed_flow_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/5T43/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_58mp_adjacent_colon_prefixed_flow_value",
        input: include_str!("fixtures/yaml-test-suite/data/58MP/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_u3c3_tag_directive_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/U3C3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mzx3_scalar_styles",
        input: include_str!("fixtures/yaml-test-suite/data/MZX3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6m2f_aliases_in_explicit_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/6M2F/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_bu8l_collection_anchor_and_tag",
        input: include_str!("fixtures/yaml-test-suite/data/BU8L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_w4tn_yaml_directive_and_boundaries",
        input: include_str!("fixtures/yaml-test-suite/data/W4TN/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_ut92_directive_looking_flow_content",
        input: include_str!("fixtures/yaml-test-suite/data/UT92/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_fp8r_zero_indented_folded_scalar_after_document_start",
        input: include_str!("fixtures/yaml-test-suite/data/FP8R/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk3j_zero_indented_folded_scalar_comment_like_line",
        input: include_str!("fixtures/yaml-test-suite/data/DK3J/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m7a3_bare_documents",
        input: include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_fta2_document_start_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/FTA2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7tmg_flow_sequence_comments",
        input: include_str!("fixtures/yaml-test-suite/data/7TMG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8kb6_multiline_flow_plain_key",
        input: include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m5c3_block_scalar_tags",
        input: include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6bfj_flow_key_metadata",
        input: include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6pbe_zero_indented_explicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/6PBE/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ske5_anchor_before_zero_indented_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/SKE5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_pw8x_anchors_on_empty_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_57h4_tagged_block_collections",
        input: include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dhp8_flow_sequence_and_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/DHP8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7w2p_block_mapping_missing_values",
        input: include_str!("fixtures/yaml-test-suite/data/7W2P/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_s3pd_implicit_block_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/S3PD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_cfd4_empty_implicit_flow_sequence_keys",
        input: include_str!("fixtures/yaml-test-suite/data/CFD4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m2n8_00_question_mark_edge_empty_compact_mapping_key",
        input: include_str!("fixtures/yaml-test-suite/data/M2N8-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ukk6_00_colon_only_compact_sequence_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ukk6_02_bare_explicit_non_specific_tag",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2ebw_allowed_plain_key_characters",
        input: include_str!("fixtures/yaml-test-suite/data/2EBW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_fbc9_allowed_plain_scalar_characters",
        input: include_str!("fixtures/yaml-test-suite/data/FBC9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_xlq9_directive_looking_plain_scalar_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/XLQ9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ab8u_sequence_entry_looking_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/AB8U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3gzx_alias_nodes",
        input: include_str!("fixtures/yaml-test-suite/data/3GZX/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_u3xv_node_and_mapping_key_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/U3XV/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2sxe_anchors_with_colon_in_name",
        input: include_str!("fixtures/yaml-test-suite/data/2SXE/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jhb9_two_documents_with_comments",
        input: include_str!("fixtures/yaml-test-suite/data/JHB9/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_6lvf_reserved_directive_is_ignored",
        input: include_str!("fixtures/yaml-test-suite/data/6LVF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6bct_separation_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/6BCT/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6ca3_tab_before_root_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/6CA3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_q5mg_tab_before_root_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/Q5MG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_y79y_001_space_tab_block_scalar_content",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-001/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_y79y_002_tab_only_flow_sequence_separation",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-002/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_y79y_010_tab_separated_negative_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-010/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3rln_001_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-001/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3rln_002_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-002/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_00_space_tab_mapping_value",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_02_space_tab_double_quoted_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_03_space_tab_blank_line_before_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-03/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_04_tab_only_blank_line_between_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-04/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_05_space_tab_blank_line_between_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-05/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_07_tab_only_line_before_document_start",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-07/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_08_tabs_in_double_quoted_folded_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-08/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kh5v_001_double_quoted_inline_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-001/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6wpf_double_quoted_multiline_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/6WPF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kss4_same_indent_double_quoted_stream_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/KSS4/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_xw4d_various_trailing_comments",
        input: include_str!("fixtures/yaml-test-suite/data/XW4D/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_rzp5_various_trailing_comments_same_line_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/RZP5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_a984_multiline_scalar_in_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/A984/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_p2ad_block_scalar_header",
        input: include_str!("fixtures/yaml-test-suite/data/P2AD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_f8f9_block_scalar_chomping",
        input: include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_f6mc_folded_block_more_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/F6MC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_36f6_multiline_plain_scalar_with_empty_line",
        input: include_str!("fixtures/yaml-test-suite/data/36F6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5gbf_empty_lines_in_flow_and_block_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/5GBF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4cqq_multi_line_flow_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/4CQQ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9sa2_multiline_double_quoted_flow_key",
        input: include_str!("fixtures/yaml-test-suite/data/9SA2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_s4jq_preserve_explicit_non_specific_tag",
        input: include_str!("fixtures/yaml-test-suite/data/S4JQ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_bec7_yaml_version_1_3_directive",
        input: include_str!("fixtures/yaml-test-suite/data/BEC7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mus6_02_yaml_version_extra_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mus6_03_yaml_version_tab_spacing",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-03/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mus6_04_yaml_version_comment",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-04/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6ck3_tag_shorthand_suffix_escapes",
        input: include_str!("fixtures/yaml-test-suite/data/6CK3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "github_actions_matrix",
        input: include_str!("fixtures/real-world/github-actions/matrix-ci.yaml"),
        docs: 1,
    },
    Case {
        name: "github_actions_starter_node_ci",
        input: include_str!("fixtures/real-world/github-actions/starter-node-ci.yml"),
        docs: 1,
    },
    Case {
        name: "github_actions_minimal",
        input: include_str!("fixtures/real-world/github-actions/minimal-ci.yaml"),
        docs: 1,
    },
    Case {
        name: "github_actions_polymorphic",
        input: include_str!("fixtures/real-world/github-actions/polymorphic-workflow.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose",
        input: include_str!("fixtures/real-world/docker-compose/compose.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_awesome_nginx_flask_mysql",
        input: include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_anchors",
        input: include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_polymorphic",
        input: include_str!("fixtures/real-world/docker-compose/compose-polymorphic.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_platform_resources",
        input: include_str!("fixtures/real-world/docker-compose/compose-platform-resources.yaml"),
        docs: 1,
    },
    Case {
        name: "kubernetes_multidoc",
        input: include_str!("fixtures/real-world/kubernetes/multi-doc.yaml"),
        docs: 2,
    },
    Case {
        name: "kubernetes_deployment",
        input: include_str!("fixtures/real-world/kubernetes/deployment.yaml"),
        docs: 1,
    },
    Case {
        name: "kubernetes_configmap_block_scalars",
        input: include_str!("fixtures/real-world/kubernetes/configmap-block-scalars.yaml"),
        docs: 1,
    },
    Case {
        name: "kubernetes_helm_rendered_stream",
        input: include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml"),
        docs: 5,
    },
    Case {
        name: "kubernetes_crd_openapi_stream",
        input: include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml"),
        docs: 2,
    },
    Case {
        name: "kubernetes_upstream_guestbook_frontend_deployment",
        input: include_str!(
            "fixtures/real-world/kubernetes/upstream-guestbook-frontend-deployment.yaml"
        ),
        docs: 1,
    },
    Case {
        name: "helm_values",
        input: include_str!("fixtures/real-world/helm/values.yaml"),
        docs: 1,
    },
    Case {
        name: "helm_chart_metadata",
        input: include_str!("fixtures/real-world/helm/Chart.yaml"),
        docs: 1,
    },
    Case {
        name: "helm_upstream_hello_world_chart",
        input: include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml"),
        docs: 1,
    },
    Case {
        name: "openapi_petstore_fragment",
        input: include_str!("fixtures/real-world/openapi/petstore-fragment.yaml"),
        docs: 1,
    },
    Case {
        name: "openapi_operations_and_polymorphism",
        input: include_str!("fixtures/real-world/openapi/operations-and-polymorphism.yaml"),
        docs: 1,
    },
    Case {
        name: "openapi_upstream_petstore",
        input: include_str!("fixtures/real-world/openapi/upstream-petstore.yaml"),
        docs: 1,
    },
    Case {
        name: "wrangler_yaml",
        input: include_str!("fixtures/real-world/cloudflare/wrangler.yaml"),
        docs: 1,
    },
    Case {
        name: "wrangler_adapted_durable_objects",
        input: include_str!("fixtures/real-world/cloudflare/adapted-durable-objects-wrangler.yaml"),
        docs: 1,
    },
    Case {
        name: "ansible_playbook",
        input: include_str!("fixtures/real-world/ansible/playbook.yaml"),
        docs: 1,
    },
    Case {
        name: "ansible_vault_and_unsafe_tags",
        input: include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml"),
        docs: 1,
    },
    Case {
        name: "ansible_upstream_lamp_simple_site",
        input: include_str!("fixtures/real-world/ansible/upstream-lamp-simple-site.yml"),
        docs: 1,
    },
];

#[test]
fn event_parity_matches_yaml_rust2_and_saphyr_parser_for_selected_cases() {
    for case in CASES {
        assert_tree_doc_count_parity(case);

        let ours_for_yaml_rust2 = normalize_ours(case.input, DocumentStartMode::Strip)
            .unwrap_or_else(|error| panic!("ours events failed {}: {error}", case.name));
        let yaml_rust2 = normalize_yaml_rust2(case.input)
            .unwrap_or_else(|error| panic!("yaml-rust2 events failed {}: {error}", case.name));
        assert_eq!(
            ours_for_yaml_rust2, yaml_rust2,
            "normalized yaml-rust2 event parity for {}",
            case.name
        );

        let ours_for_saphyr = normalize_ours(case.input, DocumentStartMode::Preserve)
            .unwrap_or_else(|error| panic!("ours events failed {}: {error}", case.name));
        let saphyr = normalize_saphyr_parser(case.input)
            .unwrap_or_else(|error| panic!("saphyr-parser events failed {}: {error}", case.name));
        assert_eq!(
            ours_for_saphyr, saphyr,
            "normalized saphyr-parser event parity for {}",
            case.name
        );
    }
}

#[test]
fn span_parity_scalar_and_alias_starts_match_reference_parsers_for_ascii() {
    let input = "x: &x 1\ny: *x\n";
    let ours_scalars = ours_scalar_starts(input).expect("ours scalars");
    assert_eq!(
        ours_scalars,
        yaml_rust2_scalar_starts(input).expect("yaml-rust2 scalars")
    );
    assert_eq!(
        ours_scalars,
        saphyr_scalar_starts(input).expect("saphyr scalars")
    );

    let ours_aliases = ours_alias_starts(input).expect("ours aliases");
    assert_eq!(
        ours_aliases,
        yaml_rust2_alias_starts(input).expect("yaml-rust2 aliases")
    );
    assert_eq!(
        ours_aliases,
        saphyr_alias_starts(input).expect("saphyr aliases")
    );
}

#[test]
fn span_parity_flow_collection_starts_match_reference_parsers_for_ascii() {
    let input = "root: {a: [b, c], d: e}\n";
    let ours = ours_flow_collection_starts(input).expect("ours flow collections");
    assert_eq!(
        ours,
        yaml_rust2_flow_collection_starts(input).expect("yaml-rust2 flow collections")
    );
    assert_eq!(
        ours,
        saphyr_flow_collection_starts(input).expect("saphyr flow collections")
    );
}

#[test]
fn span_parity_explicit_document_markers_match_reference_parsers_for_ascii() {
    let input = "---\nroot: v\n...\n";
    let ours = ours_document_marker_starts(input).expect("ours document markers");
    assert_eq!(
        ours,
        yaml_rust2_document_marker_starts(input).expect("yaml-rust2 document markers")
    );
    assert_eq!(
        ours,
        saphyr_document_marker_starts(input).expect("saphyr document markers")
    );
}

#[test]
fn span_parity_block_sequence_start_matches_reference_parsers_for_ascii() {
    let input = "root:\n  - one\n";
    let ours = ours_block_sequence_starts(input).expect("ours block sequence starts");
    assert_eq!(
        ours,
        yaml_rust2_block_sequence_starts(input).expect("yaml-rust2 block sequence starts")
    );
    assert_eq!(
        ours,
        saphyr_block_sequence_starts(input).expect("saphyr block sequence starts")
    );
}

#[test]
fn span_parity_block_scalar_locations_remain_documented_divergence() {
    let input = "body: |-\n  line\n";
    let ours = ours_scalar_starts(input).expect("ours scalars");
    let yaml_rust2 = yaml_rust2_scalar_starts(input).expect("yaml-rust2 scalars");
    let saphyr = saphyr_scalar_starts(input).expect("saphyr scalars");

    assert_eq!(
        ours.iter().map(|point| &point.0).collect::<Vec<_>>(),
        ["body", "line"]
    );
    assert_eq!(
        yaml_rust2.iter().map(|point| &point.0).collect::<Vec<_>>(),
        ["body", "line"]
    );
    assert_eq!(
        saphyr.iter().map(|point| &point.0).collect::<Vec<_>>(),
        ["body", "line"]
    );
    assert_ne!(
        ours[1], yaml_rust2[1],
        "block scalar start positions intentionally remain parser-specific"
    );
}

fn assert_tree_doc_count_parity(case: &Case) {
    let ours = yaml::parse_documents(case.input)
        .unwrap_or_else(|error| panic!("ours tree failed {}: {error}", case.name));
    assert_eq!(ours.len(), case.docs, "ours doc count for {}", case.name);

    let yaml_rust2 = yaml_rust2::YamlLoader::load_from_str(case.input)
        .unwrap_or_else(|error| panic!("yaml-rust2 tree failed {}: {error}", case.name));
    assert_eq!(
        yaml_rust2.len(),
        case.docs,
        "yaml-rust2 doc count for {}",
        case.name
    );

    let saphyr = saphyr::Yaml::load_from_str(case.input)
        .unwrap_or_else(|error| panic!("saphyr tree failed {}: {error}", case.name));
    assert_eq!(
        saphyr.len(),
        case.docs,
        "saphyr doc count for {}",
        case.name
    );
}

type ScalarPoint = (String, usize, usize);
type MarkerPoint = (&'static str, usize, usize);

fn ours_scalar_starts(input: &str) -> yaml::Result<Vec<ScalarPoint>> {
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::Scalar { value, span, .. } => Some((value, span.line, span.column)),
                _ => None,
            })
            .collect()
    })
}

fn yaml_rust2_scalar_starts(input: &str) -> Result<Vec<ScalarPoint>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;
    Ok(sink
        .events
        .into_iter()
        .filter_map(|(event, marker)| match event {
            yaml_rust2::parser::Event::Scalar(value, ..) => Some((
                normalize_null_scalar(value),
                marker.line(),
                marker.col() + 1,
            )),
            _ => None,
        })
        .collect())
}

fn saphyr_scalar_starts(input: &str) -> Result<Vec<ScalarPoint>, saphyr_parser::ScanError> {
    let mut scalars = Vec::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        if let saphyr_parser::Event::Scalar(value, ..) = event {
            scalars.push((
                normalize_null_scalar(value.into_owned()),
                span.start.line(),
                span.start.col() + 1,
            ));
        }
    }
    Ok(scalars)
}

fn ours_alias_starts(input: &str) -> yaml::Result<Vec<(usize, usize)>> {
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::Alias { anchor } => Some((anchor.span.line, anchor.span.column)),
                _ => None,
            })
            .collect()
    })
}

fn yaml_rust2_alias_starts(input: &str) -> Result<Vec<(usize, usize)>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;
    Ok(sink
        .events
        .into_iter()
        .filter_map(|(event, marker)| match event {
            yaml_rust2::parser::Event::Alias(_) => Some((marker.line(), marker.col() + 1)),
            _ => None,
        })
        .collect())
}

fn saphyr_alias_starts(input: &str) -> Result<Vec<(usize, usize)>, saphyr_parser::ScanError> {
    let mut aliases = Vec::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        if matches!(event, saphyr_parser::Event::Alias(_)) {
            aliases.push((span.start.line(), span.start.col() + 1));
        }
    }
    Ok(aliases)
}

fn ours_flow_collection_starts(input: &str) -> yaml::Result<Vec<MarkerPoint>> {
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::SequenceStart { span, .. } if byte_at(input, span.start) == Some(b'[') => {
                    Some(("sequence", span.line, span.column))
                }
                Event::MappingStart { span, .. } if byte_at(input, span.start) == Some(b'{') => {
                    Some(("mapping", span.line, span.column))
                }
                _ => None,
            })
            .collect()
    })
}

fn yaml_rust2_flow_collection_starts(
    input: &str,
) -> Result<Vec<MarkerPoint>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;
    Ok(sink
        .events
        .into_iter()
        .filter_map(|(event, marker)| match event {
            yaml_rust2::parser::Event::SequenceStart(..)
                if byte_at(input, marker.index()) == Some(b'[') =>
            {
                Some(("sequence", marker.line(), marker.col() + 1))
            }
            yaml_rust2::parser::Event::MappingStart(..)
                if byte_at(input, marker.index()) == Some(b'{') =>
            {
                Some(("mapping", marker.line(), marker.col() + 1))
            }
            _ => None,
        })
        .collect())
}

fn saphyr_flow_collection_starts(
    input: &str,
) -> Result<Vec<MarkerPoint>, saphyr_parser::ScanError> {
    let mut starts = Vec::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        match event {
            saphyr_parser::Event::SequenceStart(..)
                if byte_at(input, span.start.index()) == Some(b'[') =>
            {
                starts.push(("sequence", span.start.line(), span.start.col() + 1));
            }
            saphyr_parser::Event::MappingStart(..)
                if byte_at(input, span.start.index()) == Some(b'{') =>
            {
                starts.push(("mapping", span.start.line(), span.start.col() + 1));
            }
            _ => {}
        }
    }
    Ok(starts)
}

fn ours_document_marker_starts(input: &str) -> yaml::Result<Vec<MarkerPoint>> {
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::DocumentStart {
                    explicit: true,
                    span,
                    ..
                } => Some(("document-start", span.line, span.column)),
                Event::DocumentEnd {
                    explicit: true,
                    span,
                } => Some(("document-end", span.line, span.column)),
                _ => None,
            })
            .collect()
    })
}

fn yaml_rust2_document_marker_starts(
    input: &str,
) -> Result<Vec<MarkerPoint>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;
    Ok(sink
        .events
        .into_iter()
        .filter_map(|(event, marker)| match event {
            yaml_rust2::parser::Event::DocumentStart
                if byte_at(input, marker.index()) == Some(b'-') =>
            {
                Some(("document-start", marker.line(), marker.col() + 1))
            }
            yaml_rust2::parser::Event::DocumentEnd
                if byte_at(input, marker.index()) == Some(b'.') =>
            {
                Some(("document-end", marker.line(), marker.col() + 1))
            }
            _ => None,
        })
        .collect())
}

fn saphyr_document_marker_starts(
    input: &str,
) -> Result<Vec<MarkerPoint>, saphyr_parser::ScanError> {
    let mut markers = Vec::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        match event {
            saphyr_parser::Event::DocumentStart(true) => {
                markers.push(("document-start", span.start.line(), span.start.col() + 1));
            }
            saphyr_parser::Event::DocumentEnd
                if byte_at(input, span.start.index()) == Some(b'.') =>
            {
                markers.push(("document-end", span.start.line(), span.start.col() + 1));
            }
            _ => {}
        }
    }
    Ok(markers)
}

fn ours_block_sequence_starts(input: &str) -> yaml::Result<Vec<MarkerPoint>> {
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::SequenceStart { span, .. } if byte_at(input, span.start) == Some(b'-') => {
                    Some(("sequence", span.line, span.column))
                }
                _ => None,
            })
            .collect()
    })
}

fn yaml_rust2_block_sequence_starts(
    input: &str,
) -> Result<Vec<MarkerPoint>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;
    Ok(sink
        .events
        .into_iter()
        .filter_map(|(event, marker)| match event {
            yaml_rust2::parser::Event::SequenceStart(..)
                if byte_at(input, marker.index()) == Some(b'-') =>
            {
                Some(("sequence", marker.line(), marker.col() + 1))
            }
            _ => None,
        })
        .collect())
}

fn saphyr_block_sequence_starts(input: &str) -> Result<Vec<MarkerPoint>, saphyr_parser::ScanError> {
    let mut starts = Vec::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        if matches!(event, saphyr_parser::Event::SequenceStart(..))
            && byte_at(input, span.start.index()) == Some(b'-')
        {
            starts.push(("sequence", span.start.line(), span.start.col() + 1));
        }
    }
    Ok(starts)
}

fn byte_at(input: &str, index: usize) -> Option<u8> {
    input.as_bytes().get(index).copied()
}

fn normalize_ours(
    input: &str,
    document_start_mode: DocumentStartMode,
) -> yaml::Result<Vec<NormEvent>> {
    let mut anchors = AnchorNormalizer::default();
    yaml::parse_events(input).map(|events| {
        events
            .into_iter()
            .map(|event| match event {
                Event::StreamStart => NormEvent::StreamStart,
                Event::StreamEnd => NormEvent::StreamEnd,
                Event::DocumentStart { explicit, .. } => {
                    anchors.reset();
                    NormEvent::DocumentStart {
                        explicit: match document_start_mode {
                            DocumentStartMode::Preserve => Some(explicit),
                            DocumentStartMode::Strip => None,
                        },
                    }
                }
                Event::DocumentEnd { .. } => NormEvent::DocumentEnd,
                Event::SequenceStart { meta, style, .. } => NormEvent::SequenceStart {
                    style: normalize_our_collection_style(style),
                    anchor: meta
                        .anchor
                        .as_ref()
                        .map(|anchor| anchors.define_name(&anchor.name)),
                    tag: meta.tag.as_ref().map(|tag| normalize_our_tag(&tag.tag)),
                },
                Event::SequenceEnd { .. } => NormEvent::SequenceEnd,
                Event::MappingStart { meta, style, .. } => NormEvent::MappingStart {
                    style: normalize_our_collection_style(style),
                    anchor: meta
                        .anchor
                        .as_ref()
                        .map(|anchor| anchors.define_name(&anchor.name)),
                    tag: meta.tag.as_ref().map(|tag| normalize_our_tag(&tag.tag)),
                },
                Event::MappingEnd { .. } => NormEvent::MappingEnd,
                Event::Alias { anchor } => NormEvent::Alias {
                    anchor: anchors.alias_name(&anchor.name),
                },
                Event::Scalar {
                    value, style, meta, ..
                } => NormEvent::Scalar {
                    value: normalize_our_scalar_value(value, style),
                    style: normalize_our_style(style),
                    anchor: meta
                        .anchor
                        .as_ref()
                        .map(|anchor| anchors.define_name(&anchor.name)),
                    tag: meta.tag.as_ref().map(|tag| normalize_our_tag(&tag.tag)),
                },
            })
            .collect()
    })
}

fn normalize_yaml_rust2(input: &str) -> Result<Vec<NormEvent>, yaml_rust2::ScanError> {
    let mut sink = YamlRust2Sink { events: Vec::new() };
    let mut parser = yaml_rust2::parser::Parser::new_from_str(input);
    parser.load(&mut sink, true)?;

    let mut anchors = AnchorNormalizer::default();
    let mut events = Vec::new();
    let mut collections = Vec::<NormCollectionStyle>::new();
    for (event, marker) in sink.events {
        match event {
            yaml_rust2::parser::Event::Nothing => {}
            yaml_rust2::parser::Event::StreamStart => events.push(NormEvent::StreamStart),
            yaml_rust2::parser::Event::StreamEnd => events.push(NormEvent::StreamEnd),
            yaml_rust2::parser::Event::DocumentStart => {
                anchors.reset();
                events.push(NormEvent::DocumentStart { explicit: None });
            }
            yaml_rust2::parser::Event::DocumentEnd => events.push(NormEvent::DocumentEnd),
            yaml_rust2::parser::Event::Alias(anchor) => events.push(NormEvent::Alias {
                anchor: anchors.alias_id(anchor),
            }),
            yaml_rust2::parser::Event::Scalar(value, style, anchor, tag) => {
                events.push(NormEvent::Scalar {
                    value: normalize_null_scalar(value),
                    style: normalize_yaml_rust2_style(style),
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
            }
            yaml_rust2::parser::Event::SequenceStart(anchor, tag) => {
                let style = reference_sequence_style(input, marker.index());
                events.push(NormEvent::SequenceStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
            }
            yaml_rust2::parser::Event::SequenceEnd => {
                collections.pop();
                events.push(NormEvent::SequenceEnd);
            }
            yaml_rust2::parser::Event::MappingStart(anchor, tag) => {
                let style = reference_mapping_style(input, marker.index(), &collections);
                events.push(NormEvent::MappingStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
            }
            yaml_rust2::parser::Event::MappingEnd => {
                collections.pop();
                events.push(NormEvent::MappingEnd);
            }
        }
    }
    Ok(events)
}

fn normalize_saphyr_parser(input: &str) -> Result<Vec<NormEvent>, saphyr_parser::ScanError> {
    let mut anchors = AnchorNormalizer::default();
    let mut events = Vec::new();
    let mut collections = Vec::<NormCollectionStyle>::new();
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        match event {
            saphyr_parser::Event::Nothing => {}
            saphyr_parser::Event::StreamStart => events.push(NormEvent::StreamStart),
            saphyr_parser::Event::StreamEnd => events.push(NormEvent::StreamEnd),
            saphyr_parser::Event::DocumentStart(explicit) => {
                anchors.reset();
                events.push(NormEvent::DocumentStart {
                    explicit: Some(explicit),
                });
            }
            saphyr_parser::Event::DocumentEnd => events.push(NormEvent::DocumentEnd),
            saphyr_parser::Event::Alias(anchor) => events.push(NormEvent::Alias {
                anchor: anchors.alias_id(anchor),
            }),
            saphyr_parser::Event::Scalar(value, style, anchor, tag) => {
                events.push(NormEvent::Scalar {
                    value: normalize_null_scalar(value.into_owned()),
                    style: normalize_saphyr_style(style),
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
            }
            saphyr_parser::Event::SequenceStart(anchor, tag) => {
                let style = reference_sequence_style(input, span.start.index());
                events.push(NormEvent::SequenceStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
            }
            saphyr_parser::Event::SequenceEnd => {
                collections.pop();
                events.push(NormEvent::SequenceEnd);
            }
            saphyr_parser::Event::MappingStart(anchor, tag) => {
                let style = reference_mapping_style(input, span.start.index(), &collections);
                events.push(NormEvent::MappingStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
            }
            saphyr_parser::Event::MappingEnd => {
                collections.pop();
                events.push(NormEvent::MappingEnd);
            }
        }
    }
    Ok(events)
}

fn normalize_our_collection_style(style: CollectionStyle) -> NormCollectionStyle {
    match style {
        CollectionStyle::Block => NormCollectionStyle::Block,
        CollectionStyle::Flow => NormCollectionStyle::Flow,
    }
}

fn reference_sequence_style(input: &str, index: usize) -> NormCollectionStyle {
    if byte_at(input, index) == Some(b'[') {
        NormCollectionStyle::Flow
    } else {
        NormCollectionStyle::Block
    }
}

fn reference_mapping_style(
    input: &str,
    index: usize,
    collections: &[NormCollectionStyle],
) -> NormCollectionStyle {
    if byte_at(input, index) == Some(b'{') || collections.last() == Some(&NormCollectionStyle::Flow)
    {
        NormCollectionStyle::Flow
    } else {
        NormCollectionStyle::Block
    }
}

fn normalize_our_style(style: ScalarStyle) -> NormStyle {
    match style {
        ScalarStyle::Plain => NormStyle::Plain,
        ScalarStyle::SingleQuoted => NormStyle::SingleQuoted,
        ScalarStyle::DoubleQuoted => NormStyle::DoubleQuoted,
        ScalarStyle::Literal => NormStyle::Literal,
        ScalarStyle::Folded => NormStyle::Folded,
    }
}

fn normalize_yaml_rust2_style(style: yaml_rust2::scanner::TScalarStyle) -> NormStyle {
    match style {
        yaml_rust2::scanner::TScalarStyle::Plain => NormStyle::Plain,
        yaml_rust2::scanner::TScalarStyle::SingleQuoted => NormStyle::SingleQuoted,
        yaml_rust2::scanner::TScalarStyle::DoubleQuoted => NormStyle::DoubleQuoted,
        yaml_rust2::scanner::TScalarStyle::Literal => NormStyle::Literal,
        yaml_rust2::scanner::TScalarStyle::Folded => NormStyle::Folded,
    }
}

fn normalize_saphyr_style(style: saphyr_parser::ScalarStyle) -> NormStyle {
    match style {
        saphyr_parser::ScalarStyle::Plain => NormStyle::Plain,
        saphyr_parser::ScalarStyle::SingleQuoted => NormStyle::SingleQuoted,
        saphyr_parser::ScalarStyle::DoubleQuoted => NormStyle::DoubleQuoted,
        saphyr_parser::ScalarStyle::Literal => NormStyle::Literal,
        saphyr_parser::ScalarStyle::Folded => NormStyle::Folded,
    }
}

fn normalize_our_scalar_value(value: String, style: ScalarStyle) -> String {
    if matches!(style, ScalarStyle::Literal | ScalarStyle::Folded) {
        normalize_null_scalar(value)
    } else {
        value
    }
}

fn normalize_null_scalar(value: String) -> String {
    if value.is_empty() || value == "~" {
        "null".to_string()
    } else {
        value
    }
}

fn normalize_our_tag(tag: &yaml::Tag) -> String {
    normalize_reference_tag(&tag.handle, &tag.suffix)
}

fn normalize_reference_tag(handle: &str, suffix: &str) -> String {
    match handle {
        "!!" | "tag:yaml.org,2002:" => format!("tag:yaml.org,2002:{suffix}"),
        "!" => {
            if suffix.starts_with("tag:") {
                suffix.to_string()
            } else {
                format!("!{suffix}")
            }
        }
        _ => format!("{handle}{suffix}"),
    }
}
