//! Head-to-head benchmark: `saneyaml` vs `serde-saphyr`.
//!
//! Both crates are Serde-based, so the fair comparison feeds the *same bytes*
//! into the *same target type* with each library, isolating the YAML layer.
//!
//! Two axes are measured:
//!   1. Dynamic value (`serde_json::Value`) over the real-world config corpus
//!      — the "parse arbitrary YAML into a tree" case.
//!   2. Typed structs over a generated config — serde-saphyr's advertised
//!      sweet spot (deserialize straight into structs, no intermediate tree),
//!      and saneyaml's primary use case.
//!
//! Run:
//!   cargo run --release --example serde_saphyr_headtohead
//!   YAML_BENCH_ITERS=1000 SVC_COUNT=5000 cargo run --release --example serde_saphyr_headtohead

use serde::Deserialize;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct Fixture {
    name: String,
    input: String,
    docs: usize,
}

fn collect_yaml(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_yaml(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            out.push(path);
        }
    }
}

fn measure<F: FnMut() -> u64>(iters: usize, mut run: F) -> (Duration, u64) {
    // Warm up so the first allocation/branch-predict pass is not timed.
    black_box(run());
    let start = Instant::now();
    let mut acc = 0u64;
    for _ in 0..iters {
        acc ^= black_box(run());
    }
    (start.elapsed(), acc)
}

fn ns_per_byte(elapsed: Duration, iters: usize, bytes: usize) -> f64 {
    elapsed.as_nanos() as f64 / (iters * bytes) as f64
}

fn row(label: &str, iters: usize, bytes: usize, elapsed: Duration) {
    println!(
        "| {label} | {iters} | {bytes} | {:.3} | {:.2} |",
        elapsed.as_secs_f64() * 1000.0,
        ns_per_byte(elapsed, iters, bytes),
    );
}

fn table_header() {
    println!("| load path | iterations | bytes/iter | elapsed ms | ns/byte |");
    println!("|---|---:|---:|---:|---:|");
}

// Both libraries are safe-by-default with resource caps (saneyaml:
// max_collection_items = 16384; serde-saphyr: ~250000 total nodes). The
// generated inputs are sized just under the tighter cap so BOTH run on their
// shipping defaults — no limits are lifted on either side.

// ---- Typed-struct workload -------------------------------------------------

// Fields are populated by Serde during deserialization; the benchmark never
// reads them back, so silence the dead-code lint for this fixture type.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Service {
    name: String,
    image: String,
    replicas: u32,
    enabled: bool,
    weight: f64,
    ports: Vec<u16>,
    tags: Vec<String>,
    env: BTreeMap<String, String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Config {
    version: String,
    services: Vec<Service>,
}

// A flat, homogeneous record — the shape closest to serde-saphyr's own
// published "big file of simple records" benchmark, to check the ratio is not
// an artifact of the nested/BTreeMap shape above.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Record {
    id: u64,
    name: String,
    active: bool,
    score: f64,
}

fn generate_records(n: usize) -> String {
    let mut s = String::with_capacity(n * 64);
    for i in 0..n {
        s.push_str(&format!(
            "- id: {i}\n  name: item-{i}\n  active: {}\n  score: {}.{}\n",
            i % 2 == 0,
            i % 100,
            i % 1000,
        ));
    }
    s
}

fn generate_typed(service_count: usize) -> String {
    let mut s = String::with_capacity(service_count * 220);
    s.push_str("version: \"3.8\"\nservices:\n");
    for i in 0..service_count {
        s.push_str(&format!(
            "  - name: service-{i}\n    \
               image: registry.example.com/team/app:{i}\n    \
               replicas: {}\n    \
               enabled: {}\n    \
               weight: {}.{}\n    \
               ports: [{}, {}]\n    \
               tags: [web, prod, region-{}]\n    \
               env:\n      \
                 LOG_LEVEL: info\n      \
                 PORT: \"{}\"\n      \
                 INDEX: \"{i}\"\n",
            (i % 8) + 1,
            i % 2 == 0,
            i % 10,
            i % 100,
            8000 + (i % 1000),
            9000 + (i % 1000),
            i % 16,
            8000 + (i % 1000),
        ));
    }
    s
}

fn main() {
    let iters = std::env::var("YAML_BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500usize);
    let large_iters = std::env::var("YAML_LARGE_BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40usize);
    let service_count = std::env::var("SVC_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000usize);

    println!(
        "saneyaml {} vs serde-saphyr (see Cargo.lock for resolved version)\n",
        env!("CARGO_PKG_VERSION"),
    );

    // ---- Load corpus -------------------------------------------------------
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/real-world");
    let mut paths = Vec::new();
    collect_yaml(&root, &mut paths);

    let mut fixtures: Vec<Fixture> = Vec::new();
    for path in &paths {
        let input = std::fs::read_to_string(path).expect("read fixture");
        // Authoritative document count from saneyaml's parser.
        let docs = match saneyaml::parse_documents(&input) {
            Ok(d) => d.len(),
            Err(_) => continue,
        };
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        fixtures.push(Fixture { name, input, docs });
    }
    println!(
        "Loaded {} fixtures ({} bytes) from {}\n",
        fixtures.len(),
        fixtures.iter().map(|f| f.input.len()).sum::<usize>(),
        root.display(),
    );

    // ---- Axis 1: dynamic value (serde_json::Value), single-document --------
    //
    // Only fixtures both libraries accept into serde_json::Value are timed, so
    // the byte totals are identical for the two rows. Exclusions are reported.
    let mut both_ok: Vec<&Fixture> = Vec::new();
    let mut excluded: Vec<(String, String)> = Vec::new();
    for f in fixtures.iter().filter(|f| f.docs == 1) {
        let sane = saneyaml::from_str::<serde_json::Value>(&f.input);
        let saph = serde_saphyr::from_str::<serde_json::Value>(&f.input);
        match (&sane, saph.is_ok()) {
            (Ok(_), true) => both_ok.push(f),
            (Err(e), true) => excluded.push((f.name.clone(), format!("saneyaml rejected: {e}"))),
            (Ok(_), false) => excluded.push((f.name.clone(), "serde-saphyr rejected".into())),
            (Err(e), false) => excluded.push((f.name.clone(), format!("both rejected: {e}"))),
        }
    }
    let dyn_bytes: usize = both_ok.iter().map(|f| f.input.len()).sum();

    println!("## Axis 1 — dynamic value into serde_json::Value (single-doc corpus)\n");
    println!(
        "{} single-doc fixtures accepted by both ({} bytes); {} excluded.\n",
        both_ok.len(),
        dyn_bytes,
        excluded.len()
    );
    if !excluded.is_empty() {
        for (name, why) in &excluded {
            println!("- excluded: {name} ({why})");
        }
        println!();
    }

    table_header();
    let (e, c) = measure(iters, || {
        let mut n = 0u64;
        for f in &both_ok {
            n += saneyaml::from_str::<serde_json::Value>(&f.input)
                .unwrap()
                .is_object() as u64;
        }
        n
    });
    black_box(c);
    row(
        "saneyaml::from_str::<serde_json::Value>",
        iters,
        dyn_bytes,
        e,
    );

    let (e, c) = measure(iters, || {
        let mut n = 0u64;
        for f in &both_ok {
            n += serde_saphyr::from_str::<serde_json::Value>(&f.input)
                .unwrap()
                .is_object() as u64;
        }
        n
    });
    black_box(c);
    row(
        "serde_saphyr::from_str::<serde_json::Value>",
        iters,
        dyn_bytes,
        e,
    );

    // saneyaml native tree, for reference (not a head-to-head row).
    let (e, c) = measure(iters, || {
        let mut n = 0u64;
        for f in &both_ok {
            n += saneyaml::parse_documents(&f.input).unwrap().len() as u64;
        }
        n
    });
    black_box(c);
    row(
        "saneyaml::parse_documents (native, ref)",
        iters,
        dyn_bytes,
        e,
    );

    // ---- Axis 2: typed struct, generated config ----------------------------
    let typed = generate_typed(service_count);
    let typed_bytes = typed.len();

    // Validate both produce the expected shape before timing.
    let sane_cfg: Config = saneyaml::from_str(&typed).expect("saneyaml typed parse");
    let saph_cfg: Config = serde_saphyr::from_str(&typed).expect("serde-saphyr typed parse");
    assert_eq!(sane_cfg.services.len(), service_count);
    assert_eq!(saph_cfg.services.len(), service_count);

    println!(
        "\n## Axis 2 — typed deserialize into Config (generated, {service_count} services, {typed_bytes} bytes; both on defaults)\n"
    );
    table_header();
    let (e, c) = measure(large_iters, || {
        let cfg: Config = saneyaml::from_str(&typed).unwrap();
        cfg.services.len() as u64
    });
    black_box(c);
    row("saneyaml::from_str::<Config>", large_iters, typed_bytes, e);

    let (e, c) = measure(large_iters, || {
        let cfg: Config = serde_saphyr::from_str(&typed).unwrap();
        cfg.services.len() as u64
    });
    black_box(c);
    row(
        "serde_saphyr::from_str::<Config>",
        large_iters,
        typed_bytes,
        e,
    );

    // ---- Axis 3: typed flat records (serde-saphyr's home-turf shape) -------
    let record_count = std::env::var("REC_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15000usize);
    let records = generate_records(record_count);
    let rec_bytes = records.len();
    let sane_recs: Vec<Record> = saneyaml::from_str(&records).expect("saneyaml records");
    let saph_recs: Vec<Record> = serde_saphyr::from_str(&records).expect("serde-saphyr records");
    assert_eq!(sane_recs.len(), record_count);
    assert_eq!(saph_recs.len(), record_count);

    println!(
        "\n## Axis 3 — typed flat Vec<Record> (generated, {record_count} records, {rec_bytes} bytes; both on defaults)\n"
    );
    table_header();
    let (e, c) = measure(large_iters, || {
        let v: Vec<Record> = saneyaml::from_str(&records).unwrap();
        v.len() as u64
    });
    black_box(c);
    row(
        "saneyaml::from_str::<Vec<Record>>",
        large_iters,
        rec_bytes,
        e,
    );

    let (e, c) = measure(large_iters, || {
        let v: Vec<Record> = serde_saphyr::from_str(&records).unwrap();
        v.len() as u64
    });
    black_box(c);
    row(
        "serde_saphyr::from_str::<Vec<Record>>",
        large_iters,
        rec_bytes,
        e,
    );
}
