# Schema modes

A schema decides how a plain scalar like `NO`, `on`, or `0123` becomes a typed
value. saneyaml defaults to **YAML 1.2**, where those stay strings — so you don't
get the "Norway problem."

## The Norway problem

In YAML 1.1 (and the archived `serde_yaml`), `NO` resolves to the boolean
`false`. A country-code list `[NO, SE, FI]` silently becomes `[false, SE, FI]`.

YAML 1.2 fixed this: only `true` / `false` are booleans. saneyaml follows 1.2 by
default.

```rust
let codes: Vec<String> = saneyaml::from_str("[NO, SE, FI]")?;
assert_eq!(codes, ["NO", "SE", "FI"]); // strings, as written
```

## Default resolution (YAML 1.2)

| Plain scalar | Resolves to |
|---|---|
| `null`, `Null`, `NULL`, `~`, missing value | null |
| `true`, `false`, `True`, `FALSE` | bool |
| `yes`, `no`, `on`, `off`, `y`, `n`, `NO` | **string** |
| `123`, `+12`, `0123`, `1_000` | integer (decimal — `0123` is **123**) |
| `0x7B`, `0b1010`, `0o77` | string |
| `1.5`, `.inf`, `-.Inf`, `.NAN` | float |
| `1:20`, `1:20:30` | string |
| `2026-05-24`, datetimes | string |

The full table for every mode is in
[COMPATIBILITY.md → Scalar resolution](COMPATIBILITY.md#scalar-resolution-modes).

## Choosing a mode

Pass a configured `LoadOptions` instead of calling `from_str` directly:

```rust
use saneyaml::LoadOptions;

let cfg: Config = LoadOptions::core().from_str(text)?;          // YAML 1.2 (the default)
let cfg: Config = LoadOptions::json().from_str(text)?;          // JSON booleans/null/numbers only
let cfg: Config = LoadOptions::failsafe().from_str(text)?;      // every scalar stays a string
let cfg: Config = LoadOptions::legacy_serde_yaml().from_str(text)?; // YAML 1.1 / serde_yaml-style
```

| Mode | Use it when |
|---|---|
| `core()` *(default)* | You want correct YAML 1.2 behavior. |
| `json()` | Input is JSON-ish and you want strict JSON scalar typing. |
| `failsafe()` | You want raw strings and will type things yourself. |
| `legacy_serde_yaml()` / `yaml_1_1()` | You have an existing corpus that *depends* on `no` → `false`, octal `0123`, sexagesimals, `!!timestamp` typing, etc. |

`Schema::{Core, Json, Failsafe, LegacySerdeYaml}` are the enum equivalents;
`Schema::Yaml12` and `Schema::Yaml11` are retained aliases for `Core` and
`LegacySerdeYaml`.

## Per-document `%YAML` directives

To let each document's `%YAML` header pick the mode — `%YAML 1.1` gets legacy
construction, everything else stays 1.2 — use directive mode:

```rust
let docs = saneyaml::LoadOptions::yaml_version_directive().from_documents_str(stream)?;
```

## What YAML 1.1 mode turns on

`legacy_serde_yaml()` / `yaml_1_1()` resolves the legacy forms: boolean words
(`yes`/`no`/`on`/`off`), octal `0123`, `0x`/`0b` radix integers, base-60
sexagesimals, and `!!timestamp`-shaped scalars (read via `saneyaml::Timestamp`).
Numeric spellings that overflow `Number` stay strings.

Even in YAML 1.2 mode you can still opt into individual YAML 1.1 *types* with
explicit tags (`!!int 0o77`, `!!binary …`, `!!timestamp …`) without switching the
whole schema. See [COMPATIBILITY.md](COMPATIBILITY.md) for the exact tag rules.
