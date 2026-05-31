use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const COVERAGE: &str = include_str!("fixtures/yaml-test-suite/coverage.toml");
const MANIFEST: &str = include_str!("fixtures/yaml-test-suite/manifest.toml");
const PSYCH_COMPARISON: &str =
    include_str!("fixtures/divergences/probes/psych-libyaml-comparison.toml");
const PSYCH_COVERAGE: &str =
    include_str!("fixtures/divergences/probes/psych-libyaml-coverage.toml");

#[derive(Debug, Deserialize)]
struct SuiteCoverage {
    upstream_case_count: usize,
    selected_case_count: usize,
    not_imported_case_count: usize,
    upstream_cases: Vec<String>,
    selected_cases: Vec<String>,
    not_imported_cases: Vec<String>,
    local_case_alias: Vec<LocalCaseAlias>,
}

#[derive(Debug, Deserialize)]
struct LocalCaseAlias {
    manifest_id: String,
    upstream_id: String,
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
    parity: SuiteParity,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct SuiteParity {
    event: Vec<String>,
    event_deferred: Vec<String>,
    tree: Vec<String>,
    tree_deferred: Vec<String>,
    shared_reference: Vec<String>,
    shared_reference_deferred: Vec<String>,
    lossless_graph: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PsychComparison {
    case: Vec<PsychCase>,
}

#[derive(Debug, Deserialize)]
struct PsychCase {
    rust_policy: String,
}

#[derive(Debug, Deserialize)]
struct PsychCoverage {
    probe_case_count: usize,
    behavior_family_count: usize,
    tracked_gap_count: usize,
}

#[test]
fn yaml_test_suite_conformance_dashboard_reports_current_denominator() {
    let coverage: SuiteCoverage = toml::from_str(COVERAGE).expect("coverage parses");
    let manifest: SuiteManifest = toml::from_str(MANIFEST).expect("manifest parses");
    let psych_comparison: PsychComparison =
        toml::from_str(PSYCH_COMPARISON).expect("Psych comparison parses");
    let psych_coverage: PsychCoverage =
        toml::from_str(PSYCH_COVERAGE).expect("Psych coverage parses");

    let selected_manifest_ids = canonical_manifest_ids(&manifest.case, &coverage);
    assert_eq!(selected_manifest_ids.len(), manifest.case.len());
    assert_eq!(selected_manifest_ids.len(), coverage.selected_case_count);
    assert_eq!(
        selected_manifest_ids,
        unique_set("selected cases", &coverage.selected_cases)
    );

    let mut outcome_counts = BTreeMap::new();
    for case in &manifest.case {
        *outcome_counts
            .entry(case.expected.as_str())
            .or_insert(0usize) += 1;
    }
    let accepted = outcome_counts["accept"];
    let syntax_rejected = outcome_counts["syntax-error"];
    let tree_rejected = outcome_counts["tree-error"];
    let rejected = syntax_rejected + tree_rejected;

    let documented_divergences = documented_divergence_ids(&manifest.parity);
    let psych_policy_counts = psych_policy_counts(&psych_comparison.case);

    assert_eq!(coverage.upstream_case_count, 402);
    assert_eq!(coverage.selected_case_count, 163);
    assert_eq!(coverage.not_imported_case_count, 239);
    assert_eq!(accepted, 108);
    assert_eq!(syntax_rejected, 53);
    assert_eq!(tree_rejected, 2);
    assert_eq!(rejected, 55);
    assert_eq!(documented_divergences.len(), 32);
    assert_eq!(manifest.parity.event.len(), 108);
    assert_eq!(manifest.parity.event_deferred.len(), 0);
    assert_eq!(manifest.parity.tree.len(), 105);
    assert_eq!(manifest.parity.tree_deferred.len(), 3);
    assert_eq!(manifest.parity.shared_reference.len(), 79);
    assert_eq!(manifest.parity.shared_reference_deferred.len(), 29);
    assert_eq!(manifest.parity.lossless_graph.len(), 23);
    assert_eq!(psych_coverage.probe_case_count, 49);
    assert_eq!(psych_policy_counts["matches-psych"], 20);
    assert_eq!(psych_policy_counts["intentional-divergence"], 29);
    assert_eq!(psych_coverage.behavior_family_count, 8);
    assert_eq!(psych_coverage.tracked_gap_count, 0);
    assert_eq!(
        coverage.selected_case_count + coverage.not_imported_case_count,
        coverage.upstream_case_count
    );
    assert_eq!(
        accepted + rejected,
        coverage.selected_case_count,
        "selected outcome buckets must partition selected cases",
    );

    println!(
        "\
YAML test-suite dashboard
upstream denominator: {total}
selected/classified: {selected}/{total}
unselected: {unselected}
accepted: {accepted}
rejected: {rejected}
  syntax: {syntax_rejected}
  tree/serde: {tree_rejected}
documented divergence overlays: {documented_divergences}
parity:
  event: {event_ok} included / {event_deferred} deferred
  tree: {tree_ok} included / {tree_deferred} deferred
  shared-reference: {shared_ok} included / {shared_deferred} deferred
  lossless-graph: {lossless_graph}
Psych/libyaml pinned probes:
  cases: {psych_cases}
  matches-psych: {psych_matches}
  intentional-divergence: {psych_divergences}
  behavior families: {psych_families}
  tracked gaps: {psych_gaps}",
        total = coverage.upstream_case_count,
        selected = coverage.selected_case_count,
        unselected = coverage.not_imported_case_count,
        accepted = accepted,
        rejected = rejected,
        syntax_rejected = syntax_rejected,
        tree_rejected = tree_rejected,
        documented_divergences = documented_divergences.len(),
        event_ok = manifest.parity.event.len(),
        event_deferred = manifest.parity.event_deferred.len(),
        tree_ok = manifest.parity.tree.len(),
        tree_deferred = manifest.parity.tree_deferred.len(),
        shared_ok = manifest.parity.shared_reference.len(),
        shared_deferred = manifest.parity.shared_reference_deferred.len(),
        lossless_graph = manifest.parity.lossless_graph.len(),
        psych_cases = psych_coverage.probe_case_count,
        psych_matches = psych_policy_counts["matches-psych"],
        psych_divergences = psych_policy_counts["intentional-divergence"],
        psych_families = psych_coverage.behavior_family_count,
        psych_gaps = psych_coverage.tracked_gap_count,
    );
}

fn canonical_manifest_ids(cases: &[SuiteCase], coverage: &SuiteCoverage) -> BTreeSet<String> {
    let upstream = unique_set("upstream cases", &coverage.upstream_cases);
    let selected = unique_set("selected cases", &coverage.selected_cases);
    let not_imported = unique_set("not-imported cases", &coverage.not_imported_cases);
    let aliases = coverage
        .local_case_alias
        .iter()
        .map(|alias| (alias.manifest_id.as_str(), alias.upstream_id.as_str()))
        .collect::<BTreeMap<_, _>>();

    assert!(selected.is_subset(&upstream));
    assert!(not_imported.is_subset(&upstream));
    assert!(selected.is_disjoint(&not_imported));
    assert_eq!(
        selected
            .union(&not_imported)
            .cloned()
            .collect::<BTreeSet<_>>(),
        upstream
    );

    cases
        .iter()
        .map(|case| {
            if selected.contains(&case.id) {
                case.id.clone()
            } else {
                aliases
                    .get(case.id.as_str())
                    .unwrap_or_else(|| panic!("missing upstream alias for {}", case.id))
                    .to_string()
            }
        })
        .collect()
}

fn documented_divergence_ids(parity: &SuiteParity) -> BTreeSet<String> {
    parity
        .event_deferred
        .iter()
        .chain(&parity.tree_deferred)
        .chain(&parity.shared_reference_deferred)
        .cloned()
        .collect()
}

fn psych_policy_counts(cases: &[PsychCase]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        *counts.entry(case.rust_policy.as_str()).or_insert(0) += 1;
    }
    counts
}

fn unique_set(label: &str, values: &[String]) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for value in values {
        assert!(set.insert(value.clone()), "duplicate {label} entry {value}");
    }
    set
}
