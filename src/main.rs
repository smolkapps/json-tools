//! json-tools — a JSON swiss-army knife CLI.
//!
//! Thin wrapper over the `json_tools` library: parse args, read input from a
//! file or stdin, dispatch to a pure library function, write to `-o` or stdout.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use json_tools::diff::{self};
use json_tools::flatten;
use json_tools::fmt::{self, FmtOptions};
use json_tools::get;
use json_tools::merge::{self, ArrayPolicy};
use json_tools::to_csv;
use json_tools::{parse, read_input};

#[derive(Parser)]
#[command(
    name = "json-tools",
    version,
    about = "A JSON swiss-army knife: fmt, validate, flatten/unflatten, get, keys, diff, merge, to-csv",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pretty-print (default) or compact JSON; optionally sort keys.
    Fmt(FmtArgs),
    /// Exit 0 if input is valid JSON; non-zero with a line/col error if not.
    Validate(SingleInput),
    /// Flatten nested JSON into a flat object with dotted/bracket keys.
    Flatten(SingleInputOut),
    /// Inverse of flatten: rebuild nested JSON from a flat object.
    Unflatten(SingleInputOut),
    /// Extract a value by JSON Pointer (/a/b/0) or dot path (a.b.0).
    Get(GetArgs),
    /// List top-level keys, or keys at --path.
    Keys(KeysArgs),
    /// Structural diff of two JSON files (added/removed/changed paths).
    Diff(DiffArgs),
    /// Deep-merge two or more JSON files (later wins).
    Merge(MergeArgs),
    /// Convert an array of objects to CSV (union of keys, key order preserved).
    ToCsv(ToCsvArgs),
}

/// A single optional input file (stdin when omitted).
#[derive(Args)]
struct SingleInput {
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
}

/// A single optional input file plus an optional output file.
#[derive(Args)]
struct SingleInputOut {
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct ToCsvArgs {
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Prefix cells beginning with `=`, `+`, `-`, or `@` with a single quote so
    /// spreadsheets treat them as text (CSV formula-injection defense). Off by
    /// default.
    #[arg(long = "escape-formulas")]
    escape_formulas: bool,
}

#[derive(Args)]
struct FmtArgs {
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Emit compact single-line JSON instead of pretty-printed.
    #[arg(long)]
    compact: bool,
    /// Sort object keys alphabetically (recursively).
    #[arg(long = "sort-keys")]
    sort_keys: bool,
}

#[derive(Args)]
struct GetArgs {
    /// Path: JSON Pointer (/a/b/0) or dot path (a.b.0).
    path: String,
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Emit the extracted value compactly instead of pretty-printed.
    #[arg(long)]
    compact: bool,
}

#[derive(Args)]
struct KeysArgs {
    /// Input file; reads stdin if omitted.
    file: Option<PathBuf>,
    /// List keys of the object at this path instead of the top level.
    #[arg(long)]
    path: Option<String>,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct DiffArgs {
    /// Left-hand JSON file.
    a: PathBuf,
    /// Right-hand JSON file.
    b: PathBuf,
    /// Emit machine-readable JSON instead of text.
    #[arg(long)]
    json: bool,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct MergeArgs {
    /// Two or more JSON files to deep-merge (later wins).
    #[arg(required = true, num_args = 2..)]
    files: Vec<PathBuf>,
    /// How to combine arrays when both sides are arrays.
    #[arg(long = "array", value_enum, default_value_t = ArrayMode::Replace)]
    array: ArrayMode,
    /// Emit compact single-line JSON instead of pretty-printed.
    #[arg(long)]
    compact: bool,
    /// Write output here instead of stdout.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ArrayMode {
    Concat,
    Replace,
}

impl From<ArrayMode> for ArrayPolicy {
    fn from(m: ArrayMode) -> Self {
        match m {
            ArrayMode::Concat => ArrayPolicy::Concat,
            ArrayMode::Replace => ArrayPolicy::Replace,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Fmt(a) => cmd_fmt(a),
        Command::Validate(a) => cmd_validate(a),
        Command::Flatten(a) => cmd_flatten(a),
        Command::Unflatten(a) => cmd_unflatten(a),
        Command::Get(a) => cmd_get(a),
        Command::Keys(a) => cmd_keys(a),
        Command::Diff(a) => cmd_diff(a),
        Command::Merge(a) => cmd_merge(a),
        Command::ToCsv(a) => cmd_to_csv(a),
    }
}

/// Read + parse from an optional file path (stdin when `None`).
fn load(file: Option<&PathBuf>) -> Result<serde_json::Value> {
    let raw = read_input(file.map(|p| p.as_path()), std::io::stdin())?;
    parse(&raw)
}

/// Write `s` to the output file (with trailing newline) or to stdout.
fn emit(output: Option<&PathBuf>, s: &str) -> Result<()> {
    match output {
        Some(p) => {
            let mut f =
                std::fs::File::create(p).with_context(|| format!("creating {}", p.display()))?;
            f.write_all(s.as_bytes())?;
            f.write_all(b"\n")?;
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(s.as_bytes())?;
            out.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn cmd_fmt(a: FmtArgs) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let s = fmt::format_value(
        &v,
        FmtOptions {
            compact: a.compact,
            sort_keys: a.sort_keys,
        },
    )?;
    emit(a.output.as_ref(), &s)
}

fn cmd_validate(a: SingleInput) -> Result<()> {
    // load() already produces a clear line/col error on bad input, which
    // run()/main() turn into a non-zero exit. On success, confirm to stderr so
    // stdout stays clean for piping.
    let _ = load(a.file.as_ref())?;
    eprintln!("valid JSON");
    Ok(())
}

fn cmd_flatten(a: SingleInputOut) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let flat = flatten::flatten(&v);
    let s = serde_json::to_string_pretty(&flat)?;
    emit(a.output.as_ref(), &s)
}

fn cmd_unflatten(a: SingleInputOut) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let nested = flatten::unflatten(&v)?;
    let s = serde_json::to_string_pretty(&nested)?;
    emit(a.output.as_ref(), &s)
}

fn cmd_get(a: GetArgs) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let found = get::get(&v, &a.path)?;
    let s = if a.compact {
        serde_json::to_string(found)?
    } else {
        serde_json::to_string_pretty(found)?
    };
    emit(a.output.as_ref(), &s)
}

fn cmd_keys(a: KeysArgs) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let ks = get::keys(&v, a.path.as_deref())?;
    emit(a.output.as_ref(), &ks.join("\n"))
}

fn cmd_diff(a: DiffArgs) -> Result<()> {
    let va = load(Some(&a.a))?;
    let vb = load(Some(&a.b))?;
    let changes = diff::diff(&va, &vb);
    let s = if a.json {
        serde_json::to_string_pretty(&diff::render_json(&changes))?
    } else {
        diff::render_text(&changes)
    };
    emit(a.output.as_ref(), &s)
}

fn cmd_to_csv(a: ToCsvArgs) -> Result<()> {
    let v = load(a.file.as_ref())?;
    let s = to_csv::to_csv(
        &v,
        to_csv::CsvOptions {
            escape_formulas: a.escape_formulas,
        },
    )?;
    emit(a.output.as_ref(), &s)
}

fn cmd_merge(a: MergeArgs) -> Result<()> {
    let mut values = Vec::with_capacity(a.files.len());
    for f in &a.files {
        values.push(load(Some(f))?);
    }
    let merged = merge::merge_all(&values, a.array.into());
    let s = if a.compact {
        serde_json::to_string(&merged)?
    } else {
        serde_json::to_string_pretty(&merged)?
    };
    emit(a.output.as_ref(), &s)
}
