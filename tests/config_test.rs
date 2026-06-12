use cookiecloud_sync::{
    build_config, merge_exports, parse_export_env, Config, ConfigError, ExportSpec, OutputFormat,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

fn make_config(global_format: OutputFormat, once: bool) -> Config {
    Config {
        server_url: String::new(),
        uuid: String::new(),
        password: String::new(),
        exports: vec![],
        global_format,
        crypto_type: None,
        interval: Duration::from_secs(300),
        once,
        dry_run: false,
    }
}

#[test]
fn resolve_format_no_override() {
    let spec = ExportSpec {
        domains: None,
        path: PathBuf::from("out.txt"),
        format_override: None,
    };
    let config = make_config(OutputFormat::Json, true);
    assert_eq!(config.resolve_format(&spec), OutputFormat::Json);
}

#[test]
fn resolve_format_with_override() {
    let spec = ExportSpec {
        domains: None,
        path: PathBuf::from("out.txt"),
        format_override: Some(OutputFormat::Netscape),
    };
    let config = make_config(OutputFormat::Json, true);
    assert_eq!(config.resolve_format(&spec), OutputFormat::Netscape);
}

#[test]
fn build_config_no_exports_is_error() {
    let result = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec![],
        None,
        None,
        None,
    );
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ConfigError::NoExports));
}

#[test]
fn build_config_with_exports() {
    let config = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "json".into(),
        300,
        true,
        false,
        vec!["youtube.com:yt.txt".into(), ":all.json".into()],
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.exports.len(), 2);
    assert_eq!(config.global_format, OutputFormat::Json);

    assert_eq!(config.exports[0].path, PathBuf::from("yt.txt"));
    assert!(config.exports[0]
        .domains
        .as_ref()
        .unwrap()
        .contains("youtube.com"));

    assert_eq!(config.exports[1].path, PathBuf::from("all.json"));
    assert!(config.exports[1].domains.is_none());
}

#[test]
fn build_config_duplicate_path_rejected() {
    let result = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec!["youtube.com:out.txt".into(), "github.com:out.txt".into()],
        None,
        None,
        None,
    );
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ConfigError::DuplicateExport(_)
    ));
}

#[test]
fn build_config_env_only() {
    let config = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec![],
        Some("youtube.com:yt.txt;:all.json:json".into()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.exports.len(), 2);
    let paths: HashSet<PathBuf> = config.exports.iter().map(|e| e.path.clone()).collect();
    assert!(paths.contains(&PathBuf::from("all.json")));
    assert!(paths.contains(&PathBuf::from("yt.txt")));
}

#[test]
fn build_config_cli_overrides_env() {
    let config = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec![":custom.json:json".into()],
        Some("youtube.com:custom.json:netscape;github.com:gh.txt".into()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.exports.len(), 2);
    for spec in &config.exports {
        if spec.path == PathBuf::from("custom.json") {
            assert_eq!(spec.format_override, Some(OutputFormat::Json));
        }
        if spec.path == PathBuf::from("gh.txt") {
            assert!(spec.domains.as_ref().unwrap().contains("github.com"));
        }
    }
}

#[test]
fn build_config_cli_plus_env_overlapping_paths() {
    let config = build_config(
        "http://localhost".into(),
        "test-uuid".into(),
        "test-password".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec!["youtube.com:yt.txt:json".into()],
        Some("youtube.com:yt.txt:netscape".into()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.exports.len(), 1);
    assert_eq!(
        config.resolve_format(&config.exports[0]),
        OutputFormat::Json
    );
}

#[test]
fn parse_env_empty() {
    let result = parse_export_env("");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ConfigError::EmptyEnvExport));
}

#[test]
fn parse_env_duplicate_rejected() {
    let result = parse_export_env("a:1.txt;a:1.txt");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ConfigError::DuplicateEnvExport(_)
    ));
}

#[test]
fn merge_no_conflict() {
    let env_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["github.com".into()])),
        path: PathBuf::from("gh.txt"),
        format_override: None,
    }];
    let cli_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["youtube.com".into()])),
        path: PathBuf::from("yt.txt"),
        format_override: None,
    }];
    let merged = merge_exports(env_specs, cli_specs);
    assert_eq!(merged.len(), 2);
}

#[test]
fn merge_cli_overrides_env() {
    let env_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["github.com".into()])),
        path: PathBuf::from("out.txt"),
        format_override: Some(OutputFormat::Netscape),
    }];
    let cli_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["youtube.com".into()])),
        path: PathBuf::from("out.txt"),
        format_override: Some(OutputFormat::Json),
    }];
    let merged = merge_exports(env_specs, cli_specs);
    assert_eq!(merged.len(), 1);
    let domains = merged[0].domains.as_ref().unwrap();
    assert!(domains.contains("youtube.com"));
    assert!(!domains.contains("github.com"));
    assert_eq!(merged[0].format_override, Some(OutputFormat::Json));
}

#[test]
fn merge_keeps_both_if_no_conflict() {
    let env_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["github.com".into()])),
        path: PathBuf::from("gh.txt"),
        format_override: None,
    }];
    let cli_specs = vec![ExportSpec {
        domains: Some(HashSet::from(["youtube.com".into()])),
        path: PathBuf::from("yt.txt"),
        format_override: None,
    }];
    let merged = merge_exports(env_specs, cli_specs);
    assert_eq!(merged.len(), 2);
}

#[test]
fn build_config_output_dir_resolves_relative() {
    let config = build_config(
        "http://localhost".into(),
        "uuid".into(),
        "pwd".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec!["youtube.com:cookies.txt".into()],
        None,
        None,
        Some(PathBuf::from("/output")),
    )
    .unwrap();
    assert_eq!(config.exports[0].path, PathBuf::from("/output/cookies.txt"));
}

#[test]
fn build_config_output_dir_ignores_absolute() {
    let config = build_config(
        "http://localhost".into(),
        "uuid".into(),
        "pwd".into(),
        "netscape".into(),
        300,
        true,
        false,
        vec!["youtube.com:/custom/path.txt".into()],
        None,
        None,
        Some(PathBuf::from("/output")),
    )
    .unwrap();
    assert_eq!(config.exports[0].path, PathBuf::from("/custom/path.txt"));
}
