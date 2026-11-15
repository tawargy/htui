use std::io;
use std::process::ExitCode;

use clap::Parser;
use herdr_tui::cli::events::run_events;
use herdr_tui::cli::ls::run_ls;
use herdr_tui::cli::open::validate_space_exists;
use herdr_tui::cli::status::run_status;
use herdr_tui::cli::{Cli, Commands};
use herdr_tui::config::Config;
use herdr_tui::hc::transport::{
    ensure_server_running, is_custom_socket_from_env, resolve_socket_from_env,
};
use herdr_tui::ui::event_loop::run_tui;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = Config::load_default();
    let socket_path = resolve_socket_from_env(cli.socket.as_deref(), config.hc.socket.as_deref());
    let is_custom = is_custom_socket_from_env(cli.socket.as_deref(), config.hc.socket.as_deref());

    match cli.command {
        Some(Commands::Ls {
            json,
            all,
            space,
            label,
        }) => {
            let mut out = io::stdout();
            let mut err = io::stderr();
            let code = run_ls(
                &socket_path,
                json,
                all,
                space.as_deref(),
                &label,
                &mut out,
                &mut err,
            )
            .await;
            ExitCode::from(code as u8)
        }
        Some(Commands::Status { json }) => {
            let mut out = io::stdout();
            let mut err = io::stderr();
            let code = run_status(&socket_path, json, &mut out, &mut err).await;
            ExitCode::from(code as u8)
        }
        Some(Commands::Events { json }) => {
            let mut out = io::stdout();
            let mut err = io::stderr();
            let code = run_events(&socket_path, json, &mut out, &mut err).await;
            ExitCode::from(code as u8)
        }
        Some(Commands::Open { space, create }) => {
            if let Err(e) = ensure_server_running(&socket_path, is_custom).await {
                eprintln!("warning: {e}");
            }
            if let Err(code) = validate_space_exists(&socket_path, &space, create).await {
                eprintln!("error: space '{space}' not found (use --create to create it)");
                return ExitCode::from(code as u8);
            }
            if let Err(e) = run_tui(socket_path, config, Some(space)).await {
                eprintln!("error: {e}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        None => {
            if let Err(e) = ensure_server_running(&socket_path, is_custom).await {
                eprintln!("warning: {e}");
            }
            if let Err(e) = run_tui(socket_path, config, None).await {
                eprintln!("error: {e}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
    }
}
