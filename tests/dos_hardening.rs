use saneyaml::{Error, Event, LoadOptions, Node, Value};
use serde::Deserialize;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, Instant};

const DOS_MANIFEST: &str = include_str!("fixtures/dos/manifest.toml");
const CLEAN_SMALL: &str = include_str!("fixtures/dos/clean/small.yaml");
const REAL_WORLD_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");
const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const ALIAS_BOMB: &str = include_str!("fixtures/dos/adversarial/alias-expansion-bomb.yaml");
const DEEP_FLOW_NESTING: &str = include_str!("fixtures/dos/adversarial/deep-flow-nesting.yaml");
const OVERSIZED_PLAIN_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-plain-scalar.yaml");
const OVERSIZED_BLOCK_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-block-scalar.yaml");
const OVERSIZED_DOUBLE_QUOTED_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-double-quoted-scalar.yaml");
const WIDE_FLOW_SEQUENCE: &str = include_str!("fixtures/dos/adversarial/wide-flow-sequence.yaml");
const WIDE_FLOW_MAPPING: &str = include_str!("fixtures/dos/adversarial/wide-flow-mapping.yaml");
const MULTILINE_UNTERMINATED_FLOW: &str =
    include_str!("fixtures/dos/adversarial/multiline-unterminated-flow.yaml");
const ISSUE_13_MULTIBYTE_BLOCK_SCALAR_WHITESPACE_PAYLOADS: &[(&str, &[u8])] = &[
    (
        "document-start folded scalar with U+0085 blank line",
        b"\x2d\x2d\x2d\x20\x3e\x31\x0a\xc2\x85\x0a\x2e",
    ),
    (
        "mapping folded scalar with U+00A0 blank line",
        b"\x62\x2e\x6c\x3a\x20\x3e\x0a\x20\x6c\x72\x0a\x0a\x0a\x0a\x0a\x0a\x0a\x0a\x0a\xc2\xa0\x0a\x20\xef\xbb\xbf\x7b\x7d\x20\x62\x0a\x64\x0a\x00\x00\x0a\x53",
    ),
];

#[derive(Debug, Deserialize)]
struct RealWorldManifest {
    fixture: Vec<RealWorldFixture>,
}

#[derive(Debug, Deserialize)]
struct RealWorldFixture {
    path: String,
}

#[test]
fn adversarial_manifest_documents_goal_06_cases() {
    for required in [
        "deep-flow-nesting",
        "oversized-plain-scalar",
        "oversized-block-scalar",
        "oversized-double-quoted-scalar",
        "wide-flow-sequence",
        "wide-flow-mapping",
        "alias-expansion-bomb",
        "multiline-unterminated-flow",
        "nesting-depth",
        "scalar-growth",
        "collection-growth",
        "alias-expansion",
    ] {
        assert!(
            DOS_MANIFEST.contains(required),
            "DoS manifest must record {required}"
        );
    }
}

#[test]
fn default_limits_accept_all_real_world_fixtures() {
    let manifest: RealWorldManifest =
        toml::from_str(REAL_WORLD_SOURCE).expect("real-world manifest parses");
    assert_eq!(manifest.fixture.len(), 33);
    let root = Path::new(REAL_WORLD_ROOT);
    let mut documents = 0usize;
    for fixture in manifest.fixture {
        let input = fs::read_to_string(root.join(&fixture.path))
            .unwrap_or_else(|error| panic!("read real-world fixture {}: {error}", fixture.path));
        let parsed = LoadOptions::new()
            .parse_documents(&input)
            .unwrap_or_else(|error| panic!("default limits parse {}: {error}", fixture.path));
        documents += parsed.len();
        LoadOptions::new()
            .stream_events(&input)
            .unwrap_or_else(|error| {
                panic!("default limits stream events {}: {error}", fixture.path)
            })
            .collect::<saneyaml::Result<Vec<Event>>>()
            .unwrap_or_else(|error| {
                panic!("default event stream accepts {}: {error}", fixture.path)
            });
        LoadOptions::new()
            .from_documents_str::<Value>(&input)
            .unwrap_or_else(|error| panic!("default Serde accepts {}: {error}", fixture.path));
    }
    assert_eq!(documents, 39);
}

#[test]
fn clean_inputs_load_with_default_and_tight_limits() {
    for options in [
        LoadOptions::new(),
        LoadOptions::new()
            .max_nesting_depth(8)
            .max_scalar_bytes(16)
            .max_collection_items(4),
    ] {
        options.parse_str(CLEAN_SMALL).expect("parse clean fixture");
        options
            .parse_borrowed_documents(CLEAN_SMALL)
            .expect("borrowed clean fixture");
        options
            .stream_events(CLEAN_SMALL)
            .expect("event stream")
            .collect::<saneyaml::Result<Vec<_>>>()
            .expect("events clean fixture");
        options
            .stream_documents(CLEAN_SMALL)
            .expect("document stream")
            .collect::<saneyaml::Result<Vec<_>>>()
            .expect("documents clean fixture");
        options
            .from_str::<Value>(CLEAN_SMALL)
            .expect("Serde clean fixture");
        saneyaml::parse_lossless_with_options(CLEAN_SMALL, options)
            .expect("lossless clean fixture");
    }
}

#[test]
fn nesting_limit_rejects_every_load_shape_with_spans() {
    let input = DEEP_FLOW_NESTING;
    let options = LoadOptions::new().max_nesting_depth(4);
    assert_all_expanding_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_event_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_lossless_rejects(input, options, "maximum YAML nesting depth exceeded");
}

#[test]
fn scalar_limit_rejects_plain_and_block_scalars_with_spans() {
    let options = LoadOptions::new().max_scalar_bytes(8);
    for input in [
        OVERSIZED_PLAIN_SCALAR,
        OVERSIZED_BLOCK_SCALAR,
        OVERSIZED_DOUBLE_QUOTED_SCALAR,
    ] {
        assert_all_expanding_entrypoints_reject(
            input,
            options,
            "YAML scalar exceeds configured limit",
        );
        assert_event_entrypoints_reject(input, options, "YAML scalar exceeds configured limit");
        assert_lossless_rejects(input, options, "YAML scalar exceeds configured limit");
    }
}

#[test]
fn collection_limit_rejects_wide_sequences_and_mappings_with_spans() {
    let options = LoadOptions::new().max_collection_items(3);
    for input in [WIDE_FLOW_SEQUENCE, WIDE_FLOW_MAPPING] {
        assert_all_expanding_entrypoints_reject(
            input,
            options,
            "YAML collection exceeds configured limit",
        );
        assert_event_entrypoints_reject(input, options, "YAML collection exceeds configured limit");
        assert_lossless_rejects(input, options, "YAML collection exceeds configured limit");
    }
}

#[test]
fn alias_bomb_rejects_semantic_loaders_but_raw_events_do_not_expand() {
    let options = LoadOptions::new().max_alias_expansion_nodes(8);

    assert_all_expanding_entrypoints_reject(ALIAS_BOMB, options, "alias expansion limit exceeded");

    let events = options
        .stream_events(ALIAS_BOMB)
        .expect("raw event stream")
        .collect::<saneyaml::Result<Vec<Event>>>()
        .expect("raw events validate aliases without expansion");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Alias { anchor } if anchor.name == "e"))
    );
    saneyaml::parse_lossless_with_options(ALIAS_BOMB, options)
        .expect("lossless graph validates aliases without semantic expansion");
}

#[test]
fn issue_13_multibyte_block_scalar_whitespace_returns_errors_without_panicking() {
    for (_name, input) in ISSUE_13_MULTIBYTE_BLOCK_SCALAR_WHITESPACE_PAYLOADS {
        let input = *input;
        let text = std::str::from_utf8(input).expect("issue #13 payload is valid UTF-8");
        for error in [
            saneyaml::parse_bytes(input).expect_err("parse_bytes rejects issue #13 payload"),
            saneyaml::from_slice::<Value>(input).expect_err("from_slice rejects issue #13 payload"),
            LoadOptions::new()
                .parse_bytes(input)
                .expect_err("LoadOptions::parse_bytes rejects issue #13 payload"),
            LoadOptions::new()
                .from_slice::<Value>(input)
                .expect_err("LoadOptions::from_slice rejects issue #13 payload"),
        ] {
            assert_limit_error(text, &error, "UTF-8 character boundary");
        }
    }
}

#[test]
fn default_limits_reject_committed_adversarial_fixtures() {
    for (input, expected) in [
        (ALIAS_BOMB, "alias expansion limit exceeded"),
        (DEEP_FLOW_NESTING, "maximum YAML nesting depth exceeded"),
        (
            OVERSIZED_PLAIN_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            OVERSIZED_BLOCK_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            OVERSIZED_DOUBLE_QUOTED_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            WIDE_FLOW_SEQUENCE,
            "YAML collection exceeds configured limit",
        ),
        (
            WIDE_FLOW_MAPPING,
            "YAML collection exceeds configured limit",
        ),
        (
            MULTILINE_UNTERMINATED_FLOW,
            "maximum YAML nesting depth exceeded",
        ),
    ] {
        assert_all_expanding_entrypoints_reject(input, LoadOptions::new(), expected);
    }
}

/// A flow collection that opens one bracket per line and never closes used to
/// re-scan the entire accumulated buffer from byte 0 on every appended line,
/// making close detection O(N^2) in the input size. Close detection is now
/// incremental, so the committed 120 KB fixture is rejected by the
/// nesting-depth limit in linear time rather than seconds.
#[test]
fn multiline_unterminated_flow_is_rejected_quickly() {
    assert!(
        MULTILINE_UNTERMINATED_FLOW.len() >= 120_000,
        "regression fixture must be large enough to expose quadratic scanning, got {} bytes",
        MULTILINE_UNTERMINATED_FLOW.len()
    );

    let error = LoadOptions::new()
        .parse_str(MULTILINE_UNTERMINATED_FLOW)
        .expect_err("unterminated multi-line flow collection is rejected");
    assert_limit_error(
        MULTILINE_UNTERMINATED_FLOW,
        &error,
        "maximum YAML nesting depth exceeded",
    );
}

/// Prove the close-detection fix is sub-quadratic by measuring how parse time
/// scales with input size. Doubling the number of opened-but-never-closed flow
/// brackets roughly doubles the work with the incremental scanner, whereas the
/// old full-buffer rescan quadrupled it. Comparing the ratio (rather than an
/// absolute wall-clock bound) keeps the test robust across debug/release builds
/// and machines while still failing loudly if quadratic behavior returns.
#[test]
fn multiline_unterminated_flow_scales_subquadratically() {
    fn unterminated_flow(open_brackets: usize) -> String {
        "[\n".repeat(open_brackets)
    }

    // Reject quickly enough that the parser never gathers the whole input: with
    // the default depth limit the parser stops after ~128 levels regardless of
    // how many lines follow, so timing is dominated by the close-detection scan.
    fn time_reject(open_brackets: usize) -> Duration {
        let input = unterminated_flow(open_brackets);
        // Best-of-several runs to damp scheduler noise on the small absolute times.
        let mut best = Duration::from_secs(3600);
        for _ in 0..5 {
            let started = Instant::now();
            let result = LoadOptions::new().parse_str(&input);
            let elapsed = started.elapsed();
            assert!(
                result.is_err(),
                "unterminated flow with {open_brackets} brackets must be rejected"
            );
            best = best.min(elapsed);
        }
        best
    }

    let small = time_reject(50_000);
    let large = time_reject(100_000);

    // Linear scanning gives a ~2x ratio; the old O(N^2) scan gave ~4x. Allow
    // generous headroom (3x) for timer granularity and allocator noise while
    // still catching a regression back to quadratic behavior.
    let small_nanos = small.as_nanos().max(1);
    let large_nanos = large.as_nanos();
    assert!(
        large_nanos <= small_nanos.saturating_mul(3),
        "doubling an unterminated flow collection should scale sub-quadratically: \
         50k brackets took {small:?}, 100k brackets took {large:?} ({}x)",
        large_nanos as f64 / small_nanos as f64
    );
}

/// A single line consisting only of quote characters (no newlines) used to make
/// mapping-colon detection quadratic: every candidate quote re-scanned the line
/// from the end while deciding whether it could open a quoted scalar, so a line
/// of N quotes did O(N^2) work even though the input never reaches the
/// multi-line quoted-scalar collector. The per-line scan is now linear, so a
/// large single-line run of quotes parses-or-rejects in bounded time rather than
/// the tens of seconds the old scan required.
#[test]
fn single_line_quote_run_is_handled_quickly() {
    for quote in ['"', '\''] {
        let input = quote.to_string().repeat(500_000);
        let started = Instant::now();
        // The result (parsed scalar vs. error) is intentionally unconstrained;
        // the guarantee is only that the parser terminates promptly without the
        // old quadratic blow-up.
        let _ = saneyaml::parse_str(&input);
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "single-line run of {} {quote:?} characters must parse-or-reject quickly, took {elapsed:?}",
            input.len()
        );
    }
}

/// Prove the single-line quote-scan fix is sub-quadratic by measuring how parse
/// time scales with line length. Doubling the number of quote characters on one
/// line roughly doubles the work with the linear scanner, whereas the old
/// per-position rescan quadrupled it. Comparing the ratio (rather than an
/// absolute wall-clock bound) keeps the test robust across debug/release builds
/// and machines while still failing loudly if quadratic behavior returns.
#[test]
fn single_line_quote_run_scales_subquadratically() {
    fn time_parse(quote_count: usize) -> Duration {
        let input = "\"".repeat(quote_count);
        // Best-of-several runs to damp scheduler noise on the small absolute times.
        let mut best = Duration::from_secs(3600);
        for _ in 0..5 {
            let started = Instant::now();
            let _ = saneyaml::parse_str(&input);
            best = best.min(started.elapsed());
        }
        best
    }

    let small = time_parse(200_000);
    let large = time_parse(400_000);

    // Linear scanning gives a ~2x ratio; the old O(N^2) scan gave ~4x. Allow
    // generous headroom (3x) for timer granularity and allocator noise while
    // still catching a regression back to quadratic behavior.
    let small_nanos = small.as_nanos().max(1);
    let large_nanos = large.as_nanos();
    assert!(
        large_nanos <= small_nanos.saturating_mul(3),
        "doubling a single-line quote run should scale sub-quadratically: \
         200k quotes took {small:?}, 400k quotes took {large:?} ({}x)",
        large_nanos as f64 / small_nanos as f64
    );
}

#[test]
fn nesting_depth_opt_out_alias_expansion_uses_iterative_accounting() {
    let depth = 160usize;
    let mut input = String::from("root: &root ");
    for _ in 0..depth {
        input.push('[');
    }
    input.push('0');
    for _ in 0..depth {
        input.push(']');
    }
    input.push_str("\nalias: *root\n");

    let value: Value = LoadOptions::new()
        .without_nesting_depth_limit()
        .from_str(&input)
        .expect("depth opt-out expands deep alias without recursive accounting");
    assert!(value["alias"].is_sequence());
}

fn assert_all_expanding_entrypoints_reject(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        options.parse_str(input).expect_err("parse_str rejects"),
        options
            .parse_bytes(input.as_bytes())
            .expect_err("parse_bytes rejects"),
        options
            .parse_documents(input)
            .expect_err("parse_documents rejects"),
        options
            .parse_borrowed_documents(input)
            .expect_err("parse_borrowed_documents rejects"),
        options
            .from_str::<Value>(input)
            .expect_err("from_str rejects"),
        options
            .from_slice::<Value>(input.as_bytes())
            .expect_err("from_slice rejects"),
        options
            .from_reader::<_, Value>(Cursor::new(input.as_bytes()))
            .expect_err("from_reader rejects"),
        options
            .from_documents_str::<Value>(input)
            .expect_err("from_documents_str rejects"),
        options
            .from_documents_slice::<Value>(input.as_bytes())
            .expect_err("from_documents_slice rejects"),
        options
            .from_documents_reader::<Value, _>(Cursor::new(input.as_bytes()))
            .expect_err("from_documents_reader rejects"),
        options
            .stream_documents(input)
            .expect("document stream constructs")
            .collect::<saneyaml::Result<Vec<Node>>>()
            .expect_err("DocumentStream rejects"),
        options
            .deserializer_from_str(input)
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_str rejects"),
        options
            .deserializer_from_slice(input.as_bytes())
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_slice rejects"),
        options
            .deserializer_from_reader(Cursor::new(input.as_bytes()))
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_reader rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_event_entrypoints_reject(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        options
            .stream_events(input)
            .expect("event stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_str rejects"),
        options
            .stream_events_slice(input.as_bytes())
            .expect("event slice stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_slice rejects"),
        options
            .stream_events_reader(Cursor::new(input.as_bytes()))
            .expect("event reader stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_reader rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_lossless_rejects(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        saneyaml::parse_lossless_with_options(input, options).expect_err("lossless str rejects"),
        saneyaml::parse_lossless_bytes_with_options(input.as_bytes(), options)
            .expect_err("lossless bytes rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_limit_error(input: &str, error: &Error, expected: &str) {
    let display = error.to_string();
    assert!(
        display.contains(expected),
        "expected {expected:?} in {display:?}"
    );
    let diagnostic = error.diagnostic();
    assert!(
        !diagnostic.message.is_empty(),
        "limit errors keep a diagnostic message"
    );
    let span = diagnostic.span;
    assert!(span.line >= 1, "limit error keeps a line span: {span:?}");
    assert!(
        span.column >= 1,
        "limit error keeps a column span: {span:?}"
    );
    assert!(
        span.start <= span.end && span.end <= input.len(),
        "limit error span stays inside input: {span:?}, len={}",
        input.len()
    );
}
