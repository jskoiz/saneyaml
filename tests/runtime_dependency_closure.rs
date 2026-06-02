use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const EXPECTED_NO_DEV_TREE: &str = include_str!("fixtures/runtime-dependencies/no-dev-tree.txt");

#[test]
fn direct_runtime_dependencies_stay_minimal() {
    let cargo: toml::Value = toml::from_str(CARGO_TOML).expect("Cargo.toml parses");
    let dependencies = cargo["dependencies"]
        .as_table()
        .expect("dependencies table exists");
    let actual = dependencies
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, BTreeSet::from(["ryu", "serde"]));

    assert!(
        cargo.get("build-dependencies").is_none(),
        "build-dependencies would extend the runtime/build closure"
    );
    assert!(
        cargo.get("target").is_none(),
        "target-specific runtime dependencies must be audited explicitly"
    );
}

#[test]
fn resolved_no_dev_runtime_tree_matches_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO"))
        .args(["tree", "--locked", "-e", "no-dev", "--prefix", "none"])
        .current_dir(root)
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = String::from_utf8(output.stdout).expect("cargo tree output is UTF-8");
    let package_names = actual
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split_whitespace().next().expect("package name"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(package_names, EXPECTED_NO_DEV_TREE);
}
