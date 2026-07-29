// FrameworkTree
// args.rs
// ├── enum OutputFormat
// ├── struct Args
// ├── parse()
// └── resolve_positional_input()

use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "rozsa", about = "AI coding agent")]
pub struct Args {
    /// Existing project directory, or prompt text when used with --print/-p.
    #[arg(value_name = "DIRECTORY|PROMPT")]
    pub prompt: Option<String>,

    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,

    /// Non-interactive print mode (equivalent to providing a prompt)
    #[arg(short, long)]
    pub print: bool,

    /// Output format for print mode
    #[arg(long, default_value = "text")]
    pub output_format: OutputFormat,

    /// Continue the most recent session
    #[arg(short = 'c', long)]
    pub continue_session: bool,

    /// Resume a specific session by ID
    #[arg(short = 'r', long)]
    pub resume: Option<String>,

    /// Specify provider name
    #[arg(long)]
    pub provider: Option<String>,

    /// Custom system prompt text
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// Set thinking level (off/minimal/low/medium/high/xhigh)
    #[arg(long)]
    pub thinking: Option<String>,
}

pub fn parse() -> Args {
    Args::parse()
}

/// Interpret a bare positional argument as the GUI workspace. Prompt text is
/// accepted only with `--print` so it cannot be confused with a directory.
pub fn resolve_positional_input(
    input: Option<&str>,
    process_cwd: &Path,
    print: bool,
) -> std::io::Result<(PathBuf, Option<String>)> {
    let Some(input) = input else {
        return Ok((process_cwd.to_path_buf(), None));
    };
    if print {
        return Ok((process_cwd.to_path_buf(), Some(input.to_string())));
    }
    let candidate = PathBuf::from(input);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        process_cwd.join(candidate)
    };
    if candidate.is_dir() {
        return Ok((candidate.canonicalize()?, None));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!(
            "'{input}' is not an existing directory. Use `rozsa -p \"{input}\"` to send a prompt."
        ),
    ))
}
