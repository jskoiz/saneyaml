//! Allocator-backed memory measurement (dhat) for a single parse of an
//! identical generated corpus, one library per process invocation.
//!
//! dhat installs a global allocator, so each library must be measured in its
//! own process to keep the numbers clean. Run one at a time:
//!
//!   cargo run --release --example dhat_memory -- saneyaml-borrowed multidoc
//!   cargo run --release --example dhat_memory -- saphyr           multidoc
//!
//! Or use the bundled driver that sweeps every (library, corpus) pair:
//!
//!   cargo run --release --example dhat_memory -- --all
//!
//! Reported per parse (deltas measured *after* the input is generated, so the
//! ~1 MiB input string is excluded from the figures):
//!   - allocations    : number of heap allocations made during the parse
//!   - bytes allocated : total bytes allocated during the parse
//!   - retained        : heap still live while the parsed tree is held
//!   - peak            : max simultaneously-live heap during the parse

use std::env;
use std::hint::black_box;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const LIBS: &[&str] = &[
    "saneyaml-borrowed",
    "saneyaml-owned",
    "saneyaml-value",
    "serde_yaml",
    "yaml-rust2",
    "saphyr",
];
const CORPORA: &[&str] = &["multidoc", "wide"];

fn generated_multi_doc_stream(target_bytes: usize) -> String {
    let mut input = String::with_capacity(target_bytes + 256);
    let mut docs = 0usize;
    while input.len() < target_bytes {
        input.push_str("---\nservice:\n  name: app-");
        input.push_str(&docs.to_string());
        input.push_str("\n  image: ghcr.io/example/app:");
        input.push_str(&(docs % 97).to_string());
        input.push_str("\n  ports:\n    - ");
        input.push_str(&(8000 + docs % 1000).to_string());
        input.push_str("\n  env:\n    RUST_LOG: info\n    FEATURE_FLAG: true\n");
        docs += 1;
    }
    input
}

fn generated_wide_mapping(target_bytes: usize) -> String {
    let mut input = String::with_capacity(target_bytes + 256);
    input.push_str("services:\n");
    let mut idx = 0usize;
    while input.len() < target_bytes {
        input.push_str("  service-");
        input.push_str(&idx.to_string());
        input.push_str(":\n    image: ghcr.io/example/service:");
        input.push_str(&(idx % 113).to_string());
        input.push_str("\n    replicas: ");
        input.push_str(&(1 + idx % 9).to_string());
        input.push_str("\n    enabled: true\n");
        idx += 1;
    }
    input
}

fn corpus_input(corpus: &str) -> String {
    match corpus {
        "multidoc" => generated_multi_doc_stream(1024 * 1024),
        "wide" => generated_wide_mapping(1024 * 1024),
        other => panic!("unknown corpus {other:?}; use multidoc | wide"),
    }
}

fn report(lib: &str, corpus: &str, before: &dhat::HeapStats, after: &dhat::HeapStats) {
    let allocations = after.total_blocks - before.total_blocks;
    let bytes_alloc = after.total_bytes - before.total_bytes;
    let retained = after.curr_bytes.saturating_sub(before.curr_bytes);
    let retained_blocks = after.curr_blocks.saturating_sub(before.curr_blocks);
    let peak = after.max_bytes;
    println!(
        "{lib:<18} {corpus:<9} alloc={allocations:>8}  bytes_alloc={bytes_alloc:>10}  retained_bytes={retained:>10}  retained_blocks={retained_blocks:>8}  peak_bytes={peak:>10}"
    );
}

fn measure(lib: &str, corpus: &str) {
    let input = corpus_input(corpus);

    // `before` is snapshotted after the input string exists, so the ~1 MiB
    // input is excluded. The parsed value is held (black_box) across `after`
    // so retained memory reflects the live tree. The borrowed tree borrows
    // from `input`, hence the per-arm measurement rather than a boxed return.
    macro_rules! measure_arm {
        ($parse:expr) => {{
            let before = dhat::HeapStats::get();
            let held = $parse;
            let after = dhat::HeapStats::get();
            black_box(&held);
            report(lib, corpus, &before, &after);
        }};
    }

    match lib {
        "saneyaml-borrowed" => {
            measure_arm!(saneyaml::parse_borrowed_documents(&input).expect("saneyaml borrowed"))
        }
        "saneyaml-owned" => {
            measure_arm!(saneyaml::parse_documents(&input).expect("saneyaml owned"))
        }
        "saneyaml-value" => measure_arm!(
            saneyaml::from_documents_str::<saneyaml::Value>(&input).expect("saneyaml value")
        ),
        "serde_yaml" => {
            use serde::Deserialize;
            measure_arm!(serde_yaml::Deserializer::from_str(&input)
                .map(|d| serde_yaml::Value::deserialize(d).expect("serde_yaml value"))
                .collect::<Vec<serde_yaml::Value>>())
        }
        "yaml-rust2" => {
            measure_arm!(yaml_rust2::YamlLoader::load_from_str(&input).expect("yaml-rust2 load"))
        }
        "saphyr" => {
            use saphyr::LoadableYamlNode;
            measure_arm!(saphyr::Yaml::load_from_str(&input).expect("saphyr load"))
        }
        other => panic!("unknown lib {other:?}"),
    }

    black_box(input.len());
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("--all") {
        // Re-exec one child process per (lib, corpus) so each gets a clean
        // global allocator and an isolated dhat profiler.
        let exe = env::current_exe().expect("current exe");
        println!("dhat allocator-backed memory — 1 MiB corpora, single parse\n");
        for corpus in CORPORA {
            for lib in LIBS {
                let status = std::process::Command::new(&exe)
                    .args([lib, corpus])
                    .status()
                    .expect("spawn child");
                assert!(status.success(), "child {lib}/{corpus} failed");
            }
            println!();
        }
        return;
    }

    if args.first().map(String::as_str) == Some("--profile") {
        // Writes dhat-heap.json with per-call-site allocation stats.
        let lib = args.get(1).map(String::as_str).unwrap_or("saneyaml-borrowed");
        let corpus = args.get(2).map(String::as_str).unwrap_or("multidoc");
        let _profiler = dhat::Profiler::new_heap();
        let input = corpus_input(corpus);
        match lib {
            "saneyaml-borrowed" => {
                black_box(saneyaml::parse_borrowed_documents(&input).expect("borrowed"));
            }
            "saphyr" => {
                use saphyr::LoadableYamlNode;
                black_box(saphyr::Yaml::load_from_str(&input).expect("saphyr"));
            }
            other => panic!("profile supports saneyaml-borrowed | saphyr, got {other:?}"),
        }
        black_box(input.len());
        return;
    }

    let lib = args.first().map(String::as_str).unwrap_or("saneyaml-borrowed");
    let corpus = args.get(1).map(String::as_str).unwrap_or("multidoc");

    let _profiler = dhat::Profiler::builder().testing().build();
    measure(lib, corpus);
}
