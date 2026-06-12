//! Generates the static conformance-matrix web page from the selected YAML
//! test-suite corpus. Every curated case is fed to `saneyaml`, `serde_yaml`,
//! `yaml-rust2`, and `saphyr`, and each library is scored against the
//! manifest's expected outcome — the same methodology as
//! `examples/conformance_compare.rs`, rendered as a self-contained HTML page.
//!
//! Run with: `cargo run --locked --example conformance_matrix_html`
//!
//! The page is written to `docs/conformance/index.html` (override with the
//! first CLI argument) and is committed so the published guide can serve it
//! without building dev-dependencies.

use serde::Deserialize;
use std::fmt::Write as _;
use std::panic::{self, AssertUnwindSafe};
use std::{fs, path::PathBuf};

/// A library's tree/Value acceptance predicate for one input.
type AcceptFn = fn(&str) -> bool;

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    case: Vec<SuiteCase>,
}

#[derive(Debug, Deserialize)]
struct SuiteCase {
    id: String,
    name: String,
    expected: ExpectedOutcome,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExpectedOutcome {
    Accept,
    SyntaxError,
    TreeError,
}

impl ExpectedOutcome {
    fn label(self) -> &'static str {
        match self {
            ExpectedOutcome::Accept => "accept",
            ExpectedOutcome::SyntaxError => "syntax-error",
            ExpectedOutcome::TreeError => "tree-error",
        }
    }
}

#[derive(Debug, Deserialize)]
struct SuiteCoverage {
    local_case_alias: Vec<LocalCaseAlias>,
}

#[derive(Debug, Deserialize)]
struct LocalCaseAlias {
    manifest_id: String,
    upstream_id: String,
}

#[derive(Debug, Deserialize)]
struct SuiteSource {
    upstream: String,
    data_branch_commit: String,
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

/// Reference parsers are third-party code; a panic on hostile suite input
/// counts as a rejection rather than aborting the whole matrix run.
fn accepts_guarded(accepts: AcceptFn, input: &str) -> bool {
    panic::catch_unwind(AssertUnwindSafe(|| accepts(input))).unwrap_or(false)
}

struct Library {
    name: &'static str,
    version: String,
    accepts: AcceptFn,
}

#[derive(Default)]
struct Score {
    spec_pass: usize,
    too_strict: usize,
    too_lax: usize,
    tree_pass: usize,
    tree_total: usize,
}

impl Score {
    fn spec_total(&self) -> usize {
        self.spec_pass + self.too_strict + self.too_lax
    }

    fn spec_rate(&self) -> f64 {
        (self.spec_pass as f64) * 100.0 / (self.spec_total() as f64)
    }
}

fn locked_version(lock: &str, name: &str) -> String {
    let needle = format!("name = \"{name}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == needle
            && let Some(version) = lines
                .next()
                .and_then(|next| next.trim().strip_prefix("version = \""))
        {
            return version.trim_end_matches('"').to_string();
        }
    }
    "unknown".to_string()
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn main() {
    let manifest: SuiteManifest = toml::from_str(include_str!(
        "../tests/fixtures/yaml-test-suite/manifest.toml"
    ))
    .expect("manifest is valid TOML");
    let coverage: SuiteCoverage = toml::from_str(include_str!(
        "../tests/fixtures/yaml-test-suite/coverage.toml"
    ))
    .expect("coverage is valid TOML");
    let source: SuiteSource = toml::from_str(include_str!(
        "../tests/fixtures/yaml-test-suite/SOURCE.toml"
    ))
    .expect("SOURCE is valid TOML");

    let lock = include_str!("../Cargo.lock");
    let libraries = [
        Library {
            name: "saneyaml",
            version: env!("CARGO_PKG_VERSION").to_string(),
            accepts: saneyaml_accepts,
        },
        Library {
            name: "serde_yaml",
            version: locked_version(lock, "serde_yaml"),
            accepts: serde_yaml_accepts,
        },
        Library {
            name: "yaml-rust2",
            version: locked_version(lock, "yaml-rust2"),
            accepts: yaml_rust2_accepts,
        },
        Library {
            name: "saphyr",
            version: locked_version(lock, "saphyr"),
            accepts: saphyr_accepts,
        },
    ];

    let upstream_ids: std::collections::BTreeMap<&str, &str> = coverage
        .local_case_alias
        .iter()
        .map(|alias| (alias.manifest_id.as_str(), alias.upstream_id.as_str()))
        .collect();

    let mut cases = manifest.case;
    cases.sort_by(|a, b| a.id.cmp(&b.id));

    // Silence panic backtraces from guarded third-party parser calls.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut scores: Vec<Score> = libraries.iter().map(|_| Score::default()).collect();
    let mut rows = String::new();
    for case in &cases {
        let input = fs::read_to_string(case.fixture_path())
            .unwrap_or_else(|e| panic!("read fixture {}: {e}", case.id));

        let mut cells = String::new();
        let mut row_mismatch = false;
        for (library, score) in libraries.iter().zip(scores.iter_mut()) {
            let accepted = accepts_guarded(library.accepts, &input);
            let (class, mark) = match case.expected {
                ExpectedOutcome::Accept => {
                    if accepted {
                        score.spec_pass += 1;
                        ("ok", "&#10003;")
                    } else {
                        score.too_strict += 1;
                        row_mismatch = true;
                        ("bad", "&#10007;")
                    }
                }
                ExpectedOutcome::SyntaxError => {
                    if accepted {
                        score.too_lax += 1;
                        row_mismatch = true;
                        ("bad", "&#10003;")
                    } else {
                        score.spec_pass += 1;
                        ("ok", "&#10007;")
                    }
                }
                ExpectedOutcome::TreeError => {
                    score.tree_total += 1;
                    if accepted {
                        row_mismatch = true;
                        ("policy", "&#10003;")
                    } else {
                        score.tree_pass += 1;
                        ("ok", "&#10007;")
                    }
                }
            };
            let _ = write!(cells, "<td class=\"cell {class}\">{mark}</td>");
        }

        let upstream_id = upstream_ids
            .get(case.id.as_str())
            .copied()
            .unwrap_or(case.id.as_str());
        let case_url = format!(
            "{}/tree/{}/{}",
            source.upstream, source.data_branch_commit, upstream_id
        );
        let features = case.features.join(" ");
        let _ = writeln!(
            rows,
            "<tr data-expected=\"{expected}\" data-mismatch=\"{mismatch}\" \
             data-text=\"{id_lower} {name_lower} {features_lower}\">\
             <td class=\"id\"><a href=\"{url}\">{id}</a></td>\
             <td class=\"name\">{name}<span class=\"tags\">{tags}</span></td>\
             <td class=\"expected\"><span class=\"chip {expected}\">{expected}</span></td>\
             {cells}</tr>",
            expected = case.expected.label(),
            mismatch = row_mismatch,
            id_lower = escape_html(&case.id.to_lowercase()),
            name_lower = escape_html(&case.name.to_lowercase()),
            features_lower = escape_html(&features.to_lowercase()),
            url = escape_html(&case_url),
            id = escape_html(&case.id),
            name = escape_html(&case.name),
            tags = escape_html(&features),
        );
    }
    panic::set_hook(default_hook);

    let total = cases.len();
    let spec_total = scores[0].spec_total();
    let tree_total = scores[0].tree_total;
    let mut cards = String::new();
    for (library, score) in libraries.iter().zip(scores.iter()) {
        let _ = writeln!(
            cards,
            "<div class=\"card\"><h3>{name} <small>{version}</small></h3>\
             <p class=\"big\">{pass}/{spec_total} <small>{rate:.1}%</small></p>\
             <p class=\"detail\">{strict} too strict &middot; {lax} too lax &middot; \
             tree policy {tree_pass}/{tree_total}</p></div>",
            name = escape_html(library.name),
            version = escape_html(&library.version),
            pass = score.spec_pass,
            rate = score.spec_rate(),
            strict = score.too_strict,
            lax = score.too_lax,
            tree_pass = score.tree_pass,
        );
    }

    let mut header_cells = String::new();
    for library in &libraries {
        let _ = write!(header_cells, "<th>{}</th>", escape_html(library.name));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>saneyaml — YAML test-suite conformance matrix</title>
<style>
:root {{ color-scheme: light dark; --ok: #1a7f37; --bad: #cf222e; --policy: #9a6700; --border: #d0d7de; --muted: #57606a; --bg: #ffffff; --fg: #1f2328; --head-bg: #f6f8fa; --ok-bg: #f0fff4; --bad-bg: #fff5f5; --policy-bg: #fff8e5; --link: #0969da; }}
@media (prefers-color-scheme: dark) {{
  :root {{ --ok: #3fb950; --bad: #f85149; --policy: #d29922; --border: #30363d; --muted: #8b949e; --bg: #0d1117; --fg: #e6edf3; --head-bg: #161b22; --ok-bg: #12261e; --bad-bg: #2d1517; --policy-bg: #272115; --link: #4493f8; }}
}}
* {{ box-sizing: border-box; }}
body {{ font: 15px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; margin: 0; color: var(--fg); background: var(--bg); }}
a {{ color: var(--link); }}
main {{ max-width: 1080px; margin: 0 auto; padding: 24px 16px 64px; }}
h1 {{ margin-bottom: 4px; }}
.subtitle {{ color: var(--muted); margin-top: 0; }}
.cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 24px 0; }}
.card {{ border: 1px solid var(--border); border-radius: 8px; padding: 12px 16px; }}
.card h3 {{ margin: 0 0 4px; font-size: 16px; }}
.card h3 small {{ color: var(--muted); font-weight: normal; }}
.card .big {{ font-size: 26px; font-weight: 600; margin: 0; }}
.card .big small {{ font-size: 15px; color: var(--muted); font-weight: normal; }}
.card .detail {{ color: var(--muted); font-size: 13px; margin: 4px 0 0; }}
.controls {{ display: flex; flex-wrap: wrap; gap: 12px; align-items: center; margin: 16px 0; }}
.controls input[type="search"] {{ flex: 1 1 240px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-size: 14px; background: var(--bg); color: var(--fg); }}
.controls select {{ padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-size: 14px; background: var(--bg); color: var(--fg); }}
.controls label {{ font-size: 14px; color: var(--muted); display: flex; gap: 6px; align-items: center; }}
table {{ border-collapse: collapse; width: 100%; font-size: 14px; }}
th, td {{ border: 1px solid var(--border); padding: 5px 8px; text-align: left; }}
th {{ background: var(--head-bg); position: sticky; top: 0; }}
td.cell {{ text-align: center; font-weight: 600; width: 84px; }}
td.cell.ok {{ color: var(--ok); background: var(--ok-bg); }}
td.cell.bad {{ color: var(--bad); background: var(--bad-bg); }}
td.cell.policy {{ color: var(--policy); background: var(--policy-bg); }}
td.id a {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }}
td.name .tags {{ display: block; color: var(--muted); font-size: 12px; }}
.chip {{ font-size: 12px; padding: 1px 8px; border-radius: 999px; border: 1px solid var(--border); white-space: nowrap; }}
.chip.accept {{ color: var(--ok); }}
.chip.syntax-error {{ color: var(--bad); }}
.chip.tree-error {{ color: var(--policy); }}
.note {{ color: var(--muted); font-size: 14px; }}
footer {{ margin-top: 32px; color: var(--muted); font-size: 13px; border-top: 1px solid var(--border); padding-top: 16px; }}
#count {{ color: var(--muted); font-size: 14px; }}
</style>
</head>
<body>
<main>
<h1>YAML test-suite conformance matrix</h1>
<p class="subtitle">Tree/<code>Value</code> loading of all {total} curated
<a href="{upstream}">yaml-test-suite</a> cases (pinned at
<code>{commit_short}</code>), scored against the manifest's expected outcome.
A &#10003; means the library accepted the input; green/red shows whether that
matches the expectation. Generated by
<code>cargo run --locked --example conformance_matrix_html</code> in the
<a href="https://github.com/jskoiz/saneyaml">saneyaml repository</a>.</p>
<div class="cards">
{cards}</div>
<p class="note">Spec score covers the {spec_total} neutral accept /
syntax-error cases. The {tree_total} <em>tree-error</em> cases (amber) are
valid YAML event streams that saneyaml's stricter duplicate-key policy rejects
at tree loading; they are scored separately so no library is penalised on the
neutral axis for a policy difference.</p>
<div class="controls">
<input id="search" type="search" placeholder="Filter by case id, name, or feature&hellip;">
<select id="expected">
<option value="">all outcomes</option>
<option value="accept">accept</option>
<option value="syntax-error">syntax-error</option>
<option value="tree-error">tree-error</option>
</select>
<label><input id="mismatch" type="checkbox"> only disagreements</label>
<span id="count"></span>
</div>
<table>
<thead><tr><th>Case</th><th>Name</th><th>Expected</th>{header_cells}</tr></thead>
<tbody id="rows">
{rows}</tbody>
</table>
<footer>
<p>Methodology: acceptance is whether a full tree/<code>Value</code> load of the
fixture succeeds (saneyaml via <code>from_documents_str</code>, serde_yaml via
its document iterator, yaml-rust2 via <code>YamlLoader</code>, saphyr via
<code>Yaml::load_from_str</code>). Case ids link to the pinned upstream
fixture. The selection manifest, per-case policies, and divergence registry
live under <code>tests/fixtures/yaml-test-suite/</code>; the counts here are
cross-checked by <code>tests/conformance_dashboard.rs</code>.</p>
</footer>
</main>
<script>
const search = document.getElementById("search");
const expected = document.getElementById("expected");
const mismatch = document.getElementById("mismatch");
const count = document.getElementById("count");
const rows = Array.from(document.getElementById("rows").children);
function apply() {{
  const needle = search.value.trim().toLowerCase();
  const outcome = expected.value;
  const onlyMismatch = mismatch.checked;
  let shown = 0;
  for (const row of rows) {{
    const visible =
      (!needle || row.dataset.text.includes(needle)) &&
      (!outcome || row.dataset.expected === outcome) &&
      (!onlyMismatch || row.dataset.mismatch === "true");
    row.style.display = visible ? "" : "none";
    if (visible) shown += 1;
  }}
  count.textContent = shown + " of " + rows.length + " cases";
}}
search.addEventListener("input", apply);
expected.addEventListener("change", apply);
mismatch.addEventListener("change", apply);
apply();
</script>
</body>
</html>
"#,
        total = total,
        upstream = escape_html(&source.upstream),
        commit_short = escape_html(
            source
                .data_branch_commit
                .get(..12)
                .unwrap_or(&source.data_branch_commit)
        ),
        cards = cards,
        spec_total = spec_total,
        tree_total = tree_total,
        header_cells = header_cells,
        rows = rows,
    );

    let output = std::env::args().nth(1).map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("docs/conformance")
                .join("index.html")
        },
        PathBuf::from,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create output directory");
    }
    fs::write(&output, html).expect("write conformance matrix page");

    println!("wrote {}", output.display());
    for (library, score) in libraries.iter().zip(scores.iter()) {
        println!(
            "  {:<12} spec {}/{} ({:.2}%), tree policy {}/{}",
            library.name,
            score.spec_pass,
            score.spec_total(),
            score.spec_rate(),
            score.tree_pass,
            score.tree_total,
        );
    }
}
