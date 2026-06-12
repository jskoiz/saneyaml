//! `syq` — a minimal `yq`-style YAML editor demonstrating saneyaml's
//! comment-preserving lossless edits.
//!
//! Unlike load-and-re-emit tools, edits rewrite only the addressed value:
//! comments, anchors, key ordering, quoting, and untouched bytes all survive.
//!
//! ```sh
//! cargo run --example syq -- get    /server/port  config.yaml
//! cargo run --example syq -- set    /server/port 9090 config.yaml
//! cargo run --example syq -- push   /server/hosts web-3 config.yaml
//! cargo run --example syq -- rename /server old-server config.yaml
//! cargo run --example syq -- rm     /server/debug config.yaml
//! ```
//!
//! Paths are RFC 6901 JSON Pointers (`~1` escapes `/`, `~0` escapes `~`).
//! Values are parsed as YAML, so `9090`, `true`, `[a, b]`, and `{k: v}` all
//! work. Reads from stdin when no file is given; edits print the rewritten
//! document to stdout, or rewrite the file in place with `-i`.

use std::io::Read as _;
use std::process::ExitCode;

struct Cli {
    command: String,
    path: String,
    value: Option<String>,
    file: Option<String>,
    in_place: bool,
}

const USAGE: &str = "\
usage: syq <command> <pointer> [value] [file] [-i]

commands:
  get    <pointer>            print the value at the path as YAML
  set    <pointer> <value>    replace the value at the path
  push   <pointer> <value>    append to the sequence at the path
  rename <pointer> <new-key>  rename the mapping key at the path
  rm     <pointer>            remove the mapping entry or sequence item

The pointer is an RFC 6901 JSON Pointer, e.g. /services/web/image or
/jobs/test/steps/0/uses. Values are parsed as YAML. Input comes from the
file argument or stdin. Edits print to stdout unless -i rewrites the file.";

fn parse_args() -> Result<Cli, String> {
    let mut positional = Vec::new();
    let mut in_place = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-i" | "--in-place" => in_place = true,
            "-h" | "--help" => return Err(String::new()),
            _ => positional.push(arg),
        }
    }
    let mut positional = positional.into_iter();
    let command = positional.next().ok_or("missing command")?;
    let path = positional.next().ok_or("missing pointer path")?;
    let takes_value = matches!(command.as_str(), "set" | "push" | "rename");
    let value = if takes_value {
        Some(positional.next().ok_or("missing value argument")?)
    } else {
        None
    };
    let file = positional.next();
    if let Some(extra) = positional.next() {
        return Err(format!("unexpected argument {extra:?}"));
    }
    if in_place && file.is_none() {
        return Err("-i requires a file argument".to_string());
    }
    Ok(Cli {
        command,
        path,
        value,
        file,
        in_place,
    })
}

fn read_input(file: Option<&str>) -> std::io::Result<String> {
    match file {
        Some(path) => std::fs::read_to_string(path),
        None => {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            Ok(buffer)
        }
    }
}

/// Decodes RFC 6901 tokens for the read-only `get` traversal; edits delegate
/// decoding to `ConfigPath::json_pointer`.
fn pointer_tokens(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let rest = pointer
        .strip_prefix('/')
        .ok_or("pointer must be empty or start with '/'")?;
    rest.split('/')
        .map(|token| {
            let mut decoded = String::with_capacity(token.len());
            let mut chars = token.chars();
            while let Some(c) = chars.next() {
                if c != '~' {
                    decoded.push(c);
                    continue;
                }
                match chars.next() {
                    Some('0') => decoded.push('~'),
                    Some('1') => decoded.push('/'),
                    _ => return Err(format!("invalid escape in token {token:?}")),
                }
            }
            Ok(decoded)
        })
        .collect()
}

fn lookup<'a>(mut node: &'a saneyaml::Value, tokens: &[String]) -> Option<&'a saneyaml::Value> {
    for token in tokens {
        node = match node {
            saneyaml::Value::Mapping(_) => node.get(token.as_str())?,
            saneyaml::Value::Sequence(_) => node.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(node)
}

fn run(cli: &Cli) -> Result<(), String> {
    let input = read_input(cli.file.as_deref()).map_err(|e| format!("read input: {e}"))?;

    if cli.command == "get" {
        let tokens = pointer_tokens(&cli.path)?;
        let root: saneyaml::Value =
            saneyaml::from_str(&input).map_err(|e| format!("parse input: {e}"))?;
        let found = lookup(&root, &tokens).ok_or_else(|| format!("no value at {:?}", cli.path))?;
        let rendered = saneyaml::to_string(found).map_err(|e| format!("render value: {e}"))?;
        print!("{rendered}");
        return Ok(());
    }

    let path = saneyaml::ConfigPath::json_pointer(&cli.path).map_err(|e| e.to_string())?;
    let mut editor = saneyaml::edit(input).map_err(|e| format!("parse input: {e}"))?;
    match cli.command.as_str() {
        "set" | "push" => {
            let raw = cli.value.as_deref().expect("value argument was parsed");
            let value: saneyaml::Value =
                saneyaml::from_str(raw).map_err(|e| format!("parse value as YAML: {e}"))?;
            if cli.command == "set" {
                editor.set(path, value).map_err(|e| e.to_string())?;
            } else {
                editor.push(path, value).map_err(|e| e.to_string())?;
            }
        }
        "rename" => {
            let new_key = cli.value.as_deref().expect("value argument was parsed");
            editor.rename(path, new_key).map_err(|e| e.to_string())?;
        }
        "rm" => {
            editor.remove(path).map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unknown command {other:?}")),
    }

    let output = editor.finish().map_err(|e| e.to_string())?;
    if cli.in_place {
        let file = cli.file.as_deref().expect("-i requires a file");
        std::fs::write(file, output).map_err(|e| format!("write {file}: {e}"))?;
    } else {
        print!("{output}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(message) => {
            if message.is_empty() {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
