use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod ast;
mod backend;
mod driver;
mod lexer;
mod linker;
mod lower;
mod obj;
mod parser;
mod sema;
mod ty;

#[derive(Parser, Debug)]
#[command(
    name = "forgec",
    version = "0.1.0",
    about = "Forge compiler (.dev sources)"
)]
struct Args {
    #[arg(short, long, value_name = "TRIPLE")]
    target: Option<String>,

    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    #[arg(long)]
    freestanding: bool,

    #[arg(long, value_name = "PATH")]
    linker: Option<PathBuf>,

    source: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let source = args.source;
    let output = args.output.unwrap_or_else(|| {
        let mut p = source.clone();
        p.set_extension("");
        p
    });
    driver::compile(driver::CompileOptions {
        source,
        output,
        target: args.target,
        freestanding: args.freestanding,
        linker: args.linker,
    })?;
    Ok(())
}
