#[macro_use]
extern crate lazy_static;

use std::io;
use std::process;

use clap::{Parser, Subcommand};
use mdbook::errors::Error;
use mdbook::preprocess::{CmdPreprocessor, Preprocessor};

use crate::preprocessor::GraphvizPreprocessor;

mod preprocessor;
mod renderer;

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

fn main() {
    let cli = Cli::parse();

    let preprocessor = GraphvizPreprocessor;

    match cli.command {
        None => {
            if let Err(e) = handle_preprocessing(&preprocessor) {
                eprintln!("{e}");
                process::exit(1);
            }
        }
        Some(Commands::Supports { renderer }) => {
            // Signal whether the renderer is supported by exiting with 1 or 0.
            if preprocessor.supports_renderer(&renderer) {
                process::exit(0);
            } else {
                process::exit(1);
            }
        }
    }
}

fn handle_preprocessing(pre: &dyn Preprocessor) -> Result<(), Error> {
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
