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
pub mod script;
pub mod syntax;
pub mod terminal_colors;
pub mod theme;
pub mod tree;
pub mod ui_state;
pub mod vcs;
pub mod view;
pub mod worker;

use crate::cli::Args;
use crate::commands::{CommandError, RunOptions};

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = Args::parse();

    let color_mode = terminal_colors::detect_color_mode()?;

    let _trace_guard = commons::tracing::Tracer::builder()
        // Nested invocations (jj spawning oyui as its diff tool) inherit the
        // environment but not our CLI flags, so the file log also honors
        // OYUI_LOG_ENABLE/OYUI_LOG_PATH: the same shell command then traces
        // the real TUI process, not the delegator.
        .log_enable(
            args.common.log_enable || std::env::var_os("OYUI_LOG_ENABLE").is_some_and(|v| v == "1"),
        )
        .log_save_path(
            args.common
                .log_save_path
                .clone()
                .or_else(|| std::env::var_os("OYUI_LOG_PATH").map(std::path::PathBuf::from)),
        )
        .log_console(args.common.log_console)
        .build()
        .setup()?;

    // The frame log: per-iteration and per-flush telemetry, armed here where
    // the log path is already resolved (env fallback so nested children
    // inherit it — the jj-spawned TUI otherwise has no logger of its own).
    if let Some(path) = args
        .common
        .log_save_path
        .clone()
        .or_else(|| std::env::var_os("OYUI_LOG_PATH").map(std::path::PathBuf::from))
    {
        if args.common.log_enable
            || std::env::var_os("OYUI_LOG_ENABLE").is_some_and(|v| v == "1")
            || std::env::var_os("OYUI_LOG_PATH").is_some()
        {
            crate::commons::frame_log::arm(path);
        }
    }

    tracing::info!("Starting oyui...");

    let result = commands::run(RunOptions { args, color_mode }).await;

    match result {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(CommandError::NoModifications) => Ok(ExitCode::from(2)),
        Err(CommandError::Aborted) => Ok(ExitCode::from(1)),
        Err(CommandError::ChildExit(code)) => Ok(ExitCode::from(code.clamp(0, 255) as u8)),
        Err(CommandError::Runtime(err)) => {
            eprintln!("Error: {err}");
            Ok(ExitCode::from(1))
        }
    }
}
