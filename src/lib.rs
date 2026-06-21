//! json-tools: a JSON swiss-army knife.
//!
//! Each operation is a pure function over [`serde_json::Value`]. The binary
//! (`src/main.rs`) is a thin CLI wrapper around these functions.
//!
//! `serde_json` is compiled with the `preserve_order` feature so that object
//! key order from the input is retained through every operation that returns
//! JSON.

pub mod diff;
pub mod flatten;
pub mod fmt;
pub mod get;
pub mod merge;

use anyhow::{Context, Result};

/// Parse a JSON string into a [`serde_json::Value`], attaching a clear
/// error message that includes the line/column on failure.
pub fn parse(input: &str) -> Result<serde_json::Value> {
    serde_json::from_str(input).map_err(|e| {
        anyhow::anyhow!(
            "invalid JSON at line {}, column {}: {}",
            e.line(),
            e.column(),
            e
        )
    })
}

/// Read input either from a file path (when `Some`) or from a reader
/// (typically stdin) when `None`.
pub fn read_input(path: Option<&std::path::Path>, mut stdin: impl std::io::Read) -> Result<String> {
    match path {
        Some(p) => {
            std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))
        }
        None => {
            let mut s = String::new();
            stdin
                .read_to_string(&mut s)
                .context("reading JSON from stdin")?;
            Ok(s)
        }
    }
}
