use cookiecloud_sync::{parse_export_spec, OutputFormat};
use std::path::PathBuf;

#[test]
fn single_domain() {
    let spec = parse_export_spec("youtube.com:yt.txt").unwrap();
    let domains = spec.domains.unwrap();
    assert!(domains.contains("youtube.com"));
    assert_eq!(spec.path, PathBuf::from("yt.txt"));
    assert!(spec.format_override.is_none());
}

#[test]
fn multi_domain() {
    let spec = parse_export_spec("youtube.com,github.com:both.txt").unwrap();
    let domains = spec.domains.unwrap();
    assert!(domains.contains("youtube.com"));
    assert!(domains.contains("github.com"));
    assert_eq!(spec.path, PathBuf::from("both.txt"));
}

#[test]
fn all_domains() {
    let spec = parse_export_spec(":cookies.txt").unwrap();
    assert!(spec.domains.is_none());
    assert_eq!(spec.path, PathBuf::from("cookies.txt"));
}

#[test]
fn with_format_override() {
    let spec = parse_export_spec("youtube.com:yt.txt:netscape").unwrap();
    let domains = spec.domains.unwrap();
    assert!(domains.contains("youtube.com"));
    assert_eq!(spec.path, PathBuf::from("yt.txt"));
    assert_eq!(spec.format_override, Some(OutputFormat::Netscape));
}

#[test]
fn all_with_format() {
    let spec = parse_export_spec(":all.json:json").unwrap();
    assert!(spec.domains.is_none());
    assert_eq!(spec.path, PathBuf::from("all.json"));
    assert_eq!(spec.format_override, Some(OutputFormat::Json));
}

#[test]
fn case_normalized() {
    let spec = parse_export_spec("YouTube.com:yt.txt").unwrap();
    let domains = spec.domains.unwrap();
    assert!(domains.contains("youtube.com"));
}

#[test]
fn missing_colon() {
    let result = parse_export_spec("youtube.com");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("missing ':' separator"));
}

#[test]
fn empty_path() {
    let result = parse_export_spec("youtube.com:");
    assert!(result.is_err());
}

#[test]
fn format_keyword_in_filename() {
    let spec = parse_export_spec("youtube.com:report.both").unwrap();
    let domains = spec.domains.unwrap();
    assert!(domains.contains("youtube.com"));
    assert_eq!(spec.path, PathBuf::from("report.both"));
    assert!(spec.format_override.is_none());
}

#[test]
fn colon_in_filename_is_error() {
    let result = parse_export_spec("youtube.com:foo:bar.txt");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown format"));
}

#[test]
fn unknown_format_error() {
    let result = parse_export_spec("youtube.com:yt.txt:garbage");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown format"));
}
