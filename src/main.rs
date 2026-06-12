use clap::Parser;
use cookiecloud_sync::{build_config, sync_once, DEFAULT_FORMAT, DEFAULT_INTERVAL};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};

const ENV_SERVER_URL: &str = "COOKIECLOUD_URL";
const ENV_UUID: &str = "COOKIECLOUD_UUID";
const ENV_PASSWORD: &str = "COOKIECLOUD_PASSWORD";
const ENV_OUTPUT_FORMAT: &str = "OUTPUT_FORMAT";
const ENV_INTERVAL: &str = "INTERVAL";
const ENV_EXPORT_SPECS: &str = "EXPORT_SPECS";
const ENV_CRYPTO_TYPE: &str = "COOKIECLOUD_CRYPTO_TYPE";
const ENV_TIMEOUT: &str = "TIMEOUT";
const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Parser)]
#[command(
    name = "cookiecloud-sync",
    version = "1.0.0",
    about = "Periodically sync cookies from CookieCloud"
)]
struct Cli {
    #[arg(short = 's', long = "server", env = ENV_SERVER_URL)]
    server_url: String,

    #[arg(short = 'u', long = "uuid", env = ENV_UUID)]
    uuid: String,

    #[arg(short = 'p', long = "password", env = ENV_PASSWORD, conflicts_with = "password_file")]
    password: Option<String>,

    #[arg(
        long = "password-file",
        conflicts_with = "password",
        help = "Read password from file (first line, trimmed)"
    )]
    password_file: Option<PathBuf>,

    #[arg(
        short = 'f',
        long = "format",
        env = ENV_OUTPUT_FORMAT,
        default_value = DEFAULT_FORMAT,
        help = "Default output format: netscape, json, or both"
    )]
    output_format: String,

    #[arg(
        short = 'i',
        long = "interval",
        env = ENV_INTERVAL,
        default_value_t = DEFAULT_INTERVAL
    )]
    interval_secs: u64,

    #[arg(long = "once", help = "Run once and exit")]
    once: bool,

    #[arg(
        long = "dry-run",
        help = "Decrypt and log what would be written, without writing files"
    )]
    dry_run: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        help = "Enable debug-level logging",
        conflicts_with = "quiet"
    )]
    verbose: bool,

    #[arg(
        short = 'q',
        long = "quiet",
        help = "Suppress output except errors",
        conflicts_with = "verbose"
    )]
    quiet: bool,

    #[arg(
        long = "export",
        help = "Export spec: <domains>:<file>[:<format>]\n\
                domains: comma-separated list, empty for all\n\
                file: output path\n\
                format: netscape, json, or both (optional)\n\
                Can be repeated. Combined with env EXPORT_SPECS\n\
                (same format, semicolon-separated); CLI overrides env on path conflict."
    )]
    exports: Vec<String>,

    #[arg(
        short = 'c',
        long = "crypto-type",
        env = ENV_CRYPTO_TYPE,
        help = "Encryption algorithm: legacy or aes-128-cbc-fixed (default: auto)"
    )]
    crypto_type: Option<String>,

    #[arg(
        long = "timeout",
        env = ENV_TIMEOUT,
        default_value_t = DEFAULT_TIMEOUT_SECS,
        help = "HTTP request timeout in seconds"
    )]
    timeout_secs: u64,

    #[arg(
        long = "output-dir",
        env = "OUTPUT_DIR",
        help = "Base directory for relative export paths"
    )]
    output_dir: Option<PathBuf>,
}

fn backoff_delay(base: Duration, attempt: u32) -> Duration {
    use rand::Rng;
    let max_secs = base.as_secs_f64();
    let secs = max_secs.min(1.0 * 2f64.powi(attempt as i32 - 1));
    let jitter = 0.75 + rand::thread_rng().gen::<f64>() * 0.5;
    Duration::from_secs_f64(secs * jitter)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let log_level = if cli.quiet {
        "error"
    } else if cli.verbose {
        "debug"
    } else {
        "info"
    };

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    if cli.dry_run {
        warn!("dry-run mode: no files will be written");
    }

    let password = match (cli.password, cli.password_file) {
        (Some(p), _) => p,
        (None, Some(path)) => match std::fs::read_to_string(&path) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                error!("failed to read password file '{}': {e}", path.display());
                std::process::exit(1);
            }
        },
        (None, None) => {
            error!("password required; use -p/--password, --password-file, or COOKIECLOUD_PASSWORD env");
            std::process::exit(1);
        }
    };

    let export_env = std::env::var(ENV_EXPORT_SPECS)
        .ok()
        .filter(|v| !v.is_empty());

    let config = match build_config(
        cli.server_url,
        cli.uuid,
        password,
        cli.output_format,
        cli.interval_secs,
        cli.once,
        cli.dry_run,
        cli.exports,
        export_env,
        cli.crypto_type,
        cli.output_dir,
    ) {
        Ok(c) => c,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    info!(
        server_url = %config.server_url,
        exports = ?config.exports.iter().map(|e| e.path.display().to_string()).collect::<Vec<_>>(),
        global_format = ?config.global_format,
        timeout_secs = cli.timeout_secs,
        interval_secs = %config.interval.as_secs(),
        once = config.once,
        dry_run = config.dry_run,
        "starting cookiecloud-sync"
    );

    let http_client = match reqwest::Client::builder()
        .user_agent("cookiecloud-sync/1.0")
        .timeout(Duration::from_secs(cli.timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            error!("failed to build HTTP client: {e}");
            std::process::exit(1);
        }
    };

    let mut failures: u32 = 0;

    loop {
        match sync_once(&http_client, &config).await {
            Ok(()) => {
                failures = 0;
                if config.once {
                    return;
                }
            }
            Err(errors) => {
                failures += 1;
                for e in &errors {
                    error!("{e}");
                }
                if config.once {
                    std::process::exit(1);
                }
                let delay = backoff_delay(config.interval, failures);
                error!(
                    failures = failures,
                    delay_secs = %delay.as_secs_f64(),
                    "sync failed, will retry"
                );
                tokio::time::sleep(delay).await;
                continue;
            }
        }
        tokio::time::sleep(config.interval).await;
    }
}
