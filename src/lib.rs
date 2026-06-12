pub(crate) mod client;
pub(crate) mod cookiefile;
pub mod decrypt;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tracing::{debug, info, warn};

pub const DEFAULT_INTERVAL: u64 = 300;
pub const DEFAULT_FORMAT: &str = "netscape";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Netscape,
    Json,
    Both,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        match s {
            "netscape" => Ok(OutputFormat::Netscape),
            "json" => Ok(OutputFormat::Json),
            "both" => Ok(OutputFormat::Both),
            other => Err(ConfigError::InvalidFormat(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid format '{0}'; use netscape, json, or both")]
    InvalidFormat(String),
    #[error("{0}")]
    ExportSpec(String),
    #[error("duplicate output path in --export: {0}")]
    DuplicateExport(PathBuf),
    #[error("duplicate output path in EXPORT_SPECS: {0}")]
    DuplicateEnvExport(PathBuf),
    #[error("EXPORT_SPECS is empty or contains no valid specs")]
    EmptyEnvExport,
    #[error("no export specs provided; use --export or set EXPORT_SPECS")]
    NoExports,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("fetch failed: {0}")]
    Fetch(#[from] client::ClientError),
    #[error("decrypt failed: {0}")]
    Decrypt(#[from] decrypt::DecryptError),
    #[error("{path}: {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: cookiefile::WriteError,
    },
}

#[derive(Debug)]
pub struct ExportSpec {
    pub domains: Option<HashSet<String>>,
    pub path: PathBuf,
    pub format_override: Option<OutputFormat>,
}

pub fn parse_export_spec(input: &str) -> Result<ExportSpec, ConfigError> {
    let parts: Vec<&str> = input.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Err(ConfigError::ExportSpec(
            "invalid --export value, missing ':' separator".into(),
        ));
    }

    let domains_str = parts[0];
    let file_str = parts[1];

    let format_override = if parts.len() == 3 {
        let fmt_candidate = parts[2];
        match fmt_candidate {
            "netscape" | "json" | "both" => Some(OutputFormat::parse(fmt_candidate)?),
            _ => {
                return Err(ConfigError::ExportSpec(format!(
                    "unknown format '{fmt_candidate}' in --export; use netscape, json, or both",
                )));
            }
        }
    } else {
        None
    };

    let domains = if domains_str.is_empty() {
        None
    } else {
        let set: HashSet<String> = domains_str
            .split(',')
            .map(|d| d.trim().to_lowercase())
            .filter(|d| !d.is_empty())
            .collect();
        if set.is_empty() {
            return Err(ConfigError::ExportSpec(
                "empty domain list in --export".into(),
            ));
        }
        Some(set)
    };

    if file_str.is_empty() {
        return Err(ConfigError::ExportSpec(
            "empty output path in --export".into(),
        ));
    }

    Ok(ExportSpec {
        domains,
        path: PathBuf::from(file_str),
        format_override,
    })
}

pub fn parse_export_env(raw: &str) -> Result<Vec<ExportSpec>, ConfigError> {
    let parts: Vec<&str> = raw
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(ConfigError::EmptyEnvExport);
    }
    let mut seen = HashSet::new();
    parts
        .iter()
        .map(|s| {
            let spec = parse_export_spec(s)?;
            if !seen.insert(spec.path.clone()) {
                return Err(ConfigError::DuplicateEnvExport(spec.path));
            }
            Ok(spec)
        })
        .collect()
}

pub fn merge_exports(lower: Vec<ExportSpec>, higher: Vec<ExportSpec>) -> Vec<ExportSpec> {
    let mut map: HashMap<PathBuf, ExportSpec> = HashMap::new();
    for spec in lower {
        map.insert(spec.path.clone(), spec);
    }
    for spec in higher {
        if map.contains_key(&spec.path) {
            warn!(
                path = %spec.path.display(),
                source = "CLI",
                overridden_source = "env",
                "export path conflict: CLI overrides env"
            );
        }
        map.insert(spec.path.clone(), spec);
    }
    let mut result: Vec<ExportSpec> = map.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

fn resolve_export_paths(mut exports: Vec<ExportSpec>, base: Option<PathBuf>) -> Vec<ExportSpec> {
    let Some(base) = base else { return exports };
    for spec in &mut exports {
        if spec.path.is_relative() {
            spec.path = base.join(&spec.path);
        }
    }
    exports
}

#[derive(Debug)]
pub struct Config {
    pub server_url: String,
    pub uuid: String,
    pub password: String,
    pub exports: Vec<ExportSpec>,
    pub global_format: OutputFormat,
    pub crypto_type: Option<String>,
    pub interval: Duration,
    pub once: bool,
    pub dry_run: bool,
}

impl Config {
    pub fn resolve_format(&self, spec: &ExportSpec) -> OutputFormat {
        spec.format_override.unwrap_or(self.global_format)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_config(
    server_url: String,
    uuid: String,
    password: String,
    output_format_str: String,
    interval_secs: u64,
    once: bool,
    dry_run: bool,
    export_cli: Vec<String>,
    export_env: Option<String>,
    crypto_type: Option<String>,
    output_dir: Option<PathBuf>,
) -> Result<Config, ConfigError> {
    let global_format = OutputFormat::parse(&output_format_str)?;

    let cli_specs = if !export_cli.is_empty() {
        let mut seen = HashSet::new();
        Some(
            export_cli
                .iter()
                .map(|s| {
                    let spec = parse_export_spec(s)?;
                    if !seen.insert(spec.path.clone()) {
                        return Err(ConfigError::DuplicateExport(spec.path));
                    }
                    Ok(spec)
                })
                .collect::<Result<Vec<_>, ConfigError>>()?,
        )
    } else {
        None
    };

    let env_specs = match &export_env {
        Some(val) if !val.is_empty() => {
            debug!(raw_export_env = %val, "parsing EXPORT_SPECS");
            Some(parse_export_env(val)?)
        }
        _ => None,
    };

    let exports = match (cli_specs, env_specs) {
        (Some(cli), Some(env)) => {
            info!(
                cli_count = cli.len(),
                env_count = env.len(),
                "merging export specs: CLI overrides env on path conflict"
            );
            merge_exports(env, cli)
        }
        (Some(cli), None) => cli,
        (None, Some(env)) => env,
        (None, None) => return Err(ConfigError::NoExports),
    };

    let crypto_type = crypto_type.filter(|v| !v.eq_ignore_ascii_case("auto"));

    let exports = resolve_export_paths(exports, output_dir);

    Ok(Config {
        server_url,
        uuid,
        password,
        exports,
        global_format,
        crypto_type,
        interval: Duration::from_secs(interval_secs),
        once,
        dry_run,
    })
}

fn write_cookies(
    data: &decrypt::DecryptedData,
    path: &Path,
    fmt: OutputFormat,
) -> Result<(), cookiefile::WriteError> {
    match fmt {
        OutputFormat::Netscape => cookiefile::write_netscape(data, path),
        OutputFormat::Json => cookiefile::write_json(data, path),
        OutputFormat::Both => {
            cookiefile::write_netscape(data, path)?;
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("cookies");
            let mut json_path = path.to_path_buf();
            json_path.set_file_name(format!("{stem}.json"));
            cookiefile::write_json(data, &json_path)
        }
    }
}

pub async fn sync_once(
    http_client: &reqwest::Client,
    config: &Config,
) -> Result<(), Vec<SyncError>> {
    let encrypted = client::fetch_encrypted(http_client, &config.server_url, &config.uuid)
        .await
        .map_err(|e| vec![SyncError::Fetch(e)])?;

    let crypto_type = config
        .crypto_type
        .as_deref()
        .or(encrypted.crypto_type.as_deref());
    info!(
        crypto_type = crypto_type.unwrap_or("(not specified)"),
        server_crypto_type = ?encrypted.crypto_type,
        "decrypting"
    );
    let data = decrypt::decrypt(
        &config.uuid,
        &encrypted.encrypted,
        &config.password,
        crypto_type,
    )
    .map_err(|e| vec![SyncError::Decrypt(e)])?;

    let cookie_count: usize = data.cookie_data.values().map(|v| v.len()).sum();
    info!(
        domains = data.cookie_data.len(),
        cookies = cookie_count,
        exports = config.exports.len(),
        "decrypted successfully"
    );

    let errors: Vec<SyncError> = config
        .exports
        .iter()
        .filter_map(|spec| {
            let filtered = spec
                .domains
                .as_ref()
                .map(|ds| cookiefile::filter_domains(&data, ds));
            let write_data = filtered.as_ref().map(|d| d as &_).unwrap_or(&data);

            let fmt = config.resolve_format(spec);
            if config.dry_run {
                info!(
                    path = %spec.path.display(),
                    ?fmt,
                    export_domains = ?spec.domains,
                    "dry-run: would write cookie file"
                );
                None
            } else if let Err(e) = write_cookies(write_data, &spec.path, fmt) {
                Some(SyncError::WriteFailed {
                    path: spec.path.clone(),
                    source: e,
                })
            } else {
                info!(path = %spec.path.display(), ?fmt, "wrote cookie file");
                None
            }
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
