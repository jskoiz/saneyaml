# Untrusted input

YAML from the network, user uploads, or a CI job is hostile until proven
otherwise. saneyaml applies structural resource limits by default, and lets you
tune them per call site.

> Snippets elide the enclosing function; assume a function returning
> `saneyaml::Result<()>`.

## Defaults

Every parser, loader, streaming, lossless, and Serde entry point enforces these
out of the box:

| Limit | Default | Rejects |
|---|---|---|
| Input size | 64 MiB | Oversized payloads, before parsing |
| Nesting depth | 128 | Deeply nested block/flow bombs |
| Scalar size | 1 MiB | Single giant scalars |
| Collection size | 16,384 entries | Wide sequence/mapping bombs |
| Alias expansion | input-derived budget | Billion-laughs alias bombs |
| Recursive aliases | — | always rejected |

The defaults accept real-world config (Kubernetes CRDs, OpenAPI, Compose) while
rejecting compact bombs that sit under the byte ceiling.

## Tighten for a specific call

Lower a limit when you know your inputs are small:

```rust
use saneyaml::LoadOptions;

let cfg: Config = LoadOptions::new()
    .max_input_bytes(256 * 1024)        // cap at 256 KiB
    .max_nesting_depth(32)
    .max_collection_items(1_000)
    .from_str(text)?;
```

All knobs: `max_input_bytes`, `max_alias_expansion_nodes`, `max_nesting_depth`,
`max_scalar_bytes`, `max_collection_items`.

## Relax — only when you've bounded the source yourself

Each `without_*` opt-out transfers that part of the bound to you. Use them only
when the source is already trusted or size-checked upstream:

```rust
let node = saneyaml::LoadOptions::new()
    .without_input_limit()   // also: without_nesting_depth_limit,
    .parse_str(local_file)?;  //       without_scalar_limit, without_collection_limit
```

## What the limits are — and aren't

These are **structural construction limits**, not wall-clock or
resident-memory guarantees:

- Reader entry points fully buffer bounded input before parsing.
- Raw event/lossless streams validate alias references but don't expand them, so
  they don't spend the alias budget.
- Your own `Deserialize` impls can still allocate after the YAML layer hands them
  bounded values.
- saneyaml validates YAML structure, not application schemas (Kubernetes,
  OpenAPI, …).

For the full threat model and reporting process, see
[COMPATIBILITY.md → Threat model](COMPATIBILITY.md#threat-model-and-resource-guarantees)
and [SECURITY.md](../SECURITY.md).
