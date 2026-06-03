//! Head-to-head conformance comparison against the selected YAML test-suite
//! corpus. For every curated case we feed the fixture to both `saneyaml` and
//! the currently-pinned `serde_yaml`, then score each library against the
//! manifest's expected outcome.
//!
//! Run with: `cargo run --example conformance_compare`
//!
//! The manifest classifies each case as:
//!   - `accept`       — valid YAML; a tree/Value load should succeed
//!   - `syntax-error` — invalid YAML; loading should be rejected
//!   - `tree-error`   — valid event stream, but tree loading must reject it
//!                      (e.g. duplicate keys). This is the "sane" policy axis.
//!
//! The 400 accept + syntax-error cases are spec-derived and form the neutral
//! head-to-head. The 2 tree-error cases reflect saneyaml's stricter
//! duplicate-key policy and are reported separately.

use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
    name: String,
    expected: ExpectedOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExpectedOutcome {
    Accept,
    SyntaxError,
    TreeError,
}

impl SuiteCase {
    fn fixture_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/yaml-test-suite/data")
            .join(self.id.replace('/', "-"))
            .join("in.yaml")
    }
}

/// Whether a tree/Value load of `input` succeeds, for each library.
fn saneyaml_accepts(input: &str) -> bool {
    saneyaml::from_documents_str::<saneyaml::Value>(input).is_ok()
}

fn serde_yaml_accepts(input: &str) -> bool {
    // serde_yaml has no multi-document loader on a single call; the suite's
    // multi-doc cases are exercised via its document iterator so we don't
    // penalise it for a single-doc-only `from_str`.
    for doc in serde_yaml::Deserializer::from_str(input) {
        if serde_yaml::Value::deserialize(doc).is_err() {
            return false;
        }
    }
    // An empty stream is a valid (null) document for both libraries.
    true
}

fn yaml_rust2_accepts(input: &str) -> bool {
    yaml_rust2::YamlLoader::load_from_str(input).is_ok()
}

fn saphyr_accepts(input: &str) -> bool {
    use saphyr::LoadableYamlNode;
    saphyr::Yaml::load_from_str(input).is_ok()
}

#[derive(Default)]
struct Tally {
    pass: usize,
    fail: usize,
    mismatches: Vec<String>,
}

impl Tally {
    fn record(&mut self, ok: bool, case: &SuiteCase, observed: &str) {
        if ok {
            self.pass += 1;
        } else {
            self.fail += 1;
            self.mismatches
                .push(format!("{} ({}) — {}", case.id, case.name, observed));
        }
    }

    fn total(&self) -> usize {
        self.pass + self.fail
    }

    fn rate(&self) -> f64 {
        if self.total() == 0 {
            return 100.0;
        }
        (self.pass as f64) * 100.0 / (self.total() as f64)
    }
}

fn main() {
    let manifest: SuiteManifest = toml::from_str(include_str!(
        "../tests/fixtures/yaml-test-suite/manifest.toml"
    ))
    .expect("manifest is valid TOML");

    let libraries: &[(&str, fn(&str) -> bool)] = &[
        ("saneyaml", saneyaml_accepts),
        ("serde_yaml", serde_yaml_accepts),
        ("yaml-rust2", yaml_rust2_accepts),
        ("saphyr", saphyr_accepts),
    ];

    // Spec-derived neutral comparison (accept + syntax-error) and the
    // saneyaml-policy axis (duplicate-key / tree-error), one tally per library.
    let mut spec: Vec<Tally> = libraries.iter().map(|_| Tally::default()).collect();
    let mut tree: Vec<Tally> = libraries.iter().map(|_| Tally::default()).collect();

    for case in &manifest.case {
        let input = fs::read_to_string(case.fixture_path())
            .unwrap_or_else(|e| panic!("read fixture {}: {e}", case.id));

        for (idx, (_, accepts)) in libraries.iter().enumerate() {
            let accepted = accepts(&input);
            match case.expected {
                ExpectedOutcome::Accept => {
                    spec[idx].record(accepted, case, "rejected a valid document");
                }
                ExpectedOutcome::SyntaxError => {
                    spec[idx].record(!accepted, case, "accepted invalid YAML");
                }
                ExpectedOutcome::TreeError => {
                    tree[idx].record(!accepted, case, "accepted a duplicate-key/tree-error doc");
                }
            }
        }
    }

    println!("YAML test-suite conformance — saneyaml vs serde_yaml / yaml-rust2 / saphyr\n");
    println!(
        "Corpus: {} curated cases ({} spec accept/reject + {} tree-policy)\n",
        manifest.case.len(),
        spec[0].total(),
        tree[0].total()
    );

    println!("Spec accept/reject (neutral, YAML-test-suite-derived):");
    for (idx, (name, _)) in libraries.iter().enumerate() {
        let t = &spec[idx];
        let too_strict = t
            .mismatches
            .iter()
            .filter(|m| m.contains("rejected a valid document"))
            .count();
        let too_lax = t
            .mismatches
            .iter()
            .filter(|m| m.contains("accepted invalid YAML"))
            .count();
        println!(
            "  {name:<12} {:>4}/{:<4}  {:6.2}%   ({} too strict, {} too lax)",
            t.pass,
            t.total(),
            t.rate(),
            too_strict,
            too_lax
        );
    }
    println!();

    println!("Tree policy (duplicate-key / tree-error rejection):");
    for (idx, (name, _)) in libraries.iter().enumerate() {
        let t = &tree[idx];
        println!(
            "  {name:<12} {:>4}/{:<4}  {:6.2}%",
            t.pass,
            t.total(),
            t.rate()
        );
    }
    println!();

    for (idx, (name, _)) in libraries.iter().enumerate() {
        let t = &spec[idx];
        if t.mismatches.is_empty() {
            continue;
        }
        println!("{name} spec mismatches ({}):", t.mismatches.len());
        for m in t.mismatches.iter().take(40) {
            println!("  - {m}");
        }
        if t.mismatches.len() > 40 {
            println!("  … and {} more", t.mismatches.len() - 40);
        }
        println!();
    }
}
