use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "rozsa", about = "AI coding agent")]
pub struct Args {
    /// Initial prompt (non-interactive mode)
    pub prompt: Option<String>,

    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,
}

pub fn parse() -> Args {
    Args::parse()
}
