use saneyaml::{CollectionStyle, Event, ScalarStyle};
use saphyr::LoadableYamlNode;
use std::collections::BTreeMap;
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
        name: "yts_96nn_00_leading_tab_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/96NN-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_96nn_01_leading_tab_literal_content",
        input: include_str!("fixtures/yaml-test-suite/data/96NN-01/in.yaml"),
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
        name: "yts_27na_yaml_version_1_2_directive",
        input: include_str!("fixtures/yaml-test-suite/data/27NA/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_2lfx_reserved_directive_with_comment",
        input: include_str!("fixtures/yaml-test-suite/data/2LFX/in.yaml"),
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
        name: "yts_mus6_05_reserved_short_yaml_spelling",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-05/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_mus6_06_reserved_long_yaml_spelling",
        input: include_str!("fixtures/yaml-test-suite/data/MUS6-06/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_6ck3_tag_shorthand_suffix_escapes",
        input: include_str!("fixtures/yaml-test-suite/data/6CK3/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_5tym_local_tag_prefix_stream",
        input: include_str!("fixtures/yaml-test-suite/data/5TYM/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_7fwl_verbatim_tags",
        input: include_str!("fixtures/yaml-test-suite/data/7FWL/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_k54u_tab_after_document_header",
        input: include_str!("fixtures/yaml-test-suite/data/K54U/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_ugm3_invoice_tag_anchor_alias",
        input: include_str!("fixtures/yaml-test-suite/data/UGM3/in.yaml"),
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
        name: "yts_4abk",
        input: include_str!("fixtures/yaml-test-suite/data/4ABK/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4muz_00",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-00/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4muz_01",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_4muz_02",
        input: include_str!("fixtures/yaml-test-suite/data/4MUZ-02/in.yaml"),
        docs: 1,
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
        name: "yts_7z25",
        input: include_str!("fixtures/yaml-test-suite/data/7Z25/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_8g76",
        input: include_str!("fixtures/yaml-test-suite/data/8G76/in.yaml"),
        docs: 0,
    },
    Case {
        name: "yts_8mk2",
        input: include_str!("fixtures/yaml-test-suite/data/8MK2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_8xyn",
        input: include_str!("fixtures/yaml-test-suite/data/8XYN/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_98yd",
        input: include_str!("fixtures/yaml-test-suite/data/98YD/in.yaml"),
        docs: 0,
    },
    Case {
        name: "yts_9wxw",
        input: include_str!("fixtures/yaml-test-suite/data/9WXW/in.yaml"),
        docs: 2,
    },
    Case {
        name: "yts_a2m4",
        input: include_str!("fixtures/yaml-test-suite/data/A2M4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_avm7",
        input: include_str!("fixtures/yaml-test-suite/data/AVM7/in.yaml"),
        docs: 0,
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
        name: "yts_dbg4",
        input: include_str!("fixtures/yaml-test-suite/data/DBG4/in.yaml"),
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
        name: "yts_fh7j",
        input: include_str!("fixtures/yaml-test-suite/data/FH7J/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_frk4",
        input: include_str!("fixtures/yaml-test-suite/data/FRK4/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_hm87_00",
        input: include_str!("fixtures/yaml-test-suite/data/HM87-00/in.yaml"),
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
        name: "yts_hwv9",
        input: include_str!("fixtures/yaml-test-suite/data/HWV9/in.yaml"),
        docs: 0,
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
        name: "yts_k3wx",
        input: include_str!("fixtures/yaml-test-suite/data/K3WX/in.yaml"),
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
        name: "yts_nhx8",
        input: include_str!("fixtures/yaml-test-suite/data/NHX8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_nj66",
        input: include_str!("fixtures/yaml-test-suite/data/NJ66/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_nkf9",
        input: include_str!("fixtures/yaml-test-suite/data/NKF9/in.yaml"),
        docs: 4,
    },
    Case {
        name: "yts_p76l",
        input: include_str!("fixtures/yaml-test-suite/data/P76L/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_q9wf_separation_spaces_flow_mapping",
        input: include_str!("fixtures/yaml-test-suite/data/Q9WF/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_qt73",
        input: include_str!("fixtures/yaml-test-suite/data/QT73/in.yaml"),
        docs: 0,
    },
    Case {
        name: "yts_rtp8",
        input: include_str!("fixtures/yaml-test-suite/data/RTP8/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_sm9w_01",
        input: include_str!("fixtures/yaml-test-suite/data/SM9W-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_udm2",
        input: include_str!("fixtures/yaml-test-suite/data/UDM2/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_vjp3_01",
        input: include_str!("fixtures/yaml-test-suite/data/VJP3-01/in.yaml"),
        docs: 1,
    },
    Case {
        name: "yts_w5vh",
        input: include_str!("fixtures/yaml-test-suite/data/W5VH/in.yaml"),
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
    let ours = saneyaml::parse_documents(case.input)
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

fn ours_scalar_starts(input: &str) -> saneyaml::Result<Vec<ScalarPoint>> {
    saneyaml::parse_events(input).map(|events| {
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

fn ours_alias_starts(input: &str) -> saneyaml::Result<Vec<(usize, usize)>> {
    saneyaml::parse_events(input).map(|events| {
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

fn ours_flow_collection_starts(input: &str) -> saneyaml::Result<Vec<MarkerPoint>> {
    saneyaml::parse_events(input).map(|events| {
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

fn ours_document_marker_starts(input: &str) -> saneyaml::Result<Vec<MarkerPoint>> {
    saneyaml::parse_events(input).map(|events| {
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

fn ours_block_sequence_starts(input: &str) -> saneyaml::Result<Vec<MarkerPoint>> {
    saneyaml::parse_events(input).map(|events| {
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
) -> saneyaml::Result<Vec<NormEvent>> {
    let mut anchors = AnchorNormalizer::default();
    saneyaml::parse_events(input).map(|events| {
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
    let mut collection_starts = Vec::<usize>::new();
    let mut last_closed_collection_start = None;
    let mut previous_was_collection_end = false;
    for result in saphyr_parser::Parser::new_from_str(input) {
        let (event, span) = result?;
        match event {
            saphyr_parser::Event::Nothing => {}
            saphyr_parser::Event::StreamStart => {
                previous_was_collection_end = false;
                events.push(NormEvent::StreamStart);
            }
            saphyr_parser::Event::StreamEnd => {
                previous_was_collection_end = false;
                events.push(NormEvent::StreamEnd);
            }
            saphyr_parser::Event::DocumentStart(explicit) => {
                previous_was_collection_end = false;
                anchors.reset();
                events.push(NormEvent::DocumentStart {
                    explicit: Some(explicit),
                });
            }
            saphyr_parser::Event::DocumentEnd => {
                previous_was_collection_end = false;
                events.push(NormEvent::DocumentEnd);
            }
            saphyr_parser::Event::Alias(anchor) => {
                previous_was_collection_end = false;
                events.push(NormEvent::Alias {
                    anchor: anchors.alias_id(anchor),
                });
            }
            saphyr_parser::Event::Scalar(value, style, anchor, tag) => {
                previous_was_collection_end = false;
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
                previous_was_collection_end = false;
                let start = span.start.index();
                let style = reference_sequence_style(input, start);
                events.push(NormEvent::SequenceStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
                collection_starts.push(start);
            }
            saphyr_parser::Event::SequenceEnd => {
                collections.pop();
                last_closed_collection_start = collection_starts.pop();
                previous_was_collection_end = true;
                events.push(NormEvent::SequenceEnd);
            }
            saphyr_parser::Event::MappingStart(anchor, tag) => {
                let start = span.start.index();
                let mut style = reference_mapping_style(input, start, &collections);
                if byte_at(input, start) == Some(b'{')
                    && collections.last() != Some(&NormCollectionStyle::Flow)
                    && collection_starts.last() != Some(&start)
                    && flow_collection_token_is_mapping_key(input, start)
                {
                    style = NormCollectionStyle::Block;
                }
                if previous_was_collection_end
                    && last_closed_collection_start == Some(start)
                    && collections.last() != Some(&NormCollectionStyle::Flow)
                {
                    style = NormCollectionStyle::Block;
                }
                previous_was_collection_end = false;
                events.push(NormEvent::MappingStart {
                    style,
                    anchor: anchors.define_id(anchor),
                    tag: tag
                        .as_ref()
                        .map(|tag| normalize_reference_tag(&tag.handle, &tag.suffix)),
                });
                collections.push(style);
                collection_starts.push(start);
            }
            saphyr_parser::Event::MappingEnd => {
                collections.pop();
                last_closed_collection_start = collection_starts.pop();
                previous_was_collection_end = true;
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

fn flow_collection_token_is_mapping_key(input: &str, start: usize) -> bool {
    let Some(open) = byte_at(input, start) else {
        return false;
    };
    let close = match open {
        b'{' => '}',
        b'[' => ']',
        _ => return false,
    };
    let open = char::from(open);
    let mut depth = 0usize;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for (relative, ch) in input[start..].char_indices() {
        if double && escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if double => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            ch if !single && !double && ch == open => depth += 1,
            ch if !single && !double && ch == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let after = &input[start + relative + ch.len_utf8()..];
                    return after.trim_start_matches([' ', '\t']).starts_with(':');
                }
            }
            _ => {}
        }
    }
    false
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

fn normalize_our_tag(tag: &saneyaml::Tag) -> String {
    normalize_reference_tag(&tag.handle, &tag.suffix)
}

fn normalize_reference_tag(handle: &str, suffix: &str) -> String {
    match handle {
        "!!" | "tag:yaml.org,2002:" => format!("tag:yaml.org,2002:{suffix}"),
        "!" => {
            if suffix.starts_with("tag:") || suffix.starts_with('!') {
                suffix.to_string()
            } else {
                format!("!{suffix}")
            }
        }
        _ => format!("{handle}{suffix}"),
    }
}
