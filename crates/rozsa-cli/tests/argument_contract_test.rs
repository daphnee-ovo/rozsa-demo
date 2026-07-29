#[allow(dead_code)]
#[path = "../src/args.rs"]
mod args;

use clap::Parser;

#[test]
fn print_mode_accepts_a_positional_prompt() {
    let parsed = args::Args::try_parse_from(["rozsa", "--print", "summarize the changes"])
        .expect("--print with a prompt should parse");

    assert!(parsed.print);
    assert_eq!(parsed.prompt.as_deref(), Some("summarize the changes"));
}

#[test]
fn retired_tui_flag_is_rejected() {
    let error = args::Args::try_parse_from(["rozsa", "--tui"])
        .expect_err("the retired TUI flag must not be accepted");

    assert!(error.to_string().contains("--tui"));
}
