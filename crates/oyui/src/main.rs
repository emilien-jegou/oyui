use clap::Parser;
use std::error::Error;
use std::process::ExitCode;

pub mod actions;
pub mod app;
pub mod cli;
pub mod commands;
pub mod commons;
pub mod config;
pub mod diff;
pub mod diff_cache;
pub mod syntax;
pub mod tree;
pub mod ui_state;
pub mod view;
pub mod worker;

use crate::cli::Opts;
use crate::commands::CommandError;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let opts = Opts::parse();

    let _trace_guard = commons::tracing::Tracer::builder()
        .flamegraph_enable(opts.common.flamegraph_enable)
        .flamegraph_save_file(opts.common.flamegraph_save_file.clone())
        .log_enable(opts.common.log_enable)
        .log_save_path(opts.common.log_save_path.clone())
        .log_console(opts.common.log_console)
        .build()
        .setup()?;

    tracing::info!("Starting oyui...");

    let result = commands::run(opts).await;

    // Explicitly clear the thread-local registry to prevent TLS drop order
    // issues with scc::HashMap when the main thread terminates.
    // Kept as a security.
    crate::config::clear_registry();

    match result {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(CommandError::NoModifications) => Ok(ExitCode::from(2)),
        Err(CommandError::Aborted) => Ok(ExitCode::from(1)),
        Err(CommandError::Runtime(err)) => {
            eprintln!("Error: {err}");
            Ok(ExitCode::from(1))
        }
    }
}
