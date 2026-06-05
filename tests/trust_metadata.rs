use std::collections::BTreeSet;

use saneyaml::Value;

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const README: &str = include_str!("../README.md");
const ARCHITECTURE: &str = include_str!("../docs/ARCHITECTURE.md");
const BENCHMARKS: &str = include_str!("../docs/BENCHMARKS.md");
const OVERVIEW_SOURCE: &str = include_str!("../docs/assets/saneyaml-overview.md");
const MIGRATION: &str = include_str!("../docs/MIGRATION.md");
const SECURITY: &str = include_str!("../SECURITY.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const CONTRIBUTING: &str = include_str!("../CONTRIBUTING.md");
const FEATURE_CLIPPY: &str = include_str!("../scripts/check-feature-clippy.sh");
const PR_TEMPLATE: &str = include_str!("../.github/pull_request_template.md");
const ISSUE_CONFIG: &str = include_str!("../.github/ISSUE_TEMPLATE/config.yml");
const BUG_TEMPLATE: &str = include_str!("../.github/ISSUE_TEMPLATE/bug_report.yml");
const COMPAT_TEMPLATE: &str = include_str!("../.github/ISSUE_TEMPLATE/compatibility_report.yml");
const FUZZ_TEMPLATE: &str = include_str!("../.github/ISSUE_TEMPLATE/fuzz_crash.yml");

#[test]
fn security_policy_states_supported_preview_and_limits() {
    for term in [
        "current repository",
        "`main` line only",
        "jskoiz/saneyaml",
        "default 64 MiB input byte ceiling",
        "max_alias_expansion_nodes()",
        "Recursive aliases are rejected",
        "Reader-backed entrypoints still fully buffer",
        "ten fuzz targets",
        "scripts/fuzz-release-sweep.sh",
    ] {
        assert_contains(SECURITY, term);
    }
}

#[test]
fn changelog_and_contributing_do_not_claim_publication() {
    for term in ["Keep a Changelog", "saneyaml", "## 0.1.0"] {
        assert_contains(CHANGELOG, term);
    }

    for term in [
        "Rust 1.88",
        "scripts/check-public-api.sh",
        "Runtime dependencies remain limited to direct `ryu` and `serde`",
        "hosted Linux and Windows runners",
    ] {
        assert_contains(CONTRIBUTING, term);
    }
}

#[test]
fn migration_release_wording_tracks_manifest_metadata() {
    let manifest = package_manifest();
    let name = package_field(&manifest, "name");
    let version = package_field(&manifest, "version");
    let license = package_field(&manifest, "license");
    let expected_status = format!(
        "| Package status | `Cargo.toml` declares `{name}` {version} under the {license} license. |"
    );

    assert_contains(MIGRATION, &expected_status);
    assert_contains(
        MIGRATION,
        "Keep the named external crate build trials current before broadening ecosystem\n  replacement claims.",
    );
    assert!(
        !MIGRATION.contains("prepared as a 0.1.0"),
        "migration package status must track Cargo.toml instead of a stale release literal"
    );
    assert!(
        !MIGRATION.contains("Expand external crate build trials before claiming broad ecosystem"),
        "migration follow-up must not imply named external trials are still missing"
    );
}

#[test]
fn public_dependency_snippets_track_manifest_version() {
    let manifest = package_manifest();
    let name = package_field(&manifest, "name");
    let version = package_field(&manifest, "version");
    let direct_dependency = format!("{name} = \"{version}\"");
    let package_alias = format!("serde_yaml = {{ package = \"{name}\", version = \"{version}\" }}");

    assert_contains(README, &direct_dependency);
    assert_contains(MIGRATION, &direct_dependency);
    assert_contains(ARCHITECTURE, &direct_dependency);
    assert_contains(MIGRATION, &package_alias);
    assert_contains(ARCHITECTURE, &package_alias);

    for (path, source) in [
        ("README.md", README),
        ("docs/MIGRATION.md", MIGRATION),
        ("docs/ARCHITECTURE.md", ARCHITECTURE),
    ] {
        assert!(
            !source.contains("saneyaml = \"0.1\""),
            "{path} must not carry stale 0.1 direct dependency snippets"
        );
        assert!(
            !source.contains("package = \"saneyaml\", version = \"0.1\""),
            "{path} must not carry stale 0.1 package-alias snippets"
        );
    }
}

#[test]
fn packaged_benchmark_docs_mark_checkout_only_commands() {
    let package_entries = package_include_entries();

    assert!(package_entries.contains("docs/BENCHMARKS.md"));
    assert!(package_entries.contains("docs/assets/saneyaml-overview.md"));
    for dev_only_example in [
        "examples/real_world_benchmark.rs",
        "examples/large_input_benchmark.rs",
        "examples/dhat_memory.rs",
        "examples/conformance_compare.rs",
    ] {
        assert!(
            !package_entries.contains(dev_only_example),
            "{dev_only_example} should stay checkout-only unless package docs are reworded"
        );
    }

    for term in [
        "source-checkout-only",
        "dev-dependency examples and fixture corpora",
        "| captured section | checkout-only command |",
    ] {
        assert_contains(BENCHMARKS, term);
    }
    assert_contains(OVERVIEW_SOURCE, "Source-checkout-only captured command");
}

#[test]
fn github_templates_parse_as_yaml_and_route_sensitive_reports() {
    for (path, source) in [
        (".github/ISSUE_TEMPLATE/config.yml", ISSUE_CONFIG),
        (".github/ISSUE_TEMPLATE/bug_report.yml", BUG_TEMPLATE),
        (
            ".github/ISSUE_TEMPLATE/compatibility_report.yml",
            COMPAT_TEMPLATE,
        ),
        (".github/ISSUE_TEMPLATE/fuzz_crash.yml", FUZZ_TEMPLATE),
    ] {
        saneyaml::parse_str(source).unwrap_or_else(|err| panic!("{path} parses as YAML: {err}"));
    }

    assert_contains(ISSUE_CONFIG, "/security/advisories/new");
    assert_contains(BUG_TEMPLATE, "Use SECURITY.md for vulnerabilities");
    assert_contains(FUZZ_TEMPLATE, "report it privately through SECURITY.md");
    assert_contains(PR_TEMPLATE, "No manual hosted workflow run");
}

#[test]
fn ci_triggers_on_public_package_claim_inputs() {
    let workflow = ci_workflow();
    let push_branches = ci_string_sequence_for(&workflow, &["on", "push", "branches"]);
    let rust_runner_matrix =
        ci_string_sequence_for(&workflow, &["jobs", "rust", "strategy", "matrix", "os"]);
    let push_filters = ci_path_filters_for(&workflow, "push");
    let pull_request_filters = ci_path_filters_for(&workflow, "pull_request");

    assert_eq!(
        push_branches,
        BTreeSet::from(["main".to_owned()]),
        "push CI must stay limited to main so PR branch updates do not duplicate pull_request runs"
    );
    assert_eq!(
        rust_runner_matrix,
        BTreeSet::from(["ubuntu-latest".to_owned(), "windows-latest".to_owned()]),
        "automatic CI must avoid hosted Apple runners unless a specific run is approved"
    );

    let required_filters = public_package_claim_filters()
        .into_iter()
        .chain(trust_metadata_input_filters())
        .collect::<BTreeSet<_>>();
    for filter in required_filters {
        assert!(
            push_filters.contains(&filter),
            "expected {filter:?} in push CI path filters"
        );
        assert!(
            pull_request_filters.contains(&filter),
            "expected {filter:?} in pull_request CI path filters"
        );
    }
}

#[test]
fn feature_clippy_covers_non_default_feature_matrix() {
    assert_contains(
        FEATURE_CLIPPY,
        "cargo clippy --locked --no-default-features --lib -- -D warnings",
    );
    assert_contains(FEATURE_CLIPPY, "for features in serde emit serde,emit; do");
    assert_contains(
        FEATURE_CLIPPY,
        "cargo clippy --locked --no-default-features --features \"$features\" --lib -- -D warnings",
    );
    assert_contains(
        FEATURE_CLIPPY,
        "for features in lossless serde,lossless emit,lossless; do",
    );
    assert_contains(
        FEATURE_CLIPPY,
        "cargo clippy --locked --no-default-features --features \"$features\" --all-targets -- -D warnings",
    );
}

fn public_package_claim_filters() -> BTreeSet<String> {
    package_include_entries()
        .into_iter()
        .filter_map(|path| {
            if path.starts_with("docs/") {
                Some("docs/**".to_owned())
            } else if path == "Cargo.lock" || path == "Cargo.toml" || path.ends_with(".md") {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

fn trust_metadata_input_filters() -> BTreeSet<String> {
    BTreeSet::from([
        ".github/ISSUE_TEMPLATE/**".to_owned(),
        ".github/pull_request_template.md".to_owned(),
        ".github/workflows/ci.yml".to_owned(),
    ])
}

fn package_include_entries() -> BTreeSet<String> {
    let manifest = package_manifest();
    let package = manifest
        .get("package")
        .expect("Cargo.toml has [package] metadata");
    let include = package
        .get("include")
        .and_then(toml::Value::as_array)
        .expect("Cargo.toml package.include is an array");

    include
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .expect("Cargo.toml package.include entries are strings")
                .trim_start_matches('/')
                .to_owned()
        })
        .collect()
}

fn ci_workflow() -> Value {
    saneyaml::parse_str(CI_WORKFLOW)
        .unwrap_or_else(|err| panic!(".github/workflows/ci.yml parses as YAML: {err}"))
        .into_value()
}

fn ci_path_filters_for(workflow: &Value, trigger: &str) -> BTreeSet<String> {
    ci_string_sequence_for(workflow, &["on", trigger, "paths"])
}

fn ci_string_sequence_for(workflow: &Value, path: &[&str]) -> BTreeSet<String> {
    path.iter()
        .try_fold(workflow, |value, key| value.get(*key))
        .and_then(Value::as_sequence)
        .unwrap_or_else(|| panic!("CI workflow has {}", path.join(".")))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("CI workflow {} entries are strings", path.join(".")))
                .to_owned()
        })
        .collect()
}

fn package_manifest() -> toml::Value {
    toml::from_str(CARGO_TOML).expect("Cargo.toml parses")
}

fn package_field<'a>(manifest: &'a toml::Value, field: &str) -> &'a str {
    manifest
        .get("package")
        .and_then(|package| package.get(field))
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("Cargo.toml package.{field} is a string"))
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected artifact to contain {needle:?}"
    );
}
