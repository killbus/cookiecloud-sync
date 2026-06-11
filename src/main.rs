mod client;
mod cookiefile;
mod decrypt;

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tracing::{error, info};

const DEFAULT_OUTPUT: &str = "/output/cookies.txt";
const DEFAULT_INTERVAL: u64 = 300;
const DEFAULT_FORMAT: &str = "netscape";

#[derive(Parser)]
#[command(
    name = "cookiecloud-sync",
    about = "Periodically sync cookies from CookieCloud"
)]
struct Cli {
    #[arg(short = 's', long = "server", env = "COOKIECLOUD_URL")]
    server_url: String,

    #[arg(short = 'u', long = "uuid", env = "COOKIECLOUD_UUID")]
    uuid: String,

    #[arg(short = 'p', long = "password", env = "COOKIECLOUD_PASSWORD")]
    password: String,

    #[arg(short = 'o', long = "output", env = "OUTPUT_FILE", default_value = DEFAULT_OUTPUT)]
    output_file: PathBuf,

    #[arg(short = 'f', long = "format", env = "OUTPUT_FORMAT", default_value = DEFAULT_FORMAT)]
    output_format: String,

    #[arg(short = 'i', long = "interval", env = "INTERVAL_SECS", default_value_t = DEFAULT_INTERVAL)]
    interval_secs: u64,

    #[arg(long = "once", help = "Run once and exit")]
    once: bool,
}

#[derive(Debug)]
struct Config {
    server_url: String,
    uuid: String,
    password: String,
    output_file: PathBuf,
    output_format: String,
    interval: Duration,
    once: bool,
}

impl Config {
    fn from_cli(cli: &Cli) -> Result<Self, String> {
        match cli.output_format.as_str() {
            "netscape" | "json" | "both" => {}
            other => {
                return Err(format!(
                    "invalid output format: {other} (use netscape, json, or both)"
                ))
            }
        }
        Ok(Config {
            server_url: cli.server_url.clone(),
            uuid: cli.uuid.clone(),
            password: cli.password.clone(),
            output_file: cli.output_file.clone(),
            output_format: cli.output_format.clone(),
            interval: Duration::from_secs(cli.interval_secs),
            once: cli.once,
        })
    }
}

fn write_cookies(data: &decrypt::DecryptedData, config: &Config) -> Result<(), String> {
    let fmt = config.output_format.as_str();
    if fmt == "netscape" || fmt == "both" {
        cookiefile::write_netscape(data, &config.output_file)
            .map_err(|e| format!("failed to write Netscape file: {e}"))?;
        info!(path = %config.output_file.display(), "wrote Netscape cookie file");
    }
    if fmt == "json" || fmt == "both" {
        let json_path = if fmt == "both" {
            let mut p = config.output_file.clone();
            let stem = p
                .file_stem()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("cookies");
            p.set_file_name(format!("{stem}.json"));
            p
        } else {
            config.output_file.clone()
        };
        cookiefile::write_json(data, &json_path)
            .map_err(|e| format!("failed to write JSON file: {e}"))?;
        info!(path = %json_path.display(), "wrote JSON cookie file");
    }
    Ok(())
}

async fn sync_once(http_client: &reqwest::Client, config: &Config) -> Result<(), String> {
    let encrypted = client::fetch_encrypted(http_client, &config.server_url, &config.uuid)
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;

    let crypto_type = encrypted.crypto_type.as_deref();
    let data = decrypt::decrypt(
        &config.uuid,
        &encrypted.encrypted,
        &config.password,
        crypto_type,
    )
    .map_err(|e| format!("decrypt failed: {e}"))?;

    let cookie_count: usize = data.cookie_data.values().map(|v| v.len()).sum();
    info!(
        domains = data.cookie_data.len(),
        cookies = cookie_count,
        "decrypted successfully"
    );

    write_cookies(&data, config)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = match Config::from_cli(&cli) {
        Ok(c) => c,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    info!(
        server_url = %config.server_url,
        output = %config.output_file.display(),
        format = %config.output_format,
        interval_secs = %config.interval.as_secs(),
        once = config.once,
        "starting cookiecloud-sync"
    );

    let http_client = reqwest::Client::builder()
        .user_agent("cookiecloud-sync/1.0")
        .build()
        .expect("failed to create HTTP client");

    loop {
        if let Err(e) = sync_once(&http_client, &config).await {
            error!("sync failed: {e}");
            if config.once {
                std::process::exit(1);
            }
            error!("retrying in {}s", config.interval.as_secs());
        } else if config.once {
            return;
        }
        tokio::time::sleep(config.interval).await;
    }
}
