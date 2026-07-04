use clap::Parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "rozsa", about = "AI coding agent")]
pub struct Args {
    /// Initial prompt (non-interactive mode)
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
