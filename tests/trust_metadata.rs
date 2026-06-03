use std::collections::BTreeSet;

use saneyaml::Value;

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const MIGRATION: &str = include_str!("../docs/MIGRATION.md");
const SECURITY: &str = include_str!("../SECURITY.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const CONTRIBUTING: &str = include_str!("../CONTRIBUTING.md");
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
    let push_filters = ci_path_filters_for(&workflow, "push");
    let pull_request_filters = ci_path_filters_for(&workflow, "pull_request");

    assert_eq!(
        push_branches,
        BTreeSet::from(["main".to_owned()]),
        "push CI must stay limited to main so PR branch updates do not duplicate pull_request runs"
    );

    let required_filters = public_package_claim_filters();
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
