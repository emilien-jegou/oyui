use clap::Parser;
use std::error::Error;

pub mod app;
pub mod cli;
pub mod commands;
pub mod commons;
pub mod config;
pub mod diff;
pub mod diff_cache;
pub mod actions;
pub mod syntax;
pub mod tree;
pub mod ui_state;
pub mod view;
pub mod worker;

use crate::cli::Opts;
use crate::commands::CommandError;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opts = Opts::parse();

    let trace_guard = commons::tracing::Tracer::builder()
        .flamegraph_enable(opts.common.flamegraph_enable)
        .flamegraph_save_file(opts.common.flamegraph_save_file.clone())
        .log_enable(opts.common.log_enable)
        .log_save_path(opts.common.log_save_path.clone())
        .log_console(opts.common.log_console)
        .build()
        .setup()?;

    tracing::info!("Starting oyui...");

    let result = commands::run(opts).await;

    drop(trace_guard);

    match result {
        Ok(()) => Ok(()),
        Err(CommandError::NoModifications) => {
            std::process::exit(2);
        }
        Err(CommandError::Aborted) => {
            std::process::exit(1);
        }
        Err(CommandError::Runtime(err)) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}
