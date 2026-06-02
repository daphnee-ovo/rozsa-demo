mod args;
mod run;

fn main() -> anyhow::Result<()> {
    let _args = args::parse();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async { run::run().await })
}
