use std::io;
use std::process;

use anyhow::Result;
use clap::{Parser, Subcommand};
use env_logger::Env;
use mdbook::preprocess::CmdPreprocessor;

pub use preprocessor::*;
pub use renderer::*;

mod preprocessor;
mod renderer;
mod svg_inline;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check whether a renderer is supported by this preprocessor
    Supports { renderer: String },
}

pub fn run_preprocessor<S: SvgPreprocessor>(preprocessor: &S) {
    env_logger::init_from_env(Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"));

    let cli = Cli::parse();

    match cli.command {
        None => {
            if let Err(e) = handle_preprocessing(preprocessor) {
                eprintln!("{e}");
                process::exit(1);
            }
        }
        Some(Commands::Supports { .. }) => {
            // since we're just outputting markdown images or inline html, this "should" support any renderer
            process::exit(0);
        }
    }
}

fn handle_preprocessing<S: SvgPreprocessor>(pre: &S) -> Result<()> {
    let (ctx, book) = CmdPreprocessor::parse_input(io::stdin())?;

    if ctx.mdbook_version != mdbook::MDBOOK_VERSION {
        // We should probably use the `semver` crate to check compatibility here...
        eprintln!(
            "Warning: The {} plugin was built against version {} of mdbook, \
             but we're being called from version {}",
            pre.name(),
            mdbook::MDBOOK_VERSION,
            ctx.mdbook_version
        );
    }

    let processed_book = pre.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;

    Ok(())
}
