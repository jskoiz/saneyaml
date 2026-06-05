use saneyaml::{LoadOptions, Node, NodeValue as Value, parse_documents, parse_events, parse_str};
use saphyr::LoadableYamlNode;
use serde::Deserialize;

struct Case {
    name: &'static str,
    input: &'static str,
    docs: usize,
}

const SHARED_ACCEPT_CASES: &[Case] = &[
    Case {
        name: "core_scalars",
        input: "nulls: [null, ~]\nbools: [true, false]\nstrings: [\"true\", \"001\", \"2026-05-23\"]\nints: [0, 42, -7]\nfloats: [3.14, -0.5]\n",
        docs: 1,
    },
    Case {
        name: "flow_and_block",
        input: "flow: {a: [1, 2, 3], b: {c: d}}\nblock:\n  - name: alpha\n    enabled: true\n  - name: beta\n    enabled: false\n",
        docs: 1,
    },
    Case {
        name: "flow_sequence_implicit_mapping_entries",
        input: "root: [a: b, c: d]\n",
        docs: 1,
    },
    Case {
        name: "flow_sequence_implicit_mapping_explicit_and_collection_keys",
        input: "root: [? a: b, ? [c, d]: e, [f, g]: h]\n",
        docs: 1,
    },
    Case {
        name: "yts_9mmw_single_pair_implicit_entries",
        input: include_str!("fixtures/yaml-test-suite/data/9MMW/in.yaml"),
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
        name: "yts_8kb6_multiline_plain_flow_mapping_key_without_value",
        input: include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "multiline_flow_sequence_with_comment",
        input: include_str!("fixtures/yaml-test-suite/data/7TMG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "multiline_flow_mapping",
        input: "root: { a: b\n, c: d }\n",
        docs: 1,
    },
    Case {
        name: "flow_mapping_plain_keys_without_values",
        input: "root: {a, b: c}\n",
        docs: 1,
    },
    Case {
        name: "flow_mapping_explicit_scalar_keys",
        input: "root: {? a: b, ? c, d: e}\n",
        docs: 1,
    },
    Case {
        name: "flow_mapping_collection_keys",
        input: "root: {? [a, b]: c, ? {d: e}: f}\n",
        docs: 1,
    },
    Case {
        name: "flow_mapping_key_metadata",
        input: "key: &key alias-key\nroot: {&direct direct-key: v, ? *key : alias-v, ? &seq [a, b] : seq-v, !Thing tagged-key: tagged-v}\n",
        docs: 1,
    },
    Case {
        name: "flow_anchor_only_null_nodes",
        input: "root: [&empty, *empty]\n",
        docs: 1,
    },
    Case {
        name: "yts_pw8x_anchors_on_empty_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3r3p_single_block_sequence_with_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/3R3P/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6kgn_anchor_for_empty_node",
        input: include_str!("fixtures/yaml-test-suite/data/6KGN/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7bmt_node_and_mapping_key_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/7BMT/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7bub_spec_node_referenced_by_alias",
        input: include_str!("fixtures/yaml-test-suite/data/7BUB/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_cn3r_flow_sequence_anchor_locations",
        input: include_str!("fixtures/yaml-test-suite/data/CN3R/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_cup7_node_property_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/CUP7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_e76z_aliases_in_implicit_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/E76Z/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_y2gn_anchor_with_colon_in_middle",
        input: include_str!("fixtures/yaml-test-suite/data/Y2GN/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_zwk4_anchor_after_missing_explicit_value",
        input: include_str!("fixtures/yaml-test-suite/data/ZWK4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6bfj_mapping_key_and_flow_sequence_item_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6pbe_zero_indented_explicit_sequence_key",
        input: include_str!("fixtures/yaml-test-suite/data/6PBE/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ske5_anchor_before_zero_indented_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/SKE5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_57h4_block_collection_nodes",
        input: include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6ck3_tag_shorthand_suffix_escapes",
        input: include_str!("fixtures/yaml-test-suite/data/6CK3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_bu8l_node_anchor_and_tag_on_separate_lines",
        input: include_str!("fixtures/yaml-test-suite/data/BU8L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9kax_tag_anchor_property_combinations",
        input: include_str!("fixtures/yaml-test-suite/data/9KAX/in.yaml"),
        docs: 8,
    },
    Case {
        name: "flow_mapping_plain_url_key",
        input: "root: {http://example.com: value}\n",
        docs: 1,
    },
    Case {
        name: "typed_scalar_keys",
        input: "1: int\n\"1\": string\ntrue: bool\n\"true\": string\nnull: null-key\n\"null\": string-null\n",
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
        name: "yts_ab8u_sequence_entry_looking_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/AB8U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "explicit_block_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/5WE3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "compact_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/V9D5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "indentless_sequence_mapping_value",
        input: "items:\n- one\n",
        docs: 1,
    },
    Case {
        name: "anchor_alias_values",
        input: "x-base: &base\n  image: nginx\nservices:\n  web: *base\n",
        docs: 1,
    },
    Case {
        name: "anchor_redefinition_last_wins",
        input: "a: &x 1\nb: &x 2\nc: *x\n",
        docs: 1,
    },
    Case {
        name: "mapping_key_anchor",
        input: "top:\n  &k key: value\n",
        docs: 1,
    },
    Case {
        name: "default_merge_key_with_alias_value",
        input: "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n",
        docs: 1,
    },
    Case {
        name: "multidoc",
        input: "---\nkind: first\n---\nkind: second\n...\n",
        docs: 2,
    },
    Case {
        name: "commented_multidoc",
        input: include_str!("fixtures/yaml-test-suite/data/JHB9/in.yaml"),
        docs: 2,
    },
    Case {
        name: "explicit_empty_documents",
        input: "---\n---\nkind: second\n",
        docs: 2,
    },
    Case {
        name: "multiline_plain_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/A984/in.yaml"),
        docs: 1,
    },
    Case {
        name: "multiline_plain_scalar_empty_line",
        input: include_str!("fixtures/yaml-test-suite/data/36F6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5gbf_empty_lines_in_flow_and_block_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/5GBF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "multiline_flow_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/4CQQ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "block_scalar_headers",
        input: include_str!("fixtures/yaml-test-suite/data/P2AD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "block_scalar_chomping_trailing_lines",
        input: include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "empty_block_scalar_chomping",
        input: include_str!("fixtures/yaml-test-suite/data/K858/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_more_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/F6MC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_paragraphs",
        input: include_str!("fixtures/yaml-test-suite/data/6VJK/in.yaml"),
        docs: 1,
    },
    Case {
        name: "literal_block_scalar_spaces_only_line",
        input: include_str!("fixtures/yaml-test-suite/data/6FWR/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_empty_lines_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/4Q9F/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_empty_lines",
        input: include_str!("fixtures/yaml-test-suite/data/TS54/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_list_like_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/7T8X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_strip_spaces_explicit_start",
        input: include_str!("fixtures/yaml-test-suite/data/93WF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "folded_block_scalar_strip_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/K527/in.yaml"),
        docs: 1,
    },
    Case {
        name: "block_scalar_nodes",
        input: include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml"),
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
        name: "yts_ukk6_02_bare_explicit_non_specific_tag",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-02/in.yaml"),
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
        name: "yts_y79y_002_tab_only_flow_sequence_separation",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-002/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3rln_00_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-00/in.yaml"),
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
        name: "yts_3rln_03_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-03/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3rln_04_double_quoted_escaped_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-04/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3rln_05_double_quoted_leading_tab_folded",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-05/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_00_double_quoted_trailing_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_01_double_quoted_trailing_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_02_double_quoted_escaped_line_end_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_03_double_quoted_escaped_line_end_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-03/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_04_double_quoted_literal_trailing_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-04/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_de56_05_double_quoted_literal_trailing_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-05/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dk95_02_space_tab_double_quoted_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-02/in.yaml"),
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
        name: "yts_cpz3_double_quoted_scalar_starting_with_tab",
        input: include_str!("fixtures/yaml-test-suite/data/CPZ3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dc7x_trailing_tabs",
        input: include_str!("fixtures/yaml-test-suite/data/DC7X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_nb6z_plain_scalar_tabs_on_empty_lines",
        input: include_str!("fixtures/yaml-test-suite/data/NB6Z/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_uv7q_legal_tab_after_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/UV7Q/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kh5v_00_double_quoted_inline_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kh5v_001_double_quoted_inline_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-001/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kh5v_02_double_quoted_inline_escaped_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-02/in.yaml"),
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
        name: "yts_mzx3_scalar_styles",
        input: include_str!("fixtures/yaml-test-suite/data/MZX3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_s4jq_preserve_explicit_non_specific_tag",
        input: include_str!("fixtures/yaml-test-suite/data/S4JQ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_27na_yaml_version_1_2_directive",
        input: include_str!("fixtures/yaml-test-suite/data/27NA/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6zkb_stream_yaml_version_directive",
        input: include_str!("fixtures/yaml-test-suite/data/6ZKB/in.yaml"),
        docs: 3,
    },
    Case {
        name: "yts_9dxl_mapping_stream_yaml_version_directive",
        input: include_str!("fixtures/yaml-test-suite/data/9DXL/in.yaml"),
        docs: 3,
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
        name: "yts_u3c3_tag_directive_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/U3C3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_fta2_document_start_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/FTA2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "github_actions_minimal",
        input: include_str!("fixtures/real-world/github-actions/minimal-ci.yaml"),
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
        name: "github_actions_polymorphic",
        input: include_str!("fixtures/real-world/github-actions/polymorphic-workflow.yaml"),
        docs: 1,
    },
    Case {
        name: "github_actions_reusable_service_workflow",
        input: include_str!("fixtures/real-world/github-actions/reusable-service-workflow.yaml"),
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
        name: "docker_compose_extension_anchors",
        input: include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_polymorphic_service_fields",
        input: include_str!("fixtures/real-world/docker-compose/compose-polymorphic.yaml"),
        docs: 1,
    },
    Case {
        name: "docker_compose_adapted_compose_spec_fragments",
        input: include_str!(
            "fixtures/real-world/docker-compose/adapted-compose-spec-fragments.yaml"
        ),
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
        name: "openapi_fragment",
        input: include_str!("fixtures/real-world/openapi/petstore-fragment.yaml"),
        docs: 1,
    },
    Case {
        name: "openapi_operations_and_extensions",
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
        name: "cloudformation_sam_api",
        input: include_str!("fixtures/real-world/cloudformation/sam-api.yaml"),
        docs: 1,
    },
    Case {
        name: "symfony_services",
        input: include_str!("fixtures/real-world/symfony/services.yaml"),
        docs: 1,
    },
    Case {
        name: "gitlab_ci_basic_pipeline",
        input: include_str!("fixtures/real-world/gitlab-ci/basic-pipeline.yml"),
        docs: 1,
    },
    Case {
        name: "circleci_config",
        input: include_str!("fixtures/real-world/circleci/config.yml"),
        docs: 1,
    },
    Case {
        name: "azure_pipelines",
        input: include_str!("fixtures/real-world/azure-pipelines/azure-pipelines.yml"),
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
    Case {
        name: "yts_229q_spec_example_2_4_sequence_of_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/229Q/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2g84_02_literal_modifers",
        input: include_str!("fixtures/yaml-test-suite/data/2G84-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2g84_03_literal_modifers",
        input: include_str!("fixtures/yaml-test-suite/data/2G84-03/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3alj_block_sequence_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/3ALJ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3uys_escaped_slash_in_double_quotes",
        input: include_str!("fixtures/yaml-test-suite/data/3UYS/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4gc6_spec_example_7_7_single_quoted_characters",
        input: include_str!("fixtures/yaml-test-suite/data/4GC6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4qfq_spec_example_8_2_block_indentation_indicator_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/4QFQ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4rwc_trailing_spaces_after_flow_collection",
        input: include_str!("fixtures/yaml-test-suite/data/4RWC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4uyu_colon_in_double_quoted_string",
        input: include_str!("fixtures/yaml-test-suite/data/4UYU/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4v8u_plain_scalar_with_backslashes",
        input: include_str!("fixtures/yaml-test-suite/data/4V8U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4wa9_literal_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/4WA9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4zym_spec_example_6_4_line_prefixes",
        input: include_str!("fixtures/yaml-test-suite/data/4ZYM/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_54t7_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/54T7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5bvj_spec_example_5_7_block_scalar_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/5BVJ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5c5m_spec_example_7_15_flow_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/5C5M/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5kje_spec_example_7_13_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/5KJE/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5nyz_spec_example_6_9_separated_comment",
        input: include_str!("fixtures/yaml-test-suite/data/5NYZ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_65wh_single_entry_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/65WH/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6h3v_backslashes_in_singlequotes",
        input: include_str!("fixtures/yaml-test-suite/data/6H3V/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6hb6_spec_example_6_1_indentation_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/6HB6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6jqw_spec_example_2_13_in_literals_newlines_are_preserved",
        input: include_str!("fixtures/yaml-test-suite/data/6JQW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6xdy_two_document_start_markers",
        input: include_str!("fixtures/yaml-test-suite/data/6XDY/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_753e_block_scalar_strip_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/753E/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7a4e_spec_example_7_6_double_quoted_lines",
        input: include_str!("fixtures/yaml-test-suite/data/7A4E/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7zz5_empty_flow_collections",
        input: include_str!("fixtures/yaml-test-suite/data/7ZZ5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_82an_three_dashes_and_content_without_space",
        input: include_str!("fixtures/yaml-test-suite/data/82AN/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_87e4_spec_example_7_8_single_quoted_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/87E4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8cwc_plain_mapping_key_ending_with_colon",
        input: include_str!("fixtures/yaml-test-suite/data/8CWC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8qbe_block_sequence_in_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/8QBE/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8udb_spec_example_7_14_flow_sequence_entries",
        input: include_str!("fixtures/yaml-test-suite/data/8UDB/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_93jh_block_mappings_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/93JH/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_96l6_spec_example_2_14_in_the_folded_scalars_newlines_become_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/96L6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9bxh_multiline_doublequoted_flow_mapping_key_without_value",
        input: include_str!("fixtures/yaml-test-suite/data/9BXH/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9fmg_multi_level_mapping_indent",
        input: include_str!("fixtures/yaml-test-suite/data/9FMG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9j7a_simple_mapping_indent",
        input: include_str!("fixtures/yaml-test-suite/data/9J7A/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9mqt_00_scalar_doc_with_in_content",
        input: include_str!("fixtures/yaml-test-suite/data/9MQT-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9shh_spec_example_5_8_quoted_scalar_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/9SHH/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9tfx_spec_example_7_6_double_quoted_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/9TFX/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9u5k_spec_example_2_12_compact_nested_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/9U5K/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9yrd_multiline_scalar_at_top_level",
        input: include_str!("fixtures/yaml-test-suite/data/9YRD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_a6f9_spec_example_8_4_chomping_final_line_break",
        input: include_str!("fixtures/yaml-test-suite/data/A6F9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_az63_sequence_with_same_indentation_as_parent_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/AZ63/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_26dv_whitespace_around_colon_in_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/26DV/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_3myt_plain_scalar_looking_like_key_comment_anchor_and_tag",
        input: include_str!("fixtures/yaml-test-suite/data/3MYT/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4fj6_nested_implicit_complex_keys",
        input: include_str!("fixtures/yaml-test-suite/data/4FJ6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6sla_allowed_characters_in_quoted_mapping_key",
        input: include_str!("fixtures/yaml-test-suite/data/6SLA/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_azw3_lookahead_test_cases",
        input: include_str!("fixtures/yaml-test-suite/data/AZW3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_b3hg_spec_example_8_9_folded_scalar_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/B3HG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_d83l_block_scalar_indicator_order",
        input: include_str!("fixtures/yaml-test-suite/data/D83L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_d88j_flow_sequence_in_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/D88J/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_d9tu_single_pair_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/D9TU/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dff7_spec_example_7_16_flow_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DFF7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_dwx9_spec_example_8_8_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/DWX9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ex5h_multiline_scalar_at_top_level_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/EX5H/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_exg3_three_dashes_and_content_without_space_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/EXG3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_f3cp_nested_flow_collections_on_one_line",
        input: include_str!("fixtures/yaml-test-suite/data/F3CP/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_fq7f_spec_example_2_1_sequence_of_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/FQ7F/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_fup4_flow_sequence_in_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/FUP4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_g4rs_spec_example_2_17_quoted_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/G4RS/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_g992_spec_example_8_9_folded_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/G992/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_gh63_mixed_block_mapping_explicit_to_implicit",
        input: include_str!("fixtures/yaml-test-suite/data/GH63/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_h2rw_blank_lines",
        input: include_str!("fixtures/yaml-test-suite/data/H2RW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_h3z8_literal_unicode",
        input: include_str!("fixtures/yaml-test-suite/data/H3Z8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_hmk4_spec_example_2_16_indentation_determines_scope",
        input: include_str!("fixtures/yaml-test-suite/data/HMK4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_hs5t_spec_example_7_12_plain_lines",
        input: include_str!("fixtures/yaml-test-suite/data/HS5T/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_j3bt_spec_example_5_12_tabs_and_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/J3BT/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_j5uc_multiple_pair_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/J5UC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_j7vc_empty_lines_between_mapping_elements",
        input: include_str!("fixtures/yaml-test-suite/data/J7VC/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_j9hz_spec_example_2_9_single_document_with_two_comments",
        input: include_str!("fixtures/yaml-test-suite/data/J9HZ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jef9_00_trailing_whitespace_in_streams",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jef9_01_trailing_whitespace_in_streams",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jq4r_spec_example_8_14_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/JQ4R/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_js2j_spec_example_6_29_node_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/JS2J/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jtv5_block_mapping_with_multiline_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/JTV5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_k4su_multiple_entry_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/K4SU/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kk5p_various_combinations_of_explicit_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/KK5P/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_kmk3_block_submapping",
        input: include_str!("fixtures/yaml-test-suite/data/KMK3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_l24t_00_trailing_line_of_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/L24T-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_l383_two_scalar_docs_with_trailing_comments",
        input: include_str!("fixtures/yaml-test-suite/data/L383/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_l9u5_spec_example_7_11_plain_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/L9U5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_lp6e_whitespace_after_scalars_in_flow",
        input: include_str!("fixtures/yaml-test-suite/data/LP6E/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_lqz7_spec_example_7_4_double_quoted_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/LQZ7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_lx3p_implicit_flow_mapping_key_on_one_line",
        input: include_str!("fixtures/yaml-test-suite/data/LX3P/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m29m_literal_block_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/M29M/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m2n8_01_question_mark_edge_cases",
        input: include_str!("fixtures/yaml-test-suite/data/M2N8-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m5dy_spec_example_2_11_mapping_between_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/M5DY/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m6yh_block_sequence_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/M6YH/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m7nx_nested_flow_collections",
        input: include_str!("fixtures/yaml-test-suite/data/M7NX/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_m9b4_spec_example_8_7_literal_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/M9B4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mjs9_spec_example_6_7_block_folding",
        input: include_str!("fixtures/yaml-test-suite/data/MJS9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mxs3_flow_mapping_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/MXS3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_myw6_block_scalar_strip",
        input: include_str!("fixtures/yaml-test-suite/data/MYW6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_nat4_various_empty_or_newline_only_quoted_strings",
        input: include_str!("fixtures/yaml-test-suite/data/NAT4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_np9h_spec_example_7_5_double_quoted_line_breaks",
        input: include_str!("fixtures/yaml-test-suite/data/NP9H/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_p94k_spec_example_6_11_multi_line_comments",
        input: include_str!("fixtures/yaml-test-suite/data/P94K/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_pbj2_spec_example_2_3_mapping_scalars_to_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/PBJ2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_prh3_spec_example_7_9_single_quoted_lines",
        input: include_str!("fixtures/yaml-test-suite/data/PRH3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_puw8_document_start_on_last_line",
        input: include_str!("fixtures/yaml-test-suite/data/PUW8/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_q88a_spec_example_7_23_flow_content",
        input: include_str!("fixtures/yaml-test-suite/data/Q88A/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_q8ad_spec_example_7_5_double_quoted_line_breaks_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/Q8AD/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_r52l_nested_flow_mapping_sequence_and_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/R52L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_rlu9_sequence_indent",
        input: include_str!("fixtures/yaml-test-suite/data/RLU9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_rr7f_mixed_block_mapping_implicit_to_explicit",
        input: include_str!("fixtures/yaml-test-suite/data/RR7F/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_rzt7_spec_example_2_28_log_file",
        input: include_str!("fixtures/yaml-test-suite/data/RZT7/in.yaml"),
        docs: 3,
    },
    Case {
        name: "yts_s4t7_document_with_footer",
        input: include_str!("fixtures/yaml-test-suite/data/S4T7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_s7bg_colon_followed_by_comma",
        input: include_str!("fixtures/yaml-test-suite/data/S7BG/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_s9e8_spec_example_5_3_block_structure_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/S9E8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_sbg9_flow_sequence_in_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/SBG9/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_sm9w_00_single_character_streams",
        input: include_str!("fixtures/yaml-test-suite/data/SM9W-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ssw6_spec_example_7_7_single_quoted_characters_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/SSW6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_syw4_spec_example_2_2_mapping_scalars_to_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/SYW4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_t26h_spec_example_8_8_literal_content_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T26H/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_t4yy_spec_example_7_9_single_quoted_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T4YY/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_t5n4_spec_example_8_7_literal_scalar_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T5N4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_te2a_spec_example_8_16_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/TE2A/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_tl85_spec_example_6_8_flow_folding",
        input: include_str!("fixtures/yaml-test-suite/data/TL85/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_u9ns_spec_example_2_8_play_by_play_feed_from_a_game",
        input: include_str!("fixtures/yaml-test-suite/data/U9NS/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_udr7_spec_example_5_4_flow_collection_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/UDR7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ukk6_01_syntax_character_edge_cases",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_v55r_aliases_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/V55R/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_w42u_spec_example_8_15_block_sequence_entry_types",
        input: include_str!("fixtures/yaml-test-suite/data/W42U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_x8dw_explicit_key_and_value_seperated_by_comment",
        input: include_str!("fixtures/yaml-test-suite/data/X8DW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_xv9v_spec_example_6_5_empty_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/XV9V/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_yd5x_spec_example_2_5_sequence_of_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/YD5X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_z67p_spec_example_8_21_block_scalar_nodes_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/Z67P/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_zf4x_spec_example_2_6_mapping_of_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/ZF4X/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_zh7c_anchors_in_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/ZH7C/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_zk9h_nested_top_level_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/ZK9H/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2auy",
        input: include_str!("fixtures/yaml-test-suite/data/2AUY/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2xxw",
        input: include_str!("fixtures/yaml-test-suite/data/2XXW/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_33x3",
        input: include_str!("fixtures/yaml-test-suite/data/33X3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_35kp",
        input: include_str!("fixtures/yaml-test-suite/data/35KP/in.yaml"),
        docs: 3,
    },
    Case {
        name: "yts_52dl",
        input: include_str!("fixtures/yaml-test-suite/data/52DL/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_565n",
        input: include_str!("fixtures/yaml-test-suite/data/565N/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5tym",
        input: include_str!("fixtures/yaml-test-suite/data/5TYM/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_652z",
        input: include_str!("fixtures/yaml-test-suite/data/652Z/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6jwb",
        input: include_str!("fixtures/yaml-test-suite/data/6JWB/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6wlz",
        input: include_str!("fixtures/yaml-test-suite/data/6WLZ/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_735y",
        input: include_str!("fixtures/yaml-test-suite/data/735Y/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_74h7",
        input: include_str!("fixtures/yaml-test-suite/data/74H7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_7fwl",
        input: include_str!("fixtures/yaml-test-suite/data/7FWL/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8mk2",
        input: include_str!("fixtures/yaml-test-suite/data/8MK2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_9wxw",
        input: include_str!("fixtures/yaml-test-suite/data/9WXW/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_c4hz",
        input: include_str!("fixtures/yaml-test-suite/data/C4HZ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_cc74",
        input: include_str!("fixtures/yaml-test-suite/data/CC74/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ehf6",
        input: include_str!("fixtures/yaml-test-suite/data/EHF6/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_f2c7",
        input: include_str!("fixtures/yaml-test-suite/data/F2C7/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_fh7j_tags_on_empty_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/FH7J/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_hm87_01",
        input: include_str!("fixtures/yaml-test-suite/data/HM87-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_hmq5",
        input: include_str!("fixtures/yaml-test-suite/data/HMQ5/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_j7pz",
        input: include_str!("fixtures/yaml-test-suite/data/J7PZ/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jef9_02",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-02/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_jr7v",
        input: include_str!("fixtures/yaml-test-suite/data/JR7V/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_k54u",
        input: include_str!("fixtures/yaml-test-suite/data/K54U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_l24t_01",
        input: include_str!("fixtures/yaml-test-suite/data/L24T-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_l94m",
        input: include_str!("fixtures/yaml-test-suite/data/L94M/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_le5a",
        input: include_str!("fixtures/yaml-test-suite/data/LE5A/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_p76l",
        input: include_str!("fixtures/yaml-test-suite/data/P76L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_q9wf",
        input: include_str!("fixtures/yaml-test-suite/data/Q9WF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_rtp8",
        input: include_str!("fixtures/yaml-test-suite/data/RTP8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_udm2",
        input: include_str!("fixtures/yaml-test-suite/data/UDM2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ugm3",
        input: include_str!("fixtures/yaml-test-suite/data/UGM3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_wz62",
        input: include_str!("fixtures/yaml-test-suite/data/WZ62/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_z9m4",
        input: include_str!("fixtures/yaml-test-suite/data/Z9M4/in.yaml"),
        docs: 1,
    },
];

#[test]
#[allow(deprecated)]
fn compatibility_shared_acceptance_cases_parse_with_reference_crates() {
    for case in SHARED_ACCEPT_CASES {
        let ours = parse_documents(case.input)
            .unwrap_or_else(|error| panic!("ours failed {}: {error}", case.name));
        assert_eq!(ours.len(), case.docs, "ours doc count for {}", case.name);

        let serde_docs = serde_yaml::Deserializer::from_str(case.input).count();
        assert_eq!(
            serde_docs, case.docs,
            "serde_yaml doc count for {}",
            case.name
        );

        let yaml_rust_docs = yaml_rust2::YamlLoader::load_from_str(case.input)
            .unwrap_or_else(|error| panic!("yaml-rust2 failed {}: {error}", case.name));
        assert_eq!(
            yaml_rust_docs.len(),
            case.docs,
            "yaml-rust2 doc count for {}",
            case.name
        );

        let saphyr_docs = saphyr::Yaml::load_from_str(case.input)
            .unwrap_or_else(|error| panic!("saphyr failed {}: {error}", case.name));
        assert_eq!(
            saphyr_docs.len(),
            case.docs,
            "saphyr doc count for {}",
            case.name
        );
    }
}

#[test]
fn compatibility_core_schema_resolution_is_stable_for_config_values() {
    let doc =
        parse_str("on: push\nyes: deploy\nimage: nginx:latest\nquantity: 100m\ndate: 2026-05-23\n")
            .expect("parse core schema fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("push"));
    assert_eq!(entries[1].0.as_str(), Some("yes"));
    assert_eq!(entries[1].1.as_str(), Some("deploy"));
    assert_eq!(entries[2].1.as_str(), Some("nginx:latest"));
    assert_eq!(entries[3].1.as_str(), Some("100m"));
    assert_eq!(entries[4].1.as_str(), Some("2026-05-23"));
}

#[test]
fn compatibility_yaml_11_schema_is_explicit_not_directive_implicit() {
    let input = "%YAML 1.1\n---\nflag: ON\ncount: 0x10\noctal: 0123\nclock: 1:20\n";
    let default_doc = parse_str(input).expect("default schema accepts YAML 1.1 directive");
    let default_entries = mapping_entries(&default_doc);
    assert_eq!(default_entries[0].1.as_str(), Some("ON"));
    assert_eq!(default_entries[1].1.as_str(), Some("0x10"));
    assert_eq!(
        saneyaml::Value::from(&default_entries[2].1).as_i64(),
        Some(123)
    );
    assert_eq!(default_entries[3].1.as_str(), Some("1:20"));

    let legacy_doc = LoadOptions::yaml_1_1()
        .parse_str(input)
        .expect("explicit YAML 1.1 schema parses");
    let legacy_entries = mapping_entries(&legacy_doc);
    assert!(matches!(legacy_entries[0].1.value, Value::Bool(true)));
    assert!(matches!(legacy_entries[1].1.value, Value::Number(_)));
    assert_eq!(
        saneyaml::Value::from(&legacy_entries[1].1).as_i64(),
        Some(16)
    );
    assert_eq!(
        saneyaml::Value::from(&legacy_entries[2].1).as_i64(),
        Some(83)
    );
    assert_eq!(
        saneyaml::Value::from(&legacy_entries[3].1).as_i64(),
        Some(80)
    );
}

#[test]
fn compatibility_yaml_11_schema_can_follow_version_directives_explicitly() {
    let input = "%YAML 1.1\n---\nflag: ON\ncount: 0x10\noctal: 0123\nclock: 1:20\n";
    let directive_doc = LoadOptions::yaml_version_directive()
        .parse_str(input)
        .expect("directive-driven YAML 1.1 schema parses");
    let directive_entries = mapping_entries(&directive_doc);
    assert!(matches!(directive_entries[0].1.value, Value::Bool(true)));
    assert_eq!(
        saneyaml::Value::from(&directive_entries[1].1).as_i64(),
        Some(16)
    );
    assert_eq!(
        saneyaml::Value::from(&directive_entries[2].1).as_i64(),
        Some(83)
    );
    assert_eq!(
        saneyaml::Value::from(&directive_entries[3].1).as_i64(),
        Some(80)
    );

    let fallback_doc = LoadOptions::yaml_version_directive()
        .parse_str("%YAML 1.2\n---\nflag: ON\ncount: 0x10\noctal: 0123\nclock: 1:20\n")
        .expect("directive-driven YAML 1.2 fallback parses");
    let fallback_entries = mapping_entries(&fallback_doc);
    assert_eq!(fallback_entries[0].1.as_str(), Some("ON"));
    assert_eq!(fallback_entries[1].1.as_str(), Some("0x10"));
    assert_eq!(
        saneyaml::Value::from(&fallback_entries[2].1).as_i64(),
        Some(123)
    );
    assert_eq!(fallback_entries[3].1.as_str(), Some("1:20"));
}

#[test]
fn compatibility_leading_utf8_bom_matches_references() {
    for (name, input, expected_docs) in [
        ("block mapping", "\u{feff}name: app\n", 1),
        ("block sequence", "\u{feff}- app\n", 1),
        ("flow mapping", "\u{feff}{name: app}\n", 1),
    ] {
        let ours = parse_documents(input)
            .unwrap_or_else(|error| panic!("ours failed {name} with leading BOM: {error}"));
        assert_eq!(ours.len(), expected_docs, "ours doc count for {name}");
        parse_events(input).unwrap_or_else(|error| {
            panic!("ours event parser failed {name} with leading BOM: {error}")
        });

        serde_yaml::from_str::<serde_yaml::Value>(input)
            .unwrap_or_else(|error| panic!("serde_yaml failed {name} with leading BOM: {error}"));
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 failed {name} with leading BOM: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr failed {name} with leading BOM: {error}"));
    }

    let input = "\u{feff}name: app\n";
    let doc = parse_str(input).expect("parse leading BOM mapping");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].0.as_str(), Some("name"));
    assert_eq!(entries[0].1.as_str(), Some("app"));
    assert_eq!(entries[0].0.span.start, input.find("name").unwrap());
    assert_eq!(
        entries[0].0.span.end,
        input.find("name").unwrap() + "name".len()
    );
}

#[test]
fn compatibility_block_scalar_chomping_values_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml");
    let doc = parse_str(input).expect("parse chomping fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some("# text"));
    assert_eq!(entries[1].1.as_str(), Some("# text\n"));
    assert_eq!(entries[2].1.as_str(), Some("# text\n\n"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses chomping fixture");
    assert_eq!(reference["strip"].as_str(), Some("# text"));
    assert_eq!(reference["clip"].as_str(), Some("# text\n"));
    assert_eq!(reference["keep"].as_str(), Some("# text\n\n"));
}

#[test]
fn compatibility_empty_block_scalar_chomping_values_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/K858/in.yaml");
    let doc = parse_str(input).expect("parse empty block scalar chomping fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some(""));
    assert_eq!(entries[1].1.as_str(), Some(""));
    assert_eq!(entries[2].1.as_str(), Some("\n"));

    let reference: serde_yaml::Value = serde_yaml::from_str(input).expect("serde_yaml parses K858");
    assert_eq!(reference["strip"].as_str(), Some(""));
    assert_eq!(reference["clip"].as_str(), Some(""));
    assert_eq!(reference["keep"].as_str(), Some("\n"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts K858");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts K858");
}

#[test]
fn compatibility_folded_block_more_indented_values_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/F6MC/in.yaml");
    let doc = parse_str(input).expect("parse folded indentation fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some(" more indented\nregular\n"));
    assert_eq!(entries[1].1.as_str(), Some("\n\n more indented\nregular\n"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses folded indentation fixture");
    assert_eq!(reference["a"].as_str(), Some(" more indented\nregular\n"));
    assert_eq!(
        reference["b"].as_str(),
        Some("\n\n more indented\nregular\n")
    );
}

#[test]
fn compatibility_block_scalar_paragraph_and_space_only_lines_match_reference_expectation() {
    let folded = include_str!("fixtures/yaml-test-suite/data/6VJK/in.yaml");
    let folded_doc = parse_str(folded).expect("parse folded paragraph fixture");
    let expected_folded = "Sammy Sosa completed another fine season with great stats.\n\n  63 Home Runs\n  0.288 Batting Average\n\nWhat a year!\n";
    assert_eq!(folded_doc.as_str(), Some(expected_folded));

    let folded_reference: serde_yaml::Value =
        serde_yaml::from_str(folded).expect("serde_yaml parses folded paragraph fixture");
    assert_eq!(folded_reference.as_str(), Some(expected_folded));
    yaml_rust2::YamlLoader::load_from_str(folded).expect("yaml-rust2 accepts 6VJK");
    saphyr::Yaml::load_from_str(folded).expect("saphyr accepts 6VJK");

    let literal = include_str!("fixtures/yaml-test-suite/data/6FWR/in.yaml");
    let literal_doc = parse_str(literal).expect("parse literal spaces-only fixture");
    let expected_literal = "ab\n\n \n";
    assert_eq!(literal_doc.as_str(), Some(expected_literal));

    let literal_reference: serde_yaml::Value =
        serde_yaml::from_str(literal).expect("serde_yaml parses literal spaces-only fixture");
    assert_eq!(literal_reference.as_str(), Some(expected_literal));
    yaml_rust2::YamlLoader::load_from_str(literal).expect("yaml-rust2 accepts 6FWR");
    saphyr::Yaml::load_from_str(literal).expect("saphyr accepts 6FWR");
}

#[test]
fn compatibility_folded_block_blank_runs_match_reference_expectation() {
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
        let doc = parse_str(input).unwrap_or_else(|error| panic!("ours parses {id}: {error}"));
        assert_eq!(doc.as_str(), Some(expected), "{id}");

        let reference: serde_yaml::Value = serde_yaml::from_str(input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {id}: {error}"));
        assert_eq!(reference.as_str(), Some(expected), "{id}");
    }

    let input = include_str!("fixtures/yaml-test-suite/data/R4YG/in.yaml");
    let doc = parse_str(input).expect("ours parses R4YG");
    let Value::Sequence(items) = doc.value else {
        panic!("expected sequence");
    };
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts R4YG");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts R4YG");
    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect_err("serde_yaml/libyaml rejects R4YG tab-leading block content");
    for (index, expected) in [
        "detected\n",
        "\n\n# detected\n",
        " explicit\n",
        "\t\ndetected\n",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(items[index].as_str(), Some(expected), "R4YG item {index}");
    }
}

#[test]
fn compatibility_block_scalar_nodes_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml");
    let doc = parse_str(input).expect("parse block scalar node fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some("value\n"));
    let Value::Tagged(tagged) = &entries[1].1.value else {
        panic!("expected tagged folded scalar");
    };
    assert_eq!(tagged.tag, saneyaml::Tag::new("foo"));
    assert_eq!(tagged.value.as_str(), Some("value\n"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses block scalar node fixture");
    assert_eq!(reference["literal"].as_str(), Some("value\n"));
    assert_eq!(reference["folded"].as_str(), Some("value\n"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts M5C3");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts M5C3");
}

#[test]
fn compatibility_plain_scalar_empty_line_values_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/36F6/in.yaml");
    let doc = parse_str(input).expect("parse plain scalar empty-line fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some("a b\nc"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses plain scalar empty-line fixture");
    assert_eq!(reference["plain"].as_str(), Some("a b\nc"));
}

#[test]
fn compatibility_directive_looking_plain_scalar_continuation_matches_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/XLQ9/in.yaml");
    let doc = parse_str(input).expect("parse directive-looking continuation fixture");
    assert_eq!(doc.as_str(), Some("scalar %YAML 1.2"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses XLQ9 fixture");
    assert_eq!(reference.as_str(), Some("scalar %YAML 1.2"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts XLQ9");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts XLQ9");
}

#[test]
fn compatibility_various_trailing_comments_match_references() {
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

        let reference: serde_yaml::Value = serde_yaml::from_str(input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {name}: {error}"));
        assert_eq!(reference["a"].as_str(), Some("double quotes"), "{name}");
        assert_eq!(reference["b"].as_str(), Some("plain value"), "{name}");
        assert_eq!(reference["c"].as_str(), Some("d"), "{name}");
        assert_eq!(reference["block"].as_str(), Some("abcde\n"), "{name}");
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
    }
}

#[test]
fn compatibility_bare_documents_match_yaml_1_2_event_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml");
    let ours = parse_documents(input).expect("ours parses bare documents");
    assert_eq!(ours.len(), 2);
    assert_eq!(ours[0].as_str(), Some("Bare document"));
    assert_eq!(
        ours[1].as_str(),
        Some("%!PS-Adobe-2.0 # Not the first line\n")
    );

    let yaml_rust_docs =
        yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses bare documents");
    assert_eq!(yaml_rust_docs.len(), 2);

    let saphyr_docs = saphyr::Yaml::load_from_str(input).expect("saphyr parses bare documents");
    assert_eq!(saphyr_docs.len(), 2);

    let serde_result = serde_yaml::Deserializer::from_str(input)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>();
    assert!(
        serde_result.is_err(),
        "serde_yaml diverges on the full M7A3 bare-document stream"
    );
}

#[test]
fn compatibility_directive_looking_flow_content_matches_rust_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/UT92/in.yaml");
    let ours = parse_documents(input).expect("ours parses explicit document stream");
    assert_eq!(ours.len(), 2);
    let Value::Mapping(entries) = &ours[0].value else {
        panic!("expected first document mapping");
    };
    assert_eq!(entries[0].0.as_str(), Some("matches %"));
    assert!(matches!(
        entries[0].1.value,
        Value::Number(saneyaml::Number::Integer(20))
    ));
    assert!(matches!(ours[1].value, Value::Null));
    parse_events(input).expect("ours parses raw events");

    let yaml_rust_docs = yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses directive-looking flow content");
    assert_eq!(yaml_rust_docs.len(), 2);
    let saphyr_docs =
        saphyr::Yaml::load_from_str(input).expect("saphyr parses directive-looking flow content");
    assert_eq!(saphyr_docs.len(), 2);
    let serde_result = serde_yaml::Deserializer::from_str(input)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>();
    assert!(
        serde_result.is_err(),
        "serde_yaml/libyaml diverges on directive-looking flow content"
    );
}

#[test]
fn compatibility_multiline_flow_scalar_values_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/4CQQ/in.yaml");
    let doc = parse_str(input).expect("parse multiline flow scalar fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(
        entries[0].1.as_str(),
        Some("This unquoted scalar spans many lines.")
    );
    assert_eq!(entries[1].1.as_str(), Some("So does this quoted scalar.\n"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses multiline flow scalars");
    assert_eq!(
        reference["plain"].as_str(),
        Some("This unquoted scalar spans many lines.")
    );
    assert_eq!(
        reference["quoted"].as_str(),
        Some("So does this quoted scalar.\n")
    );
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses multiline flow scalars");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses multiline flow scalars");
}

#[test]
fn compatibility_explicit_block_mapping_entries_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/5WE3/in.yaml");
    let doc = parse_str(input).expect("parse explicit block mapping fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("explicit key"));
    assert!(matches!(entries[0].1.value, Value::Null));
    assert_eq!(entries[1].0.as_str(), Some("block key\n"));
    let Value::Sequence(items) = &entries[1].1.value else {
        panic!("expected explicit compact sequence value");
    };
    assert_eq!(items[0].as_str(), Some("one"));
    assert_eq!(items[1].as_str(), Some("two"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses explicit block mapping fixture");
    assert!(reference["explicit key"].is_null());
    assert_eq!(reference["block key\n"][0].as_str(), Some("one"));
    assert_eq!(reference["block key\n"][1].as_str(), Some("two"));
}

#[test]
fn compatibility_compact_block_mappings_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/V9D5/in.yaml");
    let doc = parse_str(input).expect("parse compact block mapping fixture");
    let Value::Sequence(items) = &doc.value else {
        panic!("expected sequence");
    };
    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("sun"));
    assert_eq!(first[0].1.as_str(), Some("yellow"));
    assert!(matches!(second[0].0.value, Value::Mapping(_)));
    assert!(matches!(second[0].1.value, Value::Mapping(_)));

    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses compact block mappings");
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses compact block mappings");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses compact block mappings");
}

#[test]
fn compatibility_alias_nodes_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/3GZX/in.yaml");
    let doc = parse_str(input).expect("parse alias fixture");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[1].1.as_str(), Some("Foo"));
    assert_eq!(entries[3].1.as_str(), Some("Bar"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses aliases");
    assert_eq!(reference["Second occurrence"].as_str(), Some("Foo"));
    assert_eq!(reference["Reuse anchor"].as_str(), Some("Bar"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses aliases");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses aliases");
}

#[test]
fn compatibility_tagged_anchor_aliases_preserve_property_wrapped_nodes() {
    for (name, input) in [
        (
            "block value anchor before tag",
            "first: &a !Thing value\nsecond: *a\n",
        ),
        (
            "block value tag before anchor",
            "first: !Thing &a value\nsecond: *a\n",
        ),
    ] {
        let doc = parse_str(input).unwrap_or_else(|error| panic!("{name} parses: {error}"));
        let entries = mapping_entries(&doc);
        assert_tagged_scalar(&entries[0].1, "Thing", "value");
        assert_tagged_scalar(&entries[1].1, "Thing", "value");
    }

    let flow_value = parse_str("root: {first: !Thing &a value, second: *a}\n")
        .expect("parse flow tagged anchor value");
    let root = mapping_entries(&flow_value);
    let Value::Mapping(flow_entries) = &root[0].1.value else {
        panic!("expected flow root mapping");
    };
    assert_tagged_scalar(&flow_entries[0].1, "Thing", "value");
    assert_tagged_scalar(&flow_entries[1].1, "Thing", "value");

    let block_key = parse_str("root:\n  ? !Thing &a tagged-key\n  : first\nalias_value: *a\n")
        .expect("parse block tagged anchor key");
    let entries = mapping_entries(&block_key);
    let Value::Mapping(root_entries) = &entries[0].1.value else {
        panic!("expected block root mapping");
    };
    assert_tagged_scalar(&root_entries[0].0, "Thing", "tagged-key");
    assert_tagged_scalar(&entries[1].1, "Thing", "tagged-key");

    let flow_key = parse_str("root: {? !Thing &a tagged-key : first, alias: *a}\n")
        .expect("parse flow tagged anchor key");
    let entries = mapping_entries(&flow_key);
    let Value::Mapping(root_entries) = &entries[0].1.value else {
        panic!("expected flow root mapping");
    };
    assert_tagged_scalar(&root_entries[0].0, "Thing", "tagged-key");
    assert_tagged_scalar(&root_entries[1].1, "Thing", "tagged-key");
}

#[test]
fn compatibility_merge_key_expands_by_default_with_alias_value() {
    let input = "defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n  name: deploy\n";
    let doc = parse_str(input).expect("parse default merge key");
    let entries = mapping_entries(&doc);
    let Value::Mapping(job) = &entries[1].1.value else {
        panic!("expected job mapping");
    };
    assert!(job.iter().all(|(key, _)| key.as_str() != Some("<<")));
    assert_eq!(job[0].0.as_str(), Some("name"));
    assert_eq!(job[0].1.as_str(), Some("deploy"));
    assert_eq!(job[1].0.as_str(), Some("retries"));
    assert!(matches!(
        job[1].1.value,
        Value::Number(saneyaml::Number::Integer(3))
    ));

    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses merge key");
    reference
        .apply_merge()
        .expect("serde_yaml applies merge key");
    assert_eq!(reference["job"]["retries"].as_i64(), Some(3));
    assert_eq!(reference["job"]["name"].as_str(), Some("deploy"));
}

#[test]
fn compatibility_merge_list_expands_by_default() {
    let input = "base1: &base1 {a: 1, b: 1, shared: first}\nbase2: &base2 {b: 2, c: 2, shared: second}\nmerged:\n  <<: [*base1, *base2]\n  b: explicit\n";
    let doc = parse_str(input).expect("parse default merge-list key");
    let entries = mapping_entries(&doc);
    let Value::Mapping(merged) = &entries[2].1.value else {
        panic!("expected merged mapping");
    };
    assert!(merged.iter().all(|(key, _)| key.as_str() != Some("<<")));
    assert_eq!(merged[0].0.as_str(), Some("b"));
    assert_eq!(merged[0].1.as_str(), Some("explicit"));
    assert_eq!(merged[1].0.as_str(), Some("a"));
    assert_eq!(merged[2].0.as_str(), Some("shared"));
    assert_eq!(merged[2].1.as_str(), Some("first"));
    assert_eq!(merged[3].0.as_str(), Some("c"));

    let mut reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml preserves merge list literally");
    reference
        .apply_merge()
        .expect("serde_yaml applies merge list");
    assert_eq!(reference["merged"]["a"].as_i64(), Some(1));
    assert_eq!(reference["merged"]["c"].as_i64(), Some(2));
    assert_eq!(reference["merged"]["shared"].as_str(), Some("first"));
    assert_eq!(reference["merged"]["b"].as_str(), Some("explicit"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses merge list");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses merge list");
}

#[test]
fn compatibility_flow_anchor_alias_values_match_reference_expectation() {
    let input = "base: &base {image: nginx}\nrefs: [*base]\nsvc: {web: *base}\n";
    let doc = parse_str(input).expect("parse flow alias values");
    let entries = mapping_entries(&doc);
    let Value::Sequence(refs) = &entries[1].1.value else {
        panic!("expected refs sequence");
    };
    let Value::Mapping(first_ref) = &refs[0].value else {
        panic!("expected aliased mapping in sequence");
    };
    assert_eq!(first_ref[0].0.as_str(), Some("image"));
    assert_eq!(first_ref[0].1.as_str(), Some("nginx"));

    let Value::Mapping(svc) = &entries[2].1.value else {
        panic!("expected svc mapping");
    };
    let Value::Mapping(web) = &svc[0].1.value else {
        panic!("expected aliased mapping in flow mapping");
    };
    assert_eq!(web[0].1.as_str(), Some("nginx"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses flow alias values");
    assert_eq!(reference["refs"][0]["image"].as_str(), Some("nginx"));
    assert_eq!(reference["svc"]["web"]["image"].as_str(), Some("nginx"));
}

#[test]
fn compatibility_flow_anchor_only_null_nodes_match_reference_expectation() {
    let input = "root: [&empty, *empty]\n";
    let doc = parse_str(input).expect("parse anchor-only flow nodes");
    let entries = mapping_entries(&doc);
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected root sequence");
    };
    assert!(matches!(items[0].value, Value::Null));
    assert!(matches!(items[1].value, Value::Null));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses anchor-only flow nodes");
    assert!(reference["root"][0].is_null());
    assert!(reference["root"][1].is_null());
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses anchor-only flow nodes");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses anchor-only flow nodes");
}

#[test]
fn compatibility_flow_anchor_alias_colon_names_match_reference_expectation() {
    let input = "root: [&a:, *a:]\n";
    let doc = parse_str(input).expect("parse colon anchor names in flow sequence");
    let entries = mapping_entries(&doc);
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected root sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0].value, Value::Null));
    assert!(matches!(items[1].value, Value::Null));

    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts colon anchor names");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts colon anchor names");
}

#[test]
fn compatibility_block_anchor_alias_colon_names_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/2SXE/in.yaml");
    let doc = parse_str(input).expect("parse block colon anchor names");
    let entries = mapping_entries(&doc);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.as_str(), Some("key"));
    assert_eq!(entries[0].1.as_str(), Some("value"));
    assert_eq!(entries[1].0.as_str(), Some("foo"));
    assert_eq!(entries[1].1.as_str(), Some("key"));

    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 accepts block colon anchor names");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts block colon anchor names");
}

#[test]
fn compatibility_anchors_on_empty_scalars_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml");
    let docs = parse_documents(input).expect("parse anchors on empty scalars");
    assert_eq!(docs.len(), 1);

    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 accepts anchors on empty scalars");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts anchors on empty scalars");
}

#[test]
fn compatibility_flow_sequence_implicit_mapping_entries_match_reference_expectation() {
    let input = "root: [a: b, c: d]\n";
    let doc = parse_str(input).expect("parse flow sequence implicit mappings");
    let entries = mapping_entries(&doc);
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected sequence");
    };
    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first sequence item mapping");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second sequence item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("a"));
    assert_eq!(first[0].1.as_str(), Some("b"));
    assert_eq!(second[0].0.as_str(), Some("c"));
    assert_eq!(second[0].1.as_str(), Some("d"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses flow sequence implicit mappings");
    assert_eq!(reference["root"][0]["a"].as_str(), Some("b"));
    assert_eq!(reference["root"][1]["c"].as_str(), Some("d"));
}

#[test]
fn compatibility_single_pair_implicit_entries_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/9MMW/in.yaml");
    parse_str(input).expect("ours parses single pair implicit entries");
    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses single pair implicit entries");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses single pair implicit entries");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses single pair implicit entries");
}

#[test]
fn compatibility_flow_sequence_implicit_mapping_explicit_and_collection_keys_match_reference_expectation()
 {
    let input = "root: [? a: b, ? [c, d]: e, [f, g]: h]\n";
    let doc = parse_str(input).expect("parse flow sequence implicit mapping keys");
    let entries = mapping_entries(&doc);
    let Value::Sequence(items) = &entries[0].1.value else {
        panic!("expected root sequence");
    };
    let Value::Mapping(first) = &items[0].value else {
        panic!("expected first item mapping");
    };
    assert_eq!(first[0].0.as_str(), Some("a"));
    assert_eq!(first[0].1.as_str(), Some("b"));

    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert!(matches!(second[0].0.value, Value::Sequence(_)));
    assert_eq!(second[0].1.as_str(), Some("e"));

    let Value::Mapping(third) = &items[2].value else {
        panic!("expected third item mapping");
    };
    assert!(matches!(third[0].0.value, Value::Sequence(_)));
    assert_eq!(third[0].1.as_str(), Some("h"));

    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses flow sequence implicit mapping collection keys");
}

#[test]
fn compatibility_multiline_flow_sequence_with_comment_matches_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/7TMG/in.yaml");
    let doc = parse_str(input).expect("parse multiline flow sequence with comment");
    let Value::Sequence(items) = doc.value else {
        panic!("expected sequence");
    };
    assert_eq!(items[0].as_str(), Some("word1"));
    assert_eq!(items[1].as_str(), Some("word2"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses multiline flow sequence");
    assert_eq!(reference[0].as_str(), Some("word1"));
    assert_eq!(reference[1].as_str(), Some("word2"));
}

#[test]
fn compatibility_multiline_quoted_flow_key_matches_yaml_1_2_rust_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/9SA2/in.yaml");
    let doc = parse_str(input).expect("parse multiline quoted flow key");
    let Value::Sequence(items) = &doc.value else {
        panic!("expected sequence");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(second[0].0.as_str(), Some("multi line"));
    assert_eq!(second[0].1.as_str(), Some("value"));

    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses multiline quoted flow key");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses multiline quoted flow key");
    assert!(
        serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
        "serde_yaml/libyaml rejects this YAML 1.2 fixture today"
    );
}

#[test]
fn compatibility_multiline_flow_mapping_matches_reference_expectation() {
    let input = "root: { a: b\n, c: d }\n";
    let doc = parse_str(input).expect("parse multiline flow mapping");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert_eq!(root[0].1.as_str(), Some("b"));
    assert_eq!(root[1].0.as_str(), Some("c"));
    assert_eq!(root[1].1.as_str(), Some("d"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses multiline flow mapping");
    assert_eq!(reference["root"]["a"].as_str(), Some("b"));
    assert_eq!(reference["root"]["c"].as_str(), Some("d"));
}

#[test]
fn compatibility_multiline_plain_flow_scalars_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml");
    let doc = parse_str(input).expect("parse multiline plain flow key");
    let Value::Sequence(items) = &doc.value else {
        panic!("expected sequence");
    };
    let Value::Mapping(second) = &items[1].value else {
        panic!("expected second item mapping");
    };
    assert_eq!(second[0].0.as_str(), Some("multi line"));
    assert!(matches!(second[0].1.value, Value::Null));

    let value_input = "root: { a: multi\n  line, b: c }\n";
    let value_doc = parse_str(value_input).expect("parse multiline plain flow value");
    let entries = mapping_entries(&value_doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert_eq!(root[0].1.as_str(), Some("multi line"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses multiline plain flow key");
    assert!(reference[1]["multi line"].is_null());
    let value_reference: serde_yaml::Value =
        serde_yaml::from_str(value_input).expect("serde_yaml parses multiline plain flow value");
    assert_eq!(value_reference["root"]["a"].as_str(), Some("multi line"));
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses multiline plain flow key");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses multiline plain flow key");
}

#[test]
fn compatibility_adjacent_flow_mapping_scalars_follow_yaml_1_2_rust_references() {
    for (name, input) in [
        (
            "5MUD",
            include_str!("fixtures/yaml-test-suite/data/5MUD/in.yaml"),
        ),
        (
            "5T43",
            include_str!("fixtures/yaml-test-suite/data/5T43/in.yaml"),
        ),
        (
            "58MP",
            include_str!("fixtures/yaml-test-suite/data/58MP/in.yaml"),
        ),
    ] {
        parse_str(input).unwrap_or_else(|error| panic!("ours parses {name}: {error}"));
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
        assert!(
            serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
            "serde_yaml/libyaml should reject adjacent-flow-scalar divergence {name}"
        );
    }
}

#[test]
fn compatibility_zero_indented_document_start_block_scalars_follow_rust_references() {
    for (name, input, docs) in [
        (
            "W4TN",
            include_str!("fixtures/yaml-test-suite/data/W4TN/in.yaml"),
            2,
        ),
        (
            "FP8R",
            include_str!("fixtures/yaml-test-suite/data/FP8R/in.yaml"),
            1,
        ),
        (
            "DK3J",
            include_str!("fixtures/yaml-test-suite/data/DK3J/in.yaml"),
            1,
        ),
    ] {
        let ours = parse_documents(input)
            .unwrap_or_else(|error| panic!("ours parses YAML 1.2 fixture {name}: {error}"));
        assert_eq!(ours.len(), docs, "ours doc count for {name}");
        parse_events(input).unwrap_or_else(|error| panic!("ours events parse {name}: {error}"));

        let yaml_rust_docs = yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        assert_eq!(
            yaml_rust_docs.len(),
            docs,
            "yaml-rust2 doc count for {name}"
        );
        let saphyr_docs = saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
        assert_eq!(saphyr_docs.len(), docs, "saphyr doc count for {name}");

        assert!(
            serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
            "serde_yaml/libyaml rejects zero-indented document-start block scalar {name}"
        );
    }
}

#[test]
fn compatibility_recent_invalid_suite_rejections_match_reference_crates() {
    for (name, input) in [
        (
            "ZXT5",
            include_str!("fixtures/yaml-test-suite/data/ZXT5/in.yaml"),
        ),
        (
            "236B",
            include_str!("fixtures/yaml-test-suite/data/236B/in.yaml"),
        ),
        (
            "5LLU",
            include_str!("fixtures/yaml-test-suite/data/5LLU/in.yaml"),
        ),
    ] {
        assert!(parse_str(input).is_err(), "ours rejects YAML-suite {name}");
        assert!(
            parse_events(input).is_err(),
            "event parser rejects YAML-suite {name}"
        );
        assert!(
            serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
            "serde_yaml rejects YAML-suite {name}"
        );
        assert!(
            yaml_rust2::YamlLoader::load_from_str(input).is_err(),
            "yaml-rust2 rejects YAML-suite {name}"
        );
        assert!(
            saphyr::Yaml::load_from_str(input).is_err(),
            "saphyr rejects YAML-suite {name}"
        );
    }
}

#[test]
fn compatibility_flow_indentation_rejections_match_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/9C9N/in.yaml");
    let error = parse_str(input).expect_err("ours rejects wrong indented flow sequence");
    assert!(
        error.to_string().contains("sufficiently indented"),
        "ours reports indentation error: {error}"
    );
    assert!(
        error.location().is_some(),
        "ours preserves diagnostic location"
    );
    parse_events(input).expect_err("event parser rejects wrong indented flow sequence");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect_err("yaml-rust2 rejects wrong indented flow sequence");
    saphyr::Yaml::load_from_str(input).expect_err("saphyr rejects wrong indented flow sequence");
}

#[test]
fn compatibility_double_quoted_mapping_value_trailing_content_rejections_match_references() {
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
        let error = parse_str(input).expect_err("ours rejects YAML-suite trailing content");
        assert!(
            error
                .to_string()
                .contains("unexpected trailing characters after quoted scalar"),
            "{name}: {error}"
        );
        parse_events(input).expect_err("event parser rejects trailing content");
        serde_yaml::from_str::<serde_yaml::Value>(input)
            .expect_err("serde_yaml rejects YAML-suite trailing content");
        yaml_rust2::YamlLoader::load_from_str(input)
            .expect_err("yaml-rust2 rejects YAML-suite trailing content");
        saphyr::Yaml::load_from_str(input).expect_err("saphyr rejects YAML-suite trailing content");
    }
}

#[test]
fn compatibility_multiline_quoted_scalar_indentation_rejections_match_yaml_rust2_saphyr() {
    for (name, input, expected, serde_yaml_accepts) in [
        (
            "QB6E",
            include_str!("fixtures/yaml-test-suite/data/QB6E/in.yaml"),
            "multiline quoted scalar continuation is not sufficiently indented",
            true,
        ),
        (
            "DK95/01",
            include_str!("fixtures/yaml-test-suite/data/DK95-01/in.yaml"),
            "tabs are not allowed for indentation",
            true,
        ),
        (
            "DK95/06",
            include_str!("fixtures/yaml-test-suite/data/DK95-06/in.yaml"),
            "tabs are not allowed for indentation",
            false,
        ),
    ] {
        let error =
            parse_str(input).expect_err("ours rejects YAML-suite quoted scalar indentation");
        assert!(error.to_string().contains(expected), "{name}: {error}");
        assert!(
            error.location().is_some(),
            "{name}: ours preserves diagnostic location"
        );
        parse_events(input).expect_err("event parser rejects quoted scalar indentation");
        let serde_yaml_result = serde_yaml::from_str::<serde_yaml::Value>(input);
        if serde_yaml_accepts {
            assert!(
                serde_yaml_result.is_ok(),
                "serde_yaml/libyaml accepts this YAML-suite invalid input for {name}: {serde_yaml_result:?}"
            );
        } else {
            assert!(
                serde_yaml_result.is_err(),
                "serde_yaml/libyaml rejects this YAML-suite invalid input for {name}"
            );
        }
        yaml_rust2::YamlLoader::load_from_str(input)
            .expect_err("yaml-rust2 rejects YAML-suite quoted scalar indentation");
        saphyr::Yaml::load_from_str(input)
            .expect_err("saphyr rejects YAML-suite quoted scalar indentation");
    }
}

#[test]
fn compatibility_block_scalar_tab_rejections_match_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y/in.yaml");
    let error = parse_str(input).expect_err("ours rejects tab-starting block scalar content");
    assert!(
        error
            .to_string()
            .contains("block scalar content cannot start with a tab"),
        "ours reports block scalar tab error: {error}"
    );
    assert!(
        error.location().is_some(),
        "ours preserves diagnostic location"
    );
    parse_events(input).expect_err("event parser rejects tab-starting block scalar content");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect_err("yaml-rust2 rejects tab-starting block scalar content");
    saphyr::Yaml::load_from_str(input)
        .expect_err("saphyr rejects tab-starting block scalar content");
}

#[test]
fn compatibility_space_tab_block_scalar_content_matches_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-001/in.yaml");
    parse_str(input).expect("ours accepts space-tab block scalar content");
    parse_events(input).expect("event parser accepts space-tab block scalar content");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 accepts space-tab block scalar content");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts space-tab block scalar content");
}

#[test]
fn compatibility_tab_only_flow_sequence_separation_matches_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-002/in.yaml");
    parse_str(input).expect("ours accepts tab-only flow sequence separation");
    parse_events(input).expect("event parser accepts tab-only flow sequence separation");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 accepts tab-only flow sequence separation");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts tab-only flow sequence separation");
}

#[test]
fn compatibility_root_tab_flow_collections_match_rust_references() {
    for (name, input) in [
        (
            "6CA3",
            include_str!("fixtures/yaml-test-suite/data/6CA3/in.yaml"),
        ),
        (
            "Q5MG",
            include_str!("fixtures/yaml-test-suite/data/Q5MG/in.yaml"),
        ),
    ] {
        parse_str(input).unwrap_or_else(|error| panic!("ours accepts {name}: {error}"));
        parse_events(input).unwrap_or_else(|error| panic!("event parser accepts {name}: {error}"));
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 accepts {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr accepts {name}: {error}"));
        assert!(
            serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
            "serde_yaml/libyaml rejects YAML 1.2 root tab flow fixture {name}"
        );
    }
}

#[test]
fn compatibility_separation_tabs_match_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/6BCT/in.yaml");
    parse_str(input).expect("ours accepts tab separation");
    parse_events(input).expect("event parser accepts tab separation");
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 accepts tab separation");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts tab separation");
    assert!(
        serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
        "serde_yaml/libyaml rejects YAML 1.2 tab separation fixture 6BCT"
    );
}

#[test]
fn compatibility_dk95_tab_looking_indentation_matches_rust_references() {
    for (name, input, serde_yaml_accepts) in [
        (
            "DK95/00",
            include_str!("fixtures/yaml-test-suite/data/DK95-00/in.yaml"),
            false,
        ),
        (
            "DK95/02",
            include_str!("fixtures/yaml-test-suite/data/DK95-02/in.yaml"),
            true,
        ),
        (
            "DK95/03",
            include_str!("fixtures/yaml-test-suite/data/DK95-03/in.yaml"),
            false,
        ),
        (
            "DK95/04",
            include_str!("fixtures/yaml-test-suite/data/DK95-04/in.yaml"),
            false,
        ),
        (
            "DK95/05",
            include_str!("fixtures/yaml-test-suite/data/DK95-05/in.yaml"),
            true,
        ),
        (
            "DK95/07",
            include_str!("fixtures/yaml-test-suite/data/DK95-07/in.yaml"),
            true,
        ),
        (
            "DK95/08",
            include_str!("fixtures/yaml-test-suite/data/DK95-08/in.yaml"),
            true,
        ),
    ] {
        parse_str(input).unwrap_or_else(|error| panic!("ours accepts {name}: {error}"));
        parse_events(input).unwrap_or_else(|error| panic!("event parser accepts {name}: {error}"));
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 accepts {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr accepts {name}: {error}"));
        assert_eq!(
            serde_yaml::from_str::<serde_yaml::Value>(input).is_ok(),
            serde_yaml_accepts,
            "serde_yaml/libyaml acceptance for YAML 1.2 DK95 tab fixture {name}"
        );
    }
}

#[test]
fn compatibility_tab_separated_negative_scalar_matches_yaml_1_2_rust_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/Y79Y-010/in.yaml");
    parse_str(input).expect("ours accepts tab-separated negative scalar");
    parse_events(input).expect("event parser accepts tab-separated negative scalar");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 accepts tab-separated negative scalar");
    saphyr::Yaml::load_from_str(input).expect("saphyr accepts tab-separated negative scalar");
}

#[test]
fn compatibility_tab_separation_indicator_rejections_match_reference_crates() {
    for (name, input) in [
        (
            "Y79Y-004",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-004/in.yaml"),
        ),
        (
            "Y79Y-005",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-005/in.yaml"),
        ),
        (
            "Y79Y-006",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-006/in.yaml"),
        ),
        (
            "Y79Y-008",
            include_str!("fixtures/yaml-test-suite/data/Y79Y-008/in.yaml"),
        ),
    ] {
        let error = match parse_str(input) {
            Ok(_) => panic!("ours rejects tab separation for {name}"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("tabs are not allowed as separation after block indicators"),
            "{name}: {error}"
        );
        assert!(
            error.location().is_some(),
            "ours preserves diagnostic location for {name}"
        );
        if parse_events(input).is_ok() {
            panic!("event parser rejects {name}");
        }
        if yaml_rust2::YamlLoader::load_from_str(input).is_ok() {
            panic!("yaml-rust2 rejects {name}");
        }
        if saphyr::Yaml::load_from_str(input).is_ok() {
            panic!("saphyr rejects {name}");
        }
    }
}

#[test]
fn compatibility_flow_plain_dash_rejections_match_reference_crates() {
    let input = include_str!("fixtures/yaml-test-suite/data/YJV2/in.yaml");
    let error = parse_str(input).expect_err("ours rejects dash flow entry");
    assert!(
        error.to_string().contains("plain scalar cannot start"),
        "ours reports dash flow error: {error}"
    );
    assert!(
        error.location().is_some(),
        "ours preserves diagnostic location"
    );
    parse_events(input).expect_err("event parser rejects dash flow entry");
    yaml_rust2::YamlLoader::load_from_str(input).expect_err("yaml-rust2 rejects dash flow entry");
    saphyr::Yaml::load_from_str(input).expect_err("saphyr rejects dash flow entry");

    for valid in ["[-1]\n", "[-foo]\n", "[--flag]\n"] {
        parse_str(valid).unwrap_or_else(|error| panic!("ours accepts {valid:?}: {error}"));
        yaml_rust2::YamlLoader::load_from_str(valid)
            .unwrap_or_else(|error| panic!("yaml-rust2 accepts {valid:?}: {error}"));
        saphyr::Yaml::load_from_str(valid)
            .unwrap_or_else(|error| panic!("saphyr accepts {valid:?}: {error}"));
    }

    let strict = include_str!("fixtures/yaml-test-suite/data/G5U8/in.yaml");
    parse_str(strict).expect_err("ours rejects YAML-suite G5U8 dash flow entries");
    parse_events(strict).expect_err("event parser rejects YAML-suite G5U8");
    yaml_rust2::YamlLoader::load_from_str(strict).expect_err("yaml-rust2 rejects G5U8");
    saphyr::Yaml::load_from_str(strict).expect_err("saphyr rejects G5U8");
    serde_yaml::from_str::<serde_yaml::Value>(strict)
        .expect("serde_yaml/libyaml tolerates YAML-suite G5U8");
}

#[test]
fn compatibility_block_scalar_indentation_rejections_match_reference_policy() {
    for (name, input) in [
        (
            "S98Z",
            include_str!("fixtures/yaml-test-suite/data/S98Z/in.yaml"),
        ),
        (
            "W9L4",
            include_str!("fixtures/yaml-test-suite/data/W9L4/in.yaml"),
        ),
    ] {
        assert!(
            parse_str(input).is_err(),
            "ours rejects malformed block scalar indentation for {name}"
        );
        assert!(
            parse_events(input).is_err(),
            "event parser rejects malformed block scalar indentation for {name}"
        );
        assert!(
            yaml_rust2::YamlLoader::load_from_str(input).is_err(),
            "yaml-rust2 rejects malformed block scalar indentation for {name}"
        );
        assert!(
            saphyr::Yaml::load_from_str(input).is_err(),
            "saphyr rejects malformed block scalar indentation for {name}"
        );
    }

    serde_yaml::from_str::<serde_yaml::Value>(include_str!(
        "fixtures/yaml-test-suite/data/S98Z/in.yaml"
    ))
    .expect("serde_yaml/libyaml tolerates YAML-suite S98Z");
    assert!(
        serde_yaml::from_str::<serde_yaml::Value>(include_str!(
            "fixtures/yaml-test-suite/data/W9L4/in.yaml"
        ))
        .is_err(),
        "serde_yaml/libyaml rejects YAML-suite W9L4"
    );
}

#[test]
fn compatibility_scalar_comment_separation_rejections_match_reference_policy() {
    for (name, input) in [
        (
            "SU5Z",
            include_str!("fixtures/yaml-test-suite/data/SU5Z/in.yaml"),
        ),
        (
            "X4QW",
            include_str!("fixtures/yaml-test-suite/data/X4QW/in.yaml"),
        ),
    ] {
        assert!(
            parse_str(input).is_err(),
            "ours rejects adjacent comment-looking text for {name}"
        );
        assert!(
            parse_events(input).is_err(),
            "event parser rejects adjacent comment-looking text for {name}"
        );
        assert!(
            yaml_rust2::YamlLoader::load_from_str(input).is_err(),
            "yaml-rust2 rejects adjacent comment-looking text for {name}"
        );
        assert!(
            saphyr::Yaml::load_from_str(input).is_err(),
            "saphyr rejects adjacent comment-looking text for {name}"
        );
        serde_yaml::from_str::<serde_yaml::Value>(input)
            .unwrap_or_else(|error| panic!("serde_yaml/libyaml tolerates {name}: {error}"));
    }
}

#[test]
fn compatibility_reserved_directives_match_yaml_1_2_rust_references() {
    let input = include_str!("fixtures/yaml-test-suite/data/6LVF/in.yaml");
    let doc = parse_str(input).expect("ours ignores reserved directive");
    assert_eq!(doc.as_str(), Some("foo"));
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 ignores reserved directive");
    saphyr::Yaml::load_from_str(input).expect("saphyr ignores reserved directive");
    assert!(
        serde_yaml::from_str::<serde_yaml::Value>(input).is_err(),
        "serde_yaml/libyaml rejects reserved directive fixture"
    );

    let missing_start = "%FOO bar\nkey: value\n";
    let error =
        parse_str(missing_start).expect_err("reserved directive still needs document start");
    assert!(error.to_string().contains("explicit document start"));
}

#[test]
fn compatibility_yaml_version_directive_variants_match_rust_references() {
    for (name, input, expected_docs) in [
        (
            "BEC7",
            include_str!("fixtures/yaml-test-suite/data/BEC7/in.yaml"),
            1,
        ),
        (
            "MUS6/02",
            include_str!("fixtures/yaml-test-suite/data/MUS6-02/in.yaml"),
            1,
        ),
        (
            "MUS6/03",
            include_str!("fixtures/yaml-test-suite/data/MUS6-03/in.yaml"),
            1,
        ),
        (
            "MUS6/04",
            include_str!("fixtures/yaml-test-suite/data/MUS6-04/in.yaml"),
            1,
        ),
    ] {
        let ours = parse_documents(input)
            .unwrap_or_else(|error| panic!("ours parses {name} directive variant: {error}"));
        assert_eq!(ours.len(), expected_docs, "ours doc count for {name}");

        let yaml_rust2 = yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        assert_eq!(
            yaml_rust2.len(),
            expected_docs,
            "yaml-rust2 doc count for {name}"
        );

        let saphyr = saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
        assert_eq!(saphyr.len(), expected_docs, "saphyr doc count for {name}");
    }

    let schema_neutral =
        parse_str("%YAML 1.1\n---\non: off\nyes: no\n").expect("YAML 1.1 syntax metadata");
    let entries = mapping_entries(&schema_neutral);
    assert_eq!(entries[0].0.as_str(), Some("on"));
    assert_eq!(entries[0].1.as_str(), Some("off"));
    assert_eq!(entries[1].0.as_str(), Some("yes"));
    assert_eq!(entries[1].1.as_str(), Some("no"));
}

#[test]
fn compatibility_flow_blank_line_folding_matches_reference_expectation() {
    let value_input = "root: { a: first\n\n  second, b: c }\n";
    let doc = parse_str(value_input).expect("parse flow blank-line value folding");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert_eq!(root[0].1.as_str(), Some("first\nsecond"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(value_input).expect("serde_yaml parses flow blank-line value folding");
    assert_eq!(reference["root"]["a"].as_str(), Some("first\nsecond"));

    let key_input = "keys: { first\n\n  second: v, b: c }\n";
    let key_doc = parse_str(key_input).expect("parse flow blank-line key folding");
    let key_entries = mapping_entries(&key_doc);
    let Value::Mapping(keys) = &key_entries[0].1.value else {
        panic!("expected keys mapping");
    };
    assert_eq!(keys[0].0.as_str(), Some("first\nsecond"));
    assert_eq!(keys[0].1.as_str(), Some("v"));

    yaml_rust2::YamlLoader::load_from_str(key_input)
        .expect("yaml-rust2 parses flow blank-line folding");
    saphyr::Yaml::load_from_str(key_input).expect("saphyr parses flow blank-line folding");
}

#[test]
fn compatibility_5gbf_empty_lines_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/5GBF/in.yaml");
    let doc = parse_str(input).expect("parse YAML-suite 5GBF");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some("Empty line\nas a line feed"));
    assert_eq!(entries[1].1.as_str(), Some("Clipped empty lines\n"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses YAML-suite 5GBF");
    assert_eq!(
        reference["Folding"].as_str(),
        Some("Empty line\nas a line feed")
    );
    assert_eq!(
        reference["Chomping"].as_str(),
        Some("Clipped empty lines\n")
    );
    yaml_rust2::YamlLoader::load_from_str(input).expect("yaml-rust2 parses YAML-suite 5GBF");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses YAML-suite 5GBF");
}

#[test]
fn compatibility_flow_mapping_plain_keys_without_values_match_reference_expectation() {
    let input = "root: {a, b: c}\n";
    let doc = parse_str(input).expect("parse flow mapping plain keys");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert!(matches!(root[0].1.value, Value::Null));
    assert_eq!(root[1].0.as_str(), Some("b"));
    assert_eq!(root[1].1.as_str(), Some("c"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses flow mapping plain keys");
    assert!(reference["root"]["a"].is_null());
    assert_eq!(reference["root"]["b"].as_str(), Some("c"));
}

#[test]
fn compatibility_flow_mapping_explicit_scalar_keys_match_reference_expectation() {
    let input = "root: {? a: b, ? c, d: e}\n";
    let doc = parse_str(input).expect("parse flow mapping explicit scalar keys");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("a"));
    assert_eq!(root[0].1.as_str(), Some("b"));
    assert_eq!(root[1].0.as_str(), Some("c"));
    assert!(matches!(root[1].1.value, Value::Null));
    assert_eq!(root[2].0.as_str(), Some("d"));
    assert_eq!(root[2].1.as_str(), Some("e"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses flow mapping explicit scalar keys");
    assert_eq!(reference["root"]["a"].as_str(), Some("b"));
    assert!(reference["root"]["c"].is_null());
    assert_eq!(reference["root"]["d"].as_str(), Some("e"));
}

#[test]
fn compatibility_flow_mapping_collection_keys_match_reference_expectation() {
    let input = "root: {? [a, b]: c, ? {d: e}: f}\n";
    let doc = parse_str(input).expect("parse flow mapping collection keys");
    let entries = mapping_entries(&doc);
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

    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses flow mapping collection keys");
}

#[test]
fn compatibility_tagged_block_collection_nodes_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml");
    let doc = parse_str(input).expect("parse tagged block collection nodes");
    let entries = mapping_entries(&doc);
    let Value::Tagged(sequence) = &entries[0].1.value else {
        panic!("expected tagged sequence");
    };
    assert_eq!(sequence.tag, saneyaml::Tag::new("!!seq"));
    let Value::Sequence(items) = &sequence.value.value else {
        panic!("expected sequence value");
    };
    assert_eq!(items[0].as_str(), Some("entry"));

    let Value::Tagged(mapping) = &entries[1].1.value else {
        panic!("expected tagged mapping");
    };
    assert_eq!(mapping.tag, saneyaml::Tag::new("!!map"));
    let Value::Mapping(mapping_entries) = &mapping.value.value else {
        panic!("expected mapping value");
    };
    assert_eq!(mapping_entries[0].1.as_str(), Some("bar"));

    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses tagged block collection nodes");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses tagged block collection nodes");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses tagged block collection nodes");
}

#[test]
fn compatibility_flow_mapping_key_metadata_matches_reference_expectation() {
    let input = "key: &key alias-key\nroot: {&direct direct-key: v, ? *key : alias-v, ? &seq [a, b] : seq-v, !Thing tagged-key: tagged-v}\n";
    let doc = parse_str(input).expect("parse flow mapping key metadata");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[1].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("direct-key"));
    assert_eq!(root[0].1.as_str(), Some("v"));
    assert_eq!(root[1].0.as_str(), Some("alias-key"));
    assert_eq!(root[1].1.as_str(), Some("alias-v"));
    let Value::Sequence(seq_key) = &root[2].0.value else {
        panic!("expected anchored sequence key");
    };
    assert_eq!(seq_key[0].as_str(), Some("a"));
    assert_eq!(seq_key[1].as_str(), Some("b"));
    let Value::Tagged(tagged_key) = &root[3].0.value else {
        panic!("expected tagged scalar key");
    };
    assert_eq!(tagged_key.tag, saneyaml::Tag::new("Thing"));
    assert_eq!(tagged_key.value.as_str(), Some("tagged-key"));
    assert_eq!(root[3].1.as_str(), Some("tagged-v"));

    serde_yaml::from_str::<serde_yaml::Value>(input)
        .expect("serde_yaml parses flow mapping key metadata");
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses flow mapping key metadata");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses flow mapping key metadata");
}

#[test]
fn compatibility_flow_mapping_plain_url_key_matches_reference_expectation() {
    let input = "root: {http://example.com: value}\n";
    let doc = parse_str(input).expect("parse flow mapping URL key");
    let entries = mapping_entries(&doc);
    let Value::Mapping(root) = &entries[0].1.value else {
        panic!("expected root mapping");
    };
    assert_eq!(root[0].0.as_str(), Some("http://example.com"));
    assert_eq!(root[0].1.as_str(), Some("value"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses flow mapping URL key");
    assert_eq!(
        reference["root"]["http://example.com"].as_str(),
        Some("value")
    );
}

#[test]
fn compatibility_allowed_plain_key_characters_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/2EBW/in.yaml");
    let doc = parse_str(input).expect("parse allowed plain key characters");
    let entries = mapping_entries(&doc);
    assert_eq!(entries.len(), 5);
    assert_eq!(
        entries[0].0.as_str(),
        Some("a!\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~")
    );
    assert_eq!(entries[1].0.as_str(), Some("?foo"));
    assert_eq!(entries[2].0.as_str(), Some(":foo"));
    assert_eq!(entries[3].0.as_str(), Some("-foo"));
    assert_eq!(entries[4].0.as_str(), Some("this is#not"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses allowed plain key characters");
    assert_eq!(reference["?foo"].as_str(), Some("safe question mark"));
    assert_eq!(reference[":foo"].as_str(), Some("safe colon"));
    assert_eq!(reference["-foo"].as_str(), Some("safe dash"));
    assert_eq!(reference["this is#not"].as_str(), Some("a comment"));
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses allowed plain key characters");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses allowed plain key characters");
}

#[test]
fn compatibility_allowed_plain_scalar_characters_match_reference_expectation() {
    let input = include_str!("fixtures/yaml-test-suite/data/FBC9/in.yaml");
    let doc = parse_str(input).expect("parse allowed plain scalar characters");
    let entries = mapping_entries(&doc);
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries[0].1.as_str(),
        Some("a!\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~ !\"#$%&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}~")
    );
    assert_eq!(entries[1].1.as_str(), Some("?foo"));
    assert_eq!(entries[2].1.as_str(), Some(":foo"));
    assert_eq!(entries[3].1.as_str(), Some("-foo"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses allowed plain scalar characters");
    assert_eq!(reference["safe question mark"].as_str(), Some("?foo"));
    assert_eq!(reference["safe colon"].as_str(), Some(":foo"));
    assert_eq!(reference["safe dash"].as_str(), Some("-foo"));
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses allowed plain scalar characters");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses allowed plain scalar characters");
}

#[test]
fn compatibility_double_quoted_yaml_escapes_match_references() {
    let input = "x: \"\\e\"\nroot: [\"\\a\", \"\\v\", \"\\_\", \"\\N\", \"\\L\", \"\\P\"]\n";
    let doc = parse_str(input).expect("parse YAML double-quoted escapes");
    let entries = mapping_entries(&doc);
    assert_eq!(entries[0].1.as_str(), Some("\u{001B}"));
    let Value::Sequence(root) = &entries[1].1.value else {
        panic!("expected flow sequence");
    };
    assert_eq!(root[0].as_str(), Some("\u{0007}"));
    assert_eq!(root[1].as_str(), Some("\u{000B}"));
    assert_eq!(root[2].as_str(), Some("\u{00A0}"));
    assert_eq!(root[3].as_str(), Some("\u{0085}"));
    assert_eq!(root[4].as_str(), Some("\u{2028}"));
    assert_eq!(root[5].as_str(), Some("\u{2029}"));

    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses YAML double-quoted escapes");
    assert_eq!(reference["x"].as_str(), Some("\u{001B}"));
    assert_eq!(reference["root"][3].as_str(), Some("\u{0085}"));
    yaml_rust2::YamlLoader::load_from_str(input)
        .expect("yaml-rust2 parses YAML double-quoted escapes");
    saphyr::Yaml::load_from_str(input).expect("saphyr parses YAML double-quoted escapes");
}

#[test]
fn compatibility_double_quoted_tabs_and_continuations_match_references() {
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
        let ours = parse_str(input).unwrap_or_else(|error| panic!("ours parses {name}: {error}"));
        assert_eq!(ours.as_str(), Some(expected), "{name}");

        let reference: serde_yaml::Value = serde_yaml::from_str(input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {name}: {error}"));
        assert_eq!(reference.as_str(), Some(expected), "{name}");

        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
    }

    let stream = include_str!("fixtures/yaml-test-suite/data/KSS4/in.yaml");
    assert_eq!(
        parse_documents(stream)
            .expect("ours parses KSS4 stream")
            .len(),
        2
    );
    assert_eq!(serde_yaml::Deserializer::from_str(stream).count(), 2);
    assert_eq!(
        yaml_rust2::YamlLoader::load_from_str(stream)
            .expect("yaml-rust2 parses KSS4")
            .len(),
        2
    );
    assert_eq!(
        saphyr::Yaml::load_from_str(stream)
            .expect("saphyr parses KSS4")
            .len(),
        2
    );
}

#[test]
fn compatibility_double_quoted_even_backslash_folds_match_references() {
    for (name, input, expected) in [
        ("even-two-backslashes", "value: \"a\\\\\n  b\"\n", "a\\ b"),
        (
            "even-four-backslashes",
            "value: \"a\\\\\\\\\n  b\"\n",
            "a\\\\ b",
        ),
        ("odd-one-backslash", "value: \"a\\\n  b\"\n", "ab"),
    ] {
        let ours = parse_str(input).unwrap_or_else(|error| panic!("ours parses {name}: {error}"));
        let entries = mapping_entries(&ours);
        assert_eq!(entries[0].1.as_str(), Some(expected), "{name}");

        let reference: serde_yaml::Value = serde_yaml::from_str(input)
            .unwrap_or_else(|error| panic!("serde_yaml parses {name}: {error}"));
        assert_eq!(reference["value"].as_str(), Some(expected), "{name}");
        yaml_rust2::YamlLoader::load_from_str(input)
            .unwrap_or_else(|error| panic!("yaml-rust2 parses {name}: {error}"));
        saphyr::Yaml::load_from_str(input)
            .unwrap_or_else(|error| panic!("saphyr parses {name}: {error}"));
    }
}

#[test]
fn compatibility_kubernetes_crd_openapi_schema_paths_match_reference_expectation() {
    let input = include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml");
    let ours: Vec<saneyaml::Value> =
        saneyaml::from_documents_str(input).expect("deserialize CRD stream");
    let reference = serde_yaml::Deserializer::from_str(input)
        .map(serde_yaml::Value::deserialize)
        .collect::<Result<Vec<_>, _>>()
        .expect("serde_yaml parses CRD stream");

    assert_eq!(ours.len(), 2);
    assert_eq!(reference.len(), 2);
    assert_eq!(ours[0]["kind"].as_str(), reference[0]["kind"].as_str());
    assert_eq!(
        ours[0]["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["required"][0].as_str(),
        reference[0]["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["required"][0].as_str()
    );
    assert_eq!(
        ours[0]["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"]
            ["properties"]["rules"]["x-kubernetes-list-map-keys"][0]
            .as_str(),
        reference[0]["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"]
            ["properties"]["rules"]["x-kubernetes-list-map-keys"][0]
            .as_str()
    );
    assert_eq!(
        ours[1]["spec"]["config"]["LOG_LEVEL"].as_str(),
        reference[1]["spec"]["config"]["LOG_LEVEL"].as_str()
    );
}

#[test]
fn compatibility_openapi_dynamic_value_paths_match_reference_expectation() {
    let input = include_str!("fixtures/real-world/openapi/petstore-fragment.yaml");
    let ours: saneyaml::Value = saneyaml::from_str(input).expect("deserialize openapi value");
    let reference: serde_yaml::Value =
        serde_yaml::from_str(input).expect("serde_yaml parses openapi value");

    assert_eq!(
        ours["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["items"]["$ref"]
            .as_str(),
        reference["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["items"]["$ref"]
            .as_str()
    );
    assert_eq!(
        ours["components"]["schemas"]["Pet"]["required"][1].as_str(),
        reference["components"]["schemas"]["Pet"]["required"][1].as_str()
    );
}

#[test]
fn compatibility_intentional_rejections_are_explicit() {
    let (name, input, expected) = ("duplicate_keys", "a: 1\na: 2\n", "duplicate mapping key");
    let error = parse_str(input).unwrap_err();
    assert!(
        error.to_string().contains(expected),
        "{name} error `{error}` should mention `{expected}`"
    );
    assert!(
        error.location().is_some(),
        "{name} should preserve diagnostic location"
    );
}

fn assert_various_trailing_comments_tree(name: &str, doc: &Node) {
    let entries = mapping_entries(doc);
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

    let Value::Sequence(e_items) = &entries[4].1.value else {
        panic!("{name}: expected anchored sequence value");
    };
    let Value::Mapping(e_mapping) = &e_items[0].value else {
        panic!("{name}: expected mapping item in anchored sequence");
    };
    assert_eq!(e_mapping[0].0.as_str(), Some("x"), "{name}");
    assert_eq!(e_mapping[0].1.as_str(), Some("y"), "{name}");
    assert_eq!(entries[5].1.as_str(), Some("abcde\n"), "{name}");
}

fn mapping_entries(node: &Node) -> &[(Node, Node)] {
    let Value::Mapping(entries) = &node.value else {
        panic!("expected mapping");
    };
    entries
}

fn assert_tagged_scalar(node: &Node, tag: &str, value: &str) {
    let Value::Tagged(tagged) = &node.value else {
        panic!("expected tagged scalar node");
    };
    assert_eq!(tagged.tag, saneyaml::Tag::new(tag));
    assert_eq!(tagged.value.as_str(), Some(value));
}
