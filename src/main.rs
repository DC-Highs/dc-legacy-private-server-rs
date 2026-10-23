//! Command line: `dragon-city <serve|migrate|create-admin|seed>`.
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use dragon_city::app::{build_router, spawn_background_tasks, startup_tasks};
use dragon_city::auth::service;
use dragon_city::config::Settings;
use dragon_city::db::{mongo, postgres};
use dragon_city::gamedata::GameData;
use dragon_city::state::AppState;
use dragon_city::storage::Storage;
use tracing_subscriber::EnvFilter;

const DB_WAIT_SECONDS: u64 = 60;

#[derive(Parser)]
#[command(name = "dragon-city", version, about = "Dragon City Legacy Server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Serve,
    Migrate,
    Seed,
    Healthcheck,
    CreateAdmin {
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
    },
}

fn init_logging(level: &str) {
    let level = level.trim().to_lowercase();
    let level = if level == "warning" { "warn".to_string() } else { level };
    let filter = EnvFilter::try_new(&level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

async fn connect(settings: Settings) -> Result<AppState, String> {
    let game = GameData::load(&settings.seed_dir)?;
    let pool = postgres::connect(&settings.database, DB_WAIT_SECONDS).await.map_err(|e| e.to_string())?;
    postgres::migrate(&pool).await.map_err(|e| e.to_string())?;
    let players = mongo::connect(&settings, DB_WAIT_SECONDS).await.map_err(|e| e.to_string())?;
    let storage = Storage::new(Arc::new(players), game).await.map_err(|e| e.to_string())?;
    Ok(AppState::new(settings, Arc::new(postgres::PgAccounts::new(pool)), storage))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("Shutting down");
}

async fn healthcheck(bind_addr: &str) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let port = bind_addr.rsplit(':').next().unwrap_or("8080");
    let address = format!("127.0.0.1:{port}");
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect(&address).await.map_err(|e| e.to_string())?;
        let request = format!("GET /api/game/health HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await.map_err(|e| e.to_string())?;
        Ok::<String, String>(response)
    };
    let response = tokio::time::timeout(std::time::Duration::from_secs(4), probe)
        .await
        .map_err(|_| "the health check timed out".to_string())??;
    if response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        Err(format!("unhealthy: {}", response.lines().next().unwrap_or("no response")))
    }
}

async fn run(cli: Cli, settings: Settings) -> Result<(), String> {
    match cli.command.unwrap_or(Command::Serve) {
        Command::Healthcheck => healthcheck(&settings.bind_addr).await?,
        Command::Migrate => {
            let pool = postgres::connect(&settings.database, DB_WAIT_SECONDS).await.map_err(|e| e.to_string())?;
            postgres::migrate(&pool).await.map_err(|e| e.to_string())?;
            tracing::info!("Database schema is up to date");
        }
        Command::Seed => {
            let state = connect(settings).await?;
            state.storage.store().ensure_indexes().await.map_err(|e| e.to_string())?;
            dragon_city::game::pvp::seed_pvp_data(&state).await.map_err(|e| e.message)?;
        }
        Command::CreateAdmin { email, password } => {
            let state = connect(settings).await?;
            state.storage.store().ensure_indexes().await.map_err(|e| e.to_string())?;
            let user = service::ensure_admin(&state, &email, &password).await.map_err(|e| e.message)?;
            if let Some(user) = user {
                tracing::info!("Administrator account ready: {} (id {})", user.email, user.id);
            }
        }
        Command::Serve => {
            let bind: SocketAddr = settings.bind_addr.parse().map_err(|e| format!("invalid BIND_ADDR: {e}"))?;
            let state = connect(settings).await?;
            startup_tasks(&state).await.map_err(|e| e.message)?;
            spawn_background_tasks(state.clone());
            tracing::info!(
                "Packet protocol: {} commands implemented",
                dragon_city::commands::implemented_count()
            );

            let listener = tokio::net::TcpListener::bind(bind).await.map_err(|e| format!("cannot listen on {bind}: {e}"))?;
            tracing::info!("Listening on http://{bind}");
            axum::serve(listener, build_router(state).into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(shutdown_signal())
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let settings = Settings::from_process_env();
    init_logging(&settings.log_level);

    match run(cli, settings).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}
