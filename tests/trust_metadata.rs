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
        "package-ready candidate",
        "current repository",
        "`main` line only",
        "GitHub private vulnerability reporting",
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
    for term in [
        "Keep a Changelog",
        "saneyaml",
        "does not announce a crates.io release",
        "0.1.0 release-candidate work in progress",
    ] {
        assert_contains(CHANGELOG, term);
    }

    for term in [
        "Rust 1.85",
        "cargo test --locked --test baseline_audit",
        "scripts/check-public-api.sh",
        "Runtime dependencies remain limited to direct `ryu` and `serde`",
        "hosted macOS/Windows runs",
    ] {
        assert_contains(CONTRIBUTING, term);
    }
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
        yaml::parse_str(source).unwrap_or_else(|err| panic!("{path} parses as YAML: {err}"));
    }

    assert_contains(ISSUE_CONFIG, "/security/advisories/new");
    assert_contains(BUG_TEMPLATE, "Use SECURITY.md for vulnerabilities");
    assert_contains(FUZZ_TEMPLATE, "report it privately through SECURITY.md");
    assert_contains(PR_TEMPLATE, "No hosted macOS/Windows workflow run");
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected artifact to contain {needle:?}"
    );
}
