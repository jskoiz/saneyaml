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
        name: "flow_mapping_key_metadata",
        input: "key: &key alias-key\nroot: {&direct direct-key: v, ? *key : alias-v, ? &seq [a, b] : seq-v, !Thing tagged-key: tagged-v}\n",
    },
    TreeCase {
        name: "yts_bu8l_node_anchor_and_tag_on_separate_lines",
        input: include_str!("fixtures/yaml-test-suite/data/BU8L/in.yaml"),
    },
    TreeCase {
        name: "yts_9kax_tag_anchor_property_combinations",
        input: include_str!("fixtures/yaml-test-suite/data/9KAX/in.yaml"),
    },
    TreeCase {
        name: "yts_3r3p_single_block_sequence_with_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/3R3P/in.yaml"),
    },
    TreeCase {
        name: "yts_7bmt_node_and_mapping_key_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/7BMT/in.yaml"),
    },
    TreeCase {
        name: "yts_7bub_spec_node_referenced_by_alias",
        input: include_str!("fixtures/yaml-test-suite/data/7BUB/in.yaml"),
    },
    TreeCase {
        name: "yts_cn3r_flow_sequence_anchor_locations",
        input: include_str!("fixtures/yaml-test-suite/data/CN3R/in.yaml"),
    },
    TreeCase {
        name: "yts_cup7_node_property_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/CUP7/in.yaml"),
    },
    TreeCase {
        name: "yts_e76z_aliases_in_implicit_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/E76Z/in.yaml"),
    },
    TreeCase {
        name: "yts_y2gn_anchor_with_colon_in_middle",
        input: include_str!("fixtures/yaml-test-suite/data/Y2GN/in.yaml"),
    },
    TreeCase {
        name: "yts_zwk4_anchor_after_missing_explicit_value",
        input: include_str!("fixtures/yaml-test-suite/data/ZWK4/in.yaml"),
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
        name: "yts_k858_empty_block_scalar_chomping",
        input: include_str!("fixtures/yaml-test-suite/data/K858/in.yaml"),
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
        name: "yts_m7a3_bare_documents",
        input: include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml"),
    },
    TreeCase {
        name: "yts_57h4_tagged_block_collections",
        input: include_str!("fixtures/yaml-test-suite/data/57H4/in.yaml"),
    },
    TreeCase {
        name: "yts_dhp8_flow_sequence_and_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/DHP8/in.yaml"),
    },
    TreeCase {
        name: "yts_7w2p_block_mapping_missing_values",
        input: include_str!("fixtures/yaml-test-suite/data/7W2P/in.yaml"),
    },
    TreeCase {
        name: "yts_5we3_explicit_block_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/5WE3/in.yaml"),
    },
    TreeCase {
        name: "yts_v9d5_compact_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/V9D5/in.yaml"),
    },
    TreeCase {
        name: "yts_s3pd_implicit_block_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/S3PD/in.yaml"),
    },
    TreeCase {
        name: "yts_cfd4_empty_implicit_flow_sequence_keys",
        input: include_str!("fixtures/yaml-test-suite/data/CFD4/in.yaml"),
    },
    TreeCase {
        name: "yts_m2n8_00_question_mark_edge_empty_compact_mapping_key",
        input: include_str!("fixtures/yaml-test-suite/data/M2N8-00/in.yaml"),
    },
    TreeCase {
        name: "yts_ukk6_00_colon_only_compact_sequence_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-00/in.yaml"),
    },
    TreeCase {
        name: "yts_ukk6_02_bare_explicit_non_specific_tag",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-02/in.yaml"),
    },
    TreeCase {
        name: "yts_2ebw_allowed_plain_key_characters",
        input: include_str!("fixtures/yaml-test-suite/data/2EBW/in.yaml"),
    },
    TreeCase {
        name: "yts_fbc9_allowed_plain_scalar_characters",
        input: include_str!("fixtures/yaml-test-suite/data/FBC9/in.yaml"),
    },
    TreeCase {
        name: "yts_xlq9_directive_looking_plain_scalar_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/XLQ9/in.yaml"),
    },
    TreeCase {
        name: "yts_ab8u_sequence_entry_looking_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/AB8U/in.yaml"),
    },
    TreeCase {
        name: "yts_3gzx_alias_nodes",
        input: include_str!("fixtures/yaml-test-suite/data/3GZX/in.yaml"),
    },
    TreeCase {
        name: "yts_u3xv_node_and_mapping_key_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/U3XV/in.yaml"),
    },
    TreeCase {
        name: "yts_2sxe_anchors_with_colon_in_name",
        input: include_str!("fixtures/yaml-test-suite/data/2SXE/in.yaml"),
    },
    TreeCase {
        name: "yts_jhb9_two_documents_with_comments",
        input: include_str!("fixtures/yaml-test-suite/data/JHB9/in.yaml"),
    },
    TreeCase {
        name: "yts_6lvf_reserved_directive_is_ignored",
        input: include_str!("fixtures/yaml-test-suite/data/6LVF/in.yaml"),
    },
    TreeCase {
        name: "yts_6bct_separation_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/6BCT/in.yaml"),
    },
    TreeCase {
        name: "yts_6ca3_tab_before_root_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/6CA3/in.yaml"),
    },
    TreeCase {
        name: "yts_q5mg_tab_before_root_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/Q5MG/in.yaml"),
    },
    TreeCase {
        name: "yts_y79y_001_space_tab_block_scalar_content",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-001/in.yaml"),
    },
    TreeCase {
        name: "yts_y79y_002_tab_only_flow_sequence_separation",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-002/in.yaml"),
    },
    TreeCase {
        name: "yts_y79y_010_tab_separated_negative_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/Y79Y-010/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_00_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-00/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_001_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-001/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_002_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-002/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_03_double_quoted_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-03/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_04_double_quoted_escaped_leading_tab",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-04/in.yaml"),
    },
    TreeCase {
        name: "yts_3rln_05_double_quoted_leading_tab_folded",
        input: include_str!("fixtures/yaml-test-suite/data/3RLN-05/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_00_double_quoted_trailing_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-00/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_01_double_quoted_trailing_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-01/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_02_double_quoted_escaped_line_end_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-02/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_03_double_quoted_escaped_line_end_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-03/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_04_double_quoted_literal_trailing_tab",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-04/in.yaml"),
    },
    TreeCase {
        name: "yts_de56_05_double_quoted_literal_trailing_tab_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/DE56-05/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_00_space_tab_mapping_value",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-00/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_02_space_tab_double_quoted_continuation",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-02/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_03_space_tab_blank_line_before_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-03/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_04_tab_only_blank_line_between_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-04/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_05_space_tab_blank_line_between_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-05/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_07_tab_only_line_before_document_start",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-07/in.yaml"),
    },
    TreeCase {
        name: "yts_dk95_08_tabs_in_double_quoted_folded_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/DK95-08/in.yaml"),
    },
    TreeCase {
        name: "yts_96nn_00_leading_tab_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/96NN-00/in.yaml"),
    },
    TreeCase {
        name: "yts_96nn_01_leading_tab_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/96NN-01/in.yaml"),
    },
    TreeCase {
        name: "yts_cpz3_double_quoted_scalar_starting_with_tab",
        input: include_str!("fixtures/yaml-test-suite/data/CPZ3/in.yaml"),
    },
    TreeCase {
        name: "yts_dc7x_trailing_tabs",
        input: include_str!("fixtures/yaml-test-suite/data/DC7X/in.yaml"),
    },
    TreeCase {
        name: "yts_nb6z_plain_scalar_tabs_on_empty_lines",
        input: include_str!("fixtures/yaml-test-suite/data/NB6Z/in.yaml"),
    },
    TreeCase {
        name: "yts_uv7q_legal_tab_after_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/UV7Q/in.yaml"),
    },
    TreeCase {
        name: "yts_kh5v_00_double_quoted_inline_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-00/in.yaml"),
    },
    TreeCase {
        name: "yts_kh5v_001_double_quoted_inline_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-001/in.yaml"),
    },
    TreeCase {
        name: "yts_kh5v_02_double_quoted_inline_escaped_tab",
        input: include_str!("fixtures/yaml-test-suite/data/KH5V-02/in.yaml"),
    },
    TreeCase {
        name: "yts_6wpf_double_quoted_multiline_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/6WPF/in.yaml"),
    },
    TreeCase {
        name: "yts_kss4_same_indent_double_quoted_stream_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/KSS4/in.yaml"),
    },
    TreeCase {
        name: "yts_xw4d_various_trailing_comments",
        input: include_str!("fixtures/yaml-test-suite/data/XW4D/in.yaml"),
    },
    TreeCase {
        name: "yts_rzp5_various_trailing_comments_same_line_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/RZP5/in.yaml"),
    },
    TreeCase {
        name: "yts_a984_multiline_scalar_in_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/A984/in.yaml"),
    },
    TreeCase {
        name: "yts_p2ad_block_scalar_header",
        input: include_str!("fixtures/yaml-test-suite/data/P2AD/in.yaml"),
    },
    TreeCase {
        name: "yts_f8f9_block_scalar_chomping",
        input: include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml"),
    },
    TreeCase {
        name: "yts_f6mc_folded_block_more_indented_lines",
        input: include_str!("fixtures/yaml-test-suite/data/F6MC/in.yaml"),
    },
    TreeCase {
        name: "yts_m5c3_block_scalar_tags",
        input: include_str!("fixtures/yaml-test-suite/data/M5C3/in.yaml"),
    },
    TreeCase {
        name: "yts_36f6_multiline_plain_scalar_with_empty_line",
        input: include_str!("fixtures/yaml-test-suite/data/36F6/in.yaml"),
    },
    TreeCase {
        name: "yts_5gbf_empty_lines_in_flow_and_block_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/5GBF/in.yaml"),
    },
    TreeCase {
        name: "yts_4cqq_multi_line_flow_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/4CQQ/in.yaml"),
    },
    TreeCase {
        name: "yts_7tmg_flow_sequence_comments",
        input: include_str!("fixtures/yaml-test-suite/data/7TMG/in.yaml"),
    },
    TreeCase {
        name: "yts_9mmw_single_pair_implicit_entries",
        input: include_str!("fixtures/yaml-test-suite/data/9MMW/in.yaml"),
    },
    TreeCase {
        name: "yts_8kb6_multiline_flow_plain_key",
        input: include_str!("fixtures/yaml-test-suite/data/8KB6/in.yaml"),
    },
    TreeCase {
        name: "yts_6bfj_flow_key_metadata",
        input: include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml"),
    },
    TreeCase {
        name: "yts_mzx3_scalar_styles",
        input: include_str!("fixtures/yaml-test-suite/data/MZX3/in.yaml"),
    },
    TreeCase {
        name: "yts_6m2f_aliases_in_explicit_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/6M2F/in.yaml"),
    },
    TreeCase {
        name: "yts_bec7_yaml_version_1_3_directive",
        input: include_str!("fixtures/yaml-test-suite/data/BEC7/in.yaml"),
    },
    TreeCase {
        name: "yts_27na_yaml_version_1_2_directive",
        input: include_str!("fixtures/yaml-test-suite/data/27NA/in.yaml"),
    },
    TreeCase {
        name: "yts_2lfx_reserved_directive_with_comment",
        input: include_str!("fixtures/yaml-test-suite/data/2LFX/in.yaml"),
    },
    TreeCase {
        name: "yts_6zkb_stream_yaml_version_directive",
        input: include_str!("fixtures/yaml-test-suite/data/6ZKB/in.yaml"),
    },
    TreeCase {
        name: "yts_9dxl_mapping_stream_yaml_version_directive",
        input: include_str!("fixtures/yaml-test-suite/data/9DXL/in.yaml"),
    },
    TreeCase {
        name: "yts_mus6_02_yaml_version_extra_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-02/in.yaml"),
    },
    TreeCase {
        name: "yts_mus6_03_yaml_version_tab_spacing",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-03/in.yaml"),
    },
    TreeCase {
        name: "yts_mus6_04_yaml_version_comment",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-04/in.yaml"),
    },
    TreeCase {
        name: "yts_mus6_05_reserved_short_yaml_spelling",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-05/in.yaml"),
    },
    TreeCase {
        name: "yts_mus6_06_reserved_long_yaml_spelling",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-06/in.yaml"),
    },
    TreeCase {
        name: "yts_u3c3_tag_directive_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/U3C3/in.yaml"),
    },
    TreeCase {
        name: "yts_6ck3_tag_shorthand_suffix_escapes",
        input: include_str!("fixtures/yaml-test-suite/data/6CK3/in.yaml"),
    },
    TreeCase {
        name: "yts_fta2_document_start_anchor",
        input: include_str!("fixtures/yaml-test-suite/data/FTA2/in.yaml"),
    },
    TreeCase {
        name: "yts_229q_spec_example_2_4_sequence_of_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/229Q/in.yaml"),
    },
    TreeCase {
        name: "yts_2g84_02_literal_modifers",
        input: include_str!("fixtures/yaml-test-suite/data/2G84-02/in.yaml"),
    },
    TreeCase {
        name: "yts_2g84_03_literal_modifers",
        input: include_str!("fixtures/yaml-test-suite/data/2G84-03/in.yaml"),
    },
    TreeCase {
        name: "yts_3alj_block_sequence_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/3ALJ/in.yaml"),
    },
    TreeCase {
        name: "yts_3uys_escaped_slash_in_double_quotes",
        input: include_str!("fixtures/yaml-test-suite/data/3UYS/in.yaml"),
    },
    TreeCase {
        name: "yts_4gc6_spec_example_7_7_single_quoted_characters",
        input: include_str!("fixtures/yaml-test-suite/data/4GC6/in.yaml"),
    },
    TreeCase {
        name: "yts_4qfq_spec_example_8_2_block_indentation_indicator_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/4QFQ/in.yaml"),
    },
    TreeCase {
        name: "yts_4rwc_trailing_spaces_after_flow_collection",
        input: include_str!("fixtures/yaml-test-suite/data/4RWC/in.yaml"),
    },
    TreeCase {
        name: "yts_4uyu_colon_in_double_quoted_string",
        input: include_str!("fixtures/yaml-test-suite/data/4UYU/in.yaml"),
    },
    TreeCase {
        name: "yts_4v8u_plain_scalar_with_backslashes",
        input: include_str!("fixtures/yaml-test-suite/data/4V8U/in.yaml"),
    },
    TreeCase {
        name: "yts_4wa9_literal_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/4WA9/in.yaml"),
    },
    TreeCase {
        name: "yts_4zym_spec_example_6_4_line_prefixes",
        input: include_str!("fixtures/yaml-test-suite/data/4ZYM/in.yaml"),
    },
    TreeCase {
        name: "yts_54t7_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/54T7/in.yaml"),
    },
    TreeCase {
        name: "yts_5bvj_spec_example_5_7_block_scalar_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/5BVJ/in.yaml"),
    },
    TreeCase {
        name: "yts_5c5m_spec_example_7_15_flow_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/5C5M/in.yaml"),
    },
    TreeCase {
        name: "yts_5kje_spec_example_7_13_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/5KJE/in.yaml"),
    },
    TreeCase {
        name: "yts_5nyz_spec_example_6_9_separated_comment",
        input: include_str!("fixtures/yaml-test-suite/data/5NYZ/in.yaml"),
    },
    TreeCase {
        name: "yts_65wh_single_entry_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/65WH/in.yaml"),
    },
    TreeCase {
        name: "yts_6h3v_backslashes_in_singlequotes",
        input: include_str!("fixtures/yaml-test-suite/data/6H3V/in.yaml"),
    },
    TreeCase {
        name: "yts_6hb6_spec_example_6_1_indentation_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/6HB6/in.yaml"),
    },
    TreeCase {
        name: "yts_6jqw_spec_example_2_13_in_literals_newlines_are_preserved",
        input: include_str!("fixtures/yaml-test-suite/data/6JQW/in.yaml"),
    },
    TreeCase {
        name: "yts_6xdy_two_document_start_markers",
        input: include_str!("fixtures/yaml-test-suite/data/6XDY/in.yaml"),
    },
    TreeCase {
        name: "yts_753e_block_scalar_strip_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/753E/in.yaml"),
    },
    TreeCase {
        name: "yts_7a4e_spec_example_7_6_double_quoted_lines",
        input: include_str!("fixtures/yaml-test-suite/data/7A4E/in.yaml"),
    },
    TreeCase {
        name: "yts_7zz5_empty_flow_collections",
        input: include_str!("fixtures/yaml-test-suite/data/7ZZ5/in.yaml"),
    },
    TreeCase {
        name: "yts_82an_three_dashes_and_content_without_space",
        input: include_str!("fixtures/yaml-test-suite/data/82AN/in.yaml"),
    },
    TreeCase {
        name: "yts_87e4_spec_example_7_8_single_quoted_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/87E4/in.yaml"),
    },
    TreeCase {
        name: "yts_8cwc_plain_mapping_key_ending_with_colon",
        input: include_str!("fixtures/yaml-test-suite/data/8CWC/in.yaml"),
    },
    TreeCase {
        name: "yts_8qbe_block_sequence_in_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/8QBE/in.yaml"),
    },
    TreeCase {
        name: "yts_8udb_spec_example_7_14_flow_sequence_entries",
        input: include_str!("fixtures/yaml-test-suite/data/8UDB/in.yaml"),
    },
    TreeCase {
        name: "yts_93jh_block_mappings_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/93JH/in.yaml"),
    },
    TreeCase {
        name: "yts_96l6_spec_example_2_14_in_the_folded_scalars_newlines_become_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/96L6/in.yaml"),
    },
    TreeCase {
        name: "yts_9bxh_multiline_doublequoted_flow_mapping_key_without_value",
        input: include_str!("fixtures/yaml-test-suite/data/9BXH/in.yaml"),
    },
    TreeCase {
        name: "yts_9fmg_multi_level_mapping_indent",
        input: include_str!("fixtures/yaml-test-suite/data/9FMG/in.yaml"),
    },
    TreeCase {
        name: "yts_9j7a_simple_mapping_indent",
        input: include_str!("fixtures/yaml-test-suite/data/9J7A/in.yaml"),
    },
    TreeCase {
        name: "yts_9mqt_00_scalar_doc_with_in_content",
        input: include_str!("fixtures/yaml-test-suite/data/9MQT-00/in.yaml"),
    },
    TreeCase {
        name: "yts_9shh_spec_example_5_8_quoted_scalar_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/9SHH/in.yaml"),
    },
    TreeCase {
        name: "yts_9tfx_spec_example_7_6_double_quoted_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/9TFX/in.yaml"),
    },
    TreeCase {
        name: "yts_9u5k_spec_example_2_12_compact_nested_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/9U5K/in.yaml"),
    },
    TreeCase {
        name: "yts_9yrd_multiline_scalar_at_top_level",
        input: include_str!("fixtures/yaml-test-suite/data/9YRD/in.yaml"),
    },
    TreeCase {
        name: "yts_a6f9_spec_example_8_4_chomping_final_line_break",
        input: include_str!("fixtures/yaml-test-suite/data/A6F9/in.yaml"),
    },
    TreeCase {
        name: "yts_az63_sequence_with_same_indentation_as_parent_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/AZ63/in.yaml"),
    },
    TreeCase {
        name: "yts_26dv_whitespace_around_colon_in_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/26DV/in.yaml"),
    },
    TreeCase {
        name: "yts_3myt_plain_scalar_looking_like_key_comment_anchor_and_tag",
        input: include_str!("fixtures/yaml-test-suite/data/3MYT/in.yaml"),
    },
    TreeCase {
        name: "yts_4fj6_nested_implicit_complex_keys",
        input: include_str!("fixtures/yaml-test-suite/data/4FJ6/in.yaml"),
    },
    TreeCase {
        name: "yts_6sla_allowed_characters_in_quoted_mapping_key",
        input: include_str!("fixtures/yaml-test-suite/data/6SLA/in.yaml"),
    },
    TreeCase {
        name: "yts_azw3_lookahead_test_cases",
        input: include_str!("fixtures/yaml-test-suite/data/AZW3/in.yaml"),
    },
    TreeCase {
        name: "yts_b3hg_spec_example_8_9_folded_scalar_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/B3HG/in.yaml"),
    },
    TreeCase {
        name: "yts_d83l_block_scalar_indicator_order",
        input: include_str!("fixtures/yaml-test-suite/data/D83L/in.yaml"),
    },
    TreeCase {
        name: "yts_d88j_flow_sequence_in_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/D88J/in.yaml"),
    },
    TreeCase {
        name: "yts_d9tu_single_pair_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/D9TU/in.yaml"),
    },
    TreeCase {
        name: "yts_dff7_spec_example_7_16_flow_mapping_entries",
        input: include_str!("fixtures/yaml-test-suite/data/DFF7/in.yaml"),
    },
    TreeCase {
        name: "yts_dwx9_spec_example_8_8_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/DWX9/in.yaml"),
    },
    TreeCase {
        name: "yts_ex5h_multiline_scalar_at_top_level_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/EX5H/in.yaml"),
    },
    TreeCase {
        name: "yts_exg3_three_dashes_and_content_without_space_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/EXG3/in.yaml"),
    },
    TreeCase {
        name: "yts_f3cp_nested_flow_collections_on_one_line",
        input: include_str!("fixtures/yaml-test-suite/data/F3CP/in.yaml"),
    },
    TreeCase {
        name: "yts_fq7f_spec_example_2_1_sequence_of_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/FQ7F/in.yaml"),
    },
    TreeCase {
        name: "yts_fup4_flow_sequence_in_flow_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/FUP4/in.yaml"),
    },
    TreeCase {
        name: "yts_g4rs_spec_example_2_17_quoted_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/G4RS/in.yaml"),
    },
    TreeCase {
        name: "yts_g992_spec_example_8_9_folded_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/G992/in.yaml"),
    },
    TreeCase {
        name: "yts_gh63_mixed_block_mapping_explicit_to_implicit",
        input: include_str!("fixtures/yaml-test-suite/data/GH63/in.yaml"),
    },
    TreeCase {
        name: "yts_h2rw_blank_lines",
        input: include_str!("fixtures/yaml-test-suite/data/H2RW/in.yaml"),
    },
    TreeCase {
        name: "yts_h3z8_literal_unicode",
        input: include_str!("fixtures/yaml-test-suite/data/H3Z8/in.yaml"),
    },
    TreeCase {
        name: "yts_hmk4_spec_example_2_16_indentation_determines_scope",
        input: include_str!("fixtures/yaml-test-suite/data/HMK4/in.yaml"),
    },
    TreeCase {
        name: "yts_hs5t_spec_example_7_12_plain_lines",
        input: include_str!("fixtures/yaml-test-suite/data/HS5T/in.yaml"),
    },
    TreeCase {
        name: "yts_j3bt_spec_example_5_12_tabs_and_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/J3BT/in.yaml"),
    },
    TreeCase {
        name: "yts_j5uc_multiple_pair_block_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/J5UC/in.yaml"),
    },
    TreeCase {
        name: "yts_j7vc_empty_lines_between_mapping_elements",
        input: include_str!("fixtures/yaml-test-suite/data/J7VC/in.yaml"),
    },
    TreeCase {
        name: "yts_j9hz_spec_example_2_9_single_document_with_two_comments",
        input: include_str!("fixtures/yaml-test-suite/data/J9HZ/in.yaml"),
    },
    TreeCase {
        name: "yts_jef9_00_trailing_whitespace_in_streams",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-00/in.yaml"),
    },
    TreeCase {
        name: "yts_jef9_01_trailing_whitespace_in_streams",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-01/in.yaml"),
    },
    TreeCase {
        name: "yts_jq4r_spec_example_8_14_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/JQ4R/in.yaml"),
    },
    TreeCase {
        name: "yts_js2j_spec_example_6_29_node_anchors",
        input: include_str!("fixtures/yaml-test-suite/data/JS2J/in.yaml"),
    },
    TreeCase {
        name: "yts_jtv5_block_mapping_with_multiline_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/JTV5/in.yaml"),
    },
    TreeCase {
        name: "yts_k4su_multiple_entry_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/K4SU/in.yaml"),
    },
    TreeCase {
        name: "yts_kk5p_various_combinations_of_explicit_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/KK5P/in.yaml"),
    },
    TreeCase {
        name: "yts_kmk3_block_submapping",
        input: include_str!("fixtures/yaml-test-suite/data/KMK3/in.yaml"),
    },
    TreeCase {
        name: "yts_l24t_00_trailing_line_of_spaces",
        input: include_str!("fixtures/yaml-test-suite/data/L24T-00/in.yaml"),
    },
    TreeCase {
        name: "yts_l383_two_scalar_docs_with_trailing_comments",
        input: include_str!("fixtures/yaml-test-suite/data/L383/in.yaml"),
    },
    TreeCase {
        name: "yts_l9u5_spec_example_7_11_plain_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/L9U5/in.yaml"),
    },
    TreeCase {
        name: "yts_lp6e_whitespace_after_scalars_in_flow",
        input: include_str!("fixtures/yaml-test-suite/data/LP6E/in.yaml"),
    },
    TreeCase {
        name: "yts_lqz7_spec_example_7_4_double_quoted_implicit_keys",
        input: include_str!("fixtures/yaml-test-suite/data/LQZ7/in.yaml"),
    },
    TreeCase {
        name: "yts_lx3p_implicit_flow_mapping_key_on_one_line",
        input: include_str!("fixtures/yaml-test-suite/data/LX3P/in.yaml"),
    },
    TreeCase {
        name: "yts_m29m_literal_block_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/M29M/in.yaml"),
    },
    TreeCase {
        name: "yts_m2n8_01_question_mark_edge_cases",
        input: include_str!("fixtures/yaml-test-suite/data/M2N8-01/in.yaml"),
    },
    TreeCase {
        name: "yts_m5dy_spec_example_2_11_mapping_between_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/M5DY/in.yaml"),
    },
    TreeCase {
        name: "yts_m6yh_block_sequence_indentation",
        input: include_str!("fixtures/yaml-test-suite/data/M6YH/in.yaml"),
    },
    TreeCase {
        name: "yts_m7nx_nested_flow_collections",
        input: include_str!("fixtures/yaml-test-suite/data/M7NX/in.yaml"),
    },
    TreeCase {
        name: "yts_m9b4_spec_example_8_7_literal_scalar",
        input: include_str!("fixtures/yaml-test-suite/data/M9B4/in.yaml"),
    },
    TreeCase {
        name: "yts_mjs9_spec_example_6_7_block_folding",
        input: include_str!("fixtures/yaml-test-suite/data/MJS9/in.yaml"),
    },
    TreeCase {
        name: "yts_mxs3_flow_mapping_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/MXS3/in.yaml"),
    },
    TreeCase {
        name: "yts_myw6_block_scalar_strip",
        input: include_str!("fixtures/yaml-test-suite/data/MYW6/in.yaml"),
    },
    TreeCase {
        name: "yts_nat4_various_empty_or_newline_only_quoted_strings",
        input: include_str!("fixtures/yaml-test-suite/data/NAT4/in.yaml"),
    },
    TreeCase {
        name: "yts_np9h_spec_example_7_5_double_quoted_line_breaks",
        input: include_str!("fixtures/yaml-test-suite/data/NP9H/in.yaml"),
    },
    TreeCase {
        name: "yts_p94k_spec_example_6_11_multi_line_comments",
        input: include_str!("fixtures/yaml-test-suite/data/P94K/in.yaml"),
    },
    TreeCase {
        name: "yts_pbj2_spec_example_2_3_mapping_scalars_to_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/PBJ2/in.yaml"),
    },
    TreeCase {
        name: "yts_prh3_spec_example_7_9_single_quoted_lines",
        input: include_str!("fixtures/yaml-test-suite/data/PRH3/in.yaml"),
    },
    TreeCase {
        name: "yts_puw8_document_start_on_last_line",
        input: include_str!("fixtures/yaml-test-suite/data/PUW8/in.yaml"),
    },
    TreeCase {
        name: "yts_q88a_spec_example_7_23_flow_content",
        input: include_str!("fixtures/yaml-test-suite/data/Q88A/in.yaml"),
    },
    TreeCase {
        name: "yts_q8ad_spec_example_7_5_double_quoted_line_breaks_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/Q8AD/in.yaml"),
    },
    TreeCase {
        name: "yts_r52l_nested_flow_mapping_sequence_and_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/R52L/in.yaml"),
    },
    TreeCase {
        name: "yts_rlu9_sequence_indent",
        input: include_str!("fixtures/yaml-test-suite/data/RLU9/in.yaml"),
    },
    TreeCase {
        name: "yts_rr7f_mixed_block_mapping_implicit_to_explicit",
        input: include_str!("fixtures/yaml-test-suite/data/RR7F/in.yaml"),
    },
    TreeCase {
        name: "yts_rzt7_spec_example_2_28_log_file",
        input: include_str!("fixtures/yaml-test-suite/data/RZT7/in.yaml"),
    },
    TreeCase {
        name: "yts_s4t7_document_with_footer",
        input: include_str!("fixtures/yaml-test-suite/data/S4T7/in.yaml"),
    },
    TreeCase {
        name: "yts_s7bg_colon_followed_by_comma",
        input: include_str!("fixtures/yaml-test-suite/data/S7BG/in.yaml"),
    },
    TreeCase {
        name: "yts_s9e8_spec_example_5_3_block_structure_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/S9E8/in.yaml"),
    },
    TreeCase {
        name: "yts_sbg9_flow_sequence_in_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/SBG9/in.yaml"),
    },
    TreeCase {
        name: "yts_sm9w_00_single_character_streams",
        input: include_str!("fixtures/yaml-test-suite/data/SM9W-00/in.yaml"),
    },
    TreeCase {
        name: "yts_ssw6_spec_example_7_7_single_quoted_characters_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/SSW6/in.yaml"),
    },
    TreeCase {
        name: "yts_syw4_spec_example_2_2_mapping_scalars_to_scalars",
        input: include_str!("fixtures/yaml-test-suite/data/SYW4/in.yaml"),
    },
    TreeCase {
        name: "yts_t26h_spec_example_8_8_literal_content_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T26H/in.yaml"),
    },
    TreeCase {
        name: "yts_t4yy_spec_example_7_9_single_quoted_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T4YY/in.yaml"),
    },
    TreeCase {
        name: "yts_t5n4_spec_example_8_7_literal_scalar_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/T5N4/in.yaml"),
    },
    TreeCase {
        name: "yts_te2a_spec_example_8_16_block_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/TE2A/in.yaml"),
    },
    TreeCase {
        name: "yts_tl85_spec_example_6_8_flow_folding",
        input: include_str!("fixtures/yaml-test-suite/data/TL85/in.yaml"),
    },
    TreeCase {
        name: "yts_u9ns_spec_example_2_8_play_by_play_feed_from_a_game",
        input: include_str!("fixtures/yaml-test-suite/data/U9NS/in.yaml"),
    },
    TreeCase {
        name: "yts_udr7_spec_example_5_4_flow_collection_indicators",
        input: include_str!("fixtures/yaml-test-suite/data/UDR7/in.yaml"),
    },
    TreeCase {
        name: "yts_ukk6_01_syntax_character_edge_cases",
        input: include_str!("fixtures/yaml-test-suite/data/UKK6-01/in.yaml"),
    },
    TreeCase {
        name: "yts_v55r_aliases_in_block_sequence",
        input: include_str!("fixtures/yaml-test-suite/data/V55R/in.yaml"),
    },
    TreeCase {
        name: "yts_w42u_spec_example_8_15_block_sequence_entry_types",
        input: include_str!("fixtures/yaml-test-suite/data/W42U/in.yaml"),
    },
    TreeCase {
        name: "yts_x8dw_explicit_key_and_value_seperated_by_comment",
        input: include_str!("fixtures/yaml-test-suite/data/X8DW/in.yaml"),
    },
    TreeCase {
        name: "yts_xv9v_spec_example_6_5_empty_lines_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/XV9V/in.yaml"),
    },
    TreeCase {
        name: "yts_yd5x_spec_example_2_5_sequence_of_sequences",
        input: include_str!("fixtures/yaml-test-suite/data/YD5X/in.yaml"),
    },
    TreeCase {
        name: "yts_z67p_spec_example_8_21_block_scalar_nodes_1_3",
        input: include_str!("fixtures/yaml-test-suite/data/Z67P/in.yaml"),
    },
    TreeCase {
        name: "yts_zf4x_spec_example_2_6_mapping_of_mappings",
        input: include_str!("fixtures/yaml-test-suite/data/ZF4X/in.yaml"),
    },
    TreeCase {
        name: "yts_zh7c_anchors_in_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/ZH7C/in.yaml"),
    },
    TreeCase {
        name: "yts_zk9h_nested_top_level_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/ZK9H/in.yaml"),
    },
    TreeCase {
        name: "yts_2xxw",
        input: include_str!("fixtures/yaml-test-suite/data/2XXW/in.yaml"),
    },
    TreeCase {
        name: "yts_35kp",
        input: include_str!("fixtures/yaml-test-suite/data/35KP/in.yaml"),
    },
    TreeCase {
        name: "yts_4abk",
        input: include_str!("fixtures/yaml-test-suite/data/4ABK/in.yaml"),
    },
    TreeCase {
        name: "yts_4muz_00",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-00/in.yaml"),
    },
    TreeCase {
        name: "yts_4muz_01",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-01/in.yaml"),
    },
    TreeCase {
        name: "yts_4muz_02",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-02/in.yaml"),
    },
    TreeCase {
        name: "yts_52dl",
        input: include_str!("fixtures/yaml-test-suite/data/52DL/in.yaml"),
    },
    TreeCase {
        name: "yts_565n",
        input: include_str!("fixtures/yaml-test-suite/data/565N/in.yaml"),
    },
    TreeCase {
        name: "yts_5tym",
        input: include_str!("fixtures/yaml-test-suite/data/5TYM/in.yaml"),
    },
    TreeCase {
        name: "yts_652z",
        input: include_str!("fixtures/yaml-test-suite/data/652Z/in.yaml"),
    },
    TreeCase {
        name: "yts_6jwb",
        input: include_str!("fixtures/yaml-test-suite/data/6JWB/in.yaml"),
    },
    TreeCase {
        name: "yts_6wlz",
        input: include_str!("fixtures/yaml-test-suite/data/6WLZ/in.yaml"),
    },
    TreeCase {
        name: "yts_735y",
        input: include_str!("fixtures/yaml-test-suite/data/735Y/in.yaml"),
    },
    TreeCase {
        name: "yts_7fwl",
        input: include_str!("fixtures/yaml-test-suite/data/7FWL/in.yaml"),
    },
    TreeCase {
        name: "yts_7z25",
        input: include_str!("fixtures/yaml-test-suite/data/7Z25/in.yaml"),
    },
    TreeCase {
        name: "yts_8g76",
        input: include_str!("fixtures/yaml-test-suite/data/8G76/in.yaml"),
    },
    TreeCase {
        name: "yts_8mk2",
        input: include_str!("fixtures/yaml-test-suite/data/8MK2/in.yaml"),
    },
    TreeCase {
        name: "yts_8xyn",
        input: include_str!("fixtures/yaml-test-suite/data/8XYN/in.yaml"),
    },
    TreeCase {
        name: "yts_98yd",
        input: include_str!("fixtures/yaml-test-suite/data/98YD/in.yaml"),
    },
    TreeCase {
        name: "yts_9wxw",
        input: include_str!("fixtures/yaml-test-suite/data/9WXW/in.yaml"),
    },
    TreeCase {
        name: "yts_a2m4",
        input: include_str!("fixtures/yaml-test-suite/data/A2M4/in.yaml"),
    },
    TreeCase {
        name: "yts_avm7",
        input: include_str!("fixtures/yaml-test-suite/data/AVM7/in.yaml"),
    },
    TreeCase {
        name: "yts_cc74",
        input: include_str!("fixtures/yaml-test-suite/data/CC74/in.yaml"),
    },
    TreeCase {
        name: "yts_dbg4",
        input: include_str!("fixtures/yaml-test-suite/data/DBG4/in.yaml"),
    },
    TreeCase {
        name: "yts_ehf6",
        input: include_str!("fixtures/yaml-test-suite/data/EHF6/in.yaml"),
    },
    TreeCase {
        name: "yts_frk4",
        input: include_str!("fixtures/yaml-test-suite/data/FRK4/in.yaml"),
    },
    TreeCase {
        name: "yts_hm87_00",
        input: include_str!("fixtures/yaml-test-suite/data/HM87-00/in.yaml"),
    },
    TreeCase {
        name: "yts_hm87_01",
        input: include_str!("fixtures/yaml-test-suite/data/HM87-01/in.yaml"),
    },
    TreeCase {
        name: "yts_hmq5",
        input: include_str!("fixtures/yaml-test-suite/data/HMQ5/in.yaml"),
    },
    TreeCase {
        name: "yts_hwv9",
        input: include_str!("fixtures/yaml-test-suite/data/HWV9/in.yaml"),
    },
    TreeCase {
        name: "yts_j7pz",
        input: include_str!("fixtures/yaml-test-suite/data/J7PZ/in.yaml"),
    },
    TreeCase {
        name: "yts_jef9_02",
        input: include_str!("fixtures/yaml-test-suite/data/JEF9-02/in.yaml"),
    },
    TreeCase {
        name: "yts_jr7v",
        input: include_str!("fixtures/yaml-test-suite/data/JR7V/in.yaml"),
    },
    TreeCase {
        name: "yts_k3wx",
        input: include_str!("fixtures/yaml-test-suite/data/K3WX/in.yaml"),
    },
    TreeCase {
        name: "yts_l24t_01",
        input: include_str!("fixtures/yaml-test-suite/data/L24T-01/in.yaml"),
    },
    TreeCase {
        name: "yts_le5a",
        input: include_str!("fixtures/yaml-test-suite/data/LE5A/in.yaml"),
    },
    TreeCase {
        name: "yts_nhx8",
        input: include_str!("fixtures/yaml-test-suite/data/NHX8/in.yaml"),
    },
    TreeCase {
        name: "yts_nj66",
        input: include_str!("fixtures/yaml-test-suite/data/NJ66/in.yaml"),
    },
    TreeCase {
        name: "yts_nkf9",
        input: include_str!("fixtures/yaml-test-suite/data/NKF9/in.yaml"),
    },
    TreeCase {
        name: "yts_p76l",
        input: include_str!("fixtures/yaml-test-suite/data/P76L/in.yaml"),
    },
    TreeCase {
        name: "yts_q9wf",
        input: include_str!("fixtures/yaml-test-suite/data/Q9WF/in.yaml"),
    },
    TreeCase {
        name: "yts_qt73",
        input: include_str!("fixtures/yaml-test-suite/data/QT73/in.yaml"),
    },
    TreeCase {
        name: "yts_rtp8",
        input: include_str!("fixtures/yaml-test-suite/data/RTP8/in.yaml"),
    },
    TreeCase {
        name: "yts_sm9w_01",
        input: include_str!("fixtures/yaml-test-suite/data/SM9W-01/in.yaml"),
    },
    TreeCase {
        name: "yts_udm2",
        input: include_str!("fixtures/yaml-test-suite/data/UDM2/in.yaml"),
    },
    TreeCase {
        name: "yts_ugm3",
        input: include_str!("fixtures/yaml-test-suite/data/UGM3/in.yaml"),
    },
    TreeCase {
        name: "yts_vjp3_01",
        input: include_str!("fixtures/yaml-test-suite/data/VJP3-01/in.yaml"),
    },
    TreeCase {
        name: "yts_w5vh",
        input: include_str!("fixtures/yaml-test-suite/data/W5VH/in.yaml"),
    },
    TreeCase {
        name: "yts_z9m4",
        input: include_str!("fixtures/yaml-test-suite/data/Z9M4/in.yaml"),
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
        name: "github_actions_starter_node_ci",
        input: include_str!("fixtures/real-world/github-actions/starter-node-ci.yml"),
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
        name: "docker_compose_awesome_nginx_flask_mysql",
        input: include_str!("fixtures/real-world/docker-compose/awesome-nginx-flask-mysql.yaml"),
    },
    TreeCase {
        name: "docker_compose_anchors",
        input: include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml"),
    },
    TreeCase {
        name: "docker_compose_adapted_compose_spec_fragments",
        input: include_str!(
            "fixtures/real-world/docker-compose/adapted-compose-spec-fragments.yaml"
        ),
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
        name: "kubernetes_upstream_guestbook_frontend_deployment",
        input: include_str!(
            "fixtures/real-world/kubernetes/upstream-guestbook-frontend-deployment.yaml"
        ),
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
        name: "helm_upstream_hello_world_chart",
        input: include_str!("fixtures/real-world/helm/upstream-hello-world-Chart.yaml"),
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
        name: "openapi_upstream_petstore",
        input: include_str!("fixtures/real-world/openapi/upstream-petstore.yaml"),
    },
    TreeCase {
        name: "wrangler_yaml",
        input: include_str!("fixtures/real-world/cloudflare/wrangler.yaml"),
    },
    TreeCase {
        name: "wrangler_adapted_durable_objects",
        input: include_str!("fixtures/real-world/cloudflare/adapted-durable-objects-wrangler.yaml"),
    },
    TreeCase {
        name: "ansible_playbook",
        input: include_str!("fixtures/real-world/ansible/playbook.yaml"),
    },
    TreeCase {
        name: "ansible_vault_and_unsafe_tags",
        input: include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml"),
    },
    TreeCase {
        name: "ansible_upstream_lamp_simple_site",
        input: include_str!("fixtures/real-world/ansible/upstream-lamp-simple-site.yml"),
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
        let yaml_rust2 = normalize_yaml_rust2_documents_with_default_merges(case.input)
            .unwrap_or_else(|error| panic!("yaml-rust2 tree failed {}: {error}", case.name));
        let saphyr = normalize_saphyr_documents_with_default_merges(case.input, TagPolicy::Strip)
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

fn normalize_yaml_rust2_documents_with_default_merges(
    input: &str,
) -> Result<Vec<NormTree>, yaml_rust2::ScanError> {
    normalize_yaml_rust2_documents(input).map(|documents| {
        documents
            .into_iter()
            .map(expand_default_norm_merges)
            .collect()
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

fn normalize_saphyr_documents_with_default_merges(
    input: &str,
    tags: TagPolicy,
) -> Result<Vec<NormTree>, saphyr::ScanError> {
    normalize_saphyr_documents(input, tags).map(|documents| {
        documents
            .into_iter()
            .map(expand_default_norm_merges)
            .collect()
    })
}

// The Rust reference loaders keep real-world `<<` merge syntax in loaded trees.
// For those fixtures, compare their normalized trees after applying this
// crate's public default merge policy while raw events/lossless tests keep
// proving source syntax retention.
fn expand_default_norm_merges(tree: NormTree) -> NormTree {
    match tree {
        NormTree::Seq(items) => {
            NormTree::Seq(items.into_iter().map(expand_default_norm_merges).collect())
        }
        NormTree::Map(entries) => expand_default_norm_mapping(entries),
        NormTree::Tagged(tag, value) => {
            NormTree::Tagged(tag, Box::new(expand_default_norm_merges(*value)))
        }
        other => other,
    }
}

fn expand_default_norm_mapping(entries: Vec<(NormTree, NormTree)>) -> NormTree {
    let mut explicit_entries = Vec::new();
    let mut merged_entries = Vec::new();

    for (key, value) in entries {
        let key = expand_default_norm_merges(key);
        let value = expand_default_norm_merges(value);
        if is_norm_merge_key(&key) {
            insert_norm_merge_entries(&mut merged_entries, norm_merge_entries(value));
        } else {
            explicit_entries.push((key, value));
        }
    }

    insert_norm_merge_entries(&mut explicit_entries, merged_entries);
    NormTree::Map(explicit_entries)
}

fn norm_merge_entries(value: NormTree) -> Vec<(NormTree, NormTree)> {
    match value {
        NormTree::Map(entries) => entries,
        NormTree::Seq(items) => {
            let mut entries = Vec::new();
            for item in items {
                if let NormTree::Map(item_entries) = item {
                    insert_norm_merge_entries(&mut entries, item_entries);
                }
            }
            entries
        }
        _ => Vec::new(),
    }
}

fn insert_norm_merge_entries(
    target: &mut Vec<(NormTree, NormTree)>,
    merge_entries: Vec<(NormTree, NormTree)>,
) {
    for (key, value) in merge_entries {
        if !target.iter().any(|(existing, _)| existing == &key) {
            target.push((key, value));
        }
    }
}

fn is_norm_merge_key(key: &NormTree) -> bool {
    matches!(key, NormTree::String(value) if value == "<<")
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
            TagPolicy::Strip if normalize_tag(&tag.to_string()) == "!" => {
                normalize_saphyr_non_specific_tagged_node(value)
            }
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

fn normalize_saphyr_non_specific_tagged_node(node: &saphyr::Yaml<'_>) -> NormTree {
    match node {
        saphyr::Yaml::Value(value) => normalize_saphyr_non_specific_scalar(value),
        saphyr::Yaml::Representation(value, _, _) => NormTree::String(value.to_string()),
        _ => normalize_saphyr_node(node, TagPolicy::Strip),
    }
}

fn normalize_saphyr_non_specific_scalar(scalar: &saphyr::Scalar<'_>) -> NormTree {
    match scalar {
        saphyr::Scalar::Null => NormTree::String(String::new()),
        saphyr::Scalar::Boolean(value) => NormTree::String(value.to_string()),
        saphyr::Scalar::Integer(value) => NormTree::String(value.to_string()),
        saphyr::Scalar::FloatingPoint(value) => NormTree::String(normalize_float(value.0)),
        saphyr::Scalar::String(value) => NormTree::String(value.to_string()),
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
