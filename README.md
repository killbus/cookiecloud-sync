# cookiecloud-sync

Periodically fetches encrypted cookies from a [CookieCloud](https://github.com/easychen/CookieCloud) server, decrypts them, and writes to local files.

## Usage

All options accept CLI flags (local dev) or environment variables (Docker). CLI flags take precedence.

```bash
# with CLI flags (one-shot)
cargo run -- -s http://cookiecloud:8088 -u your-uuid -p your-password --export all:./cookies.txt --once

# with CLI flags (daemon)
cargo run -- \
  -s http://cookiecloud:8088 \
  -u your-uuid \
  -p your-password \
  --export all:./cookies.txt \
  -i 60

# domain filter: leading dots are ignored, so youtube.com matches .youtube.com
cargo run -- \
  -s http://cookiecloud:8088 \
  -u your-uuid \
  -p your-password \
  --export "youtube.com,github.com:./cookies.txt:netscape" \
  --once

# with env vars (relative paths resolve to cwd)
export COOKIECLOUD_URL=http://cookiecloud:8088
export COOKIECLOUD_UUID=your-uuid
export COOKIECLOUD_PASSWORD=your-password
export EXPORT_SPECS="youtube.com:cookies.txt"
cargo run

# with env vars + output-dir (relative paths resolve under OUTPUT_DIR)
export COOKIECLOUD_URL=http://cookiecloud:8088
export COOKIECLOUD_UUID=your-uuid
export COOKIECLOUD_PASSWORD=your-password
export EXPORT_SPECS="youtube.com:cookies.txt"
export OUTPUT_DIR=/tmp
cargo run
```

## Docker

```bash
docker run -d \
  -e COOKIECLOUD_URL=http://cookiecloud:8088 \
  -e COOKIECLOUD_UUID=your-uuid \
  -e COOKIECLOUD_PASSWORD=your-password \
  -e EXPORT_SPECS="youtube.com,github.com:cookies.txt" \
  -e OUTPUT_DIR=/output \
  -v /path/to/output:/output \
  ghcr.io/killbus/cookiecloud-sync:latest
```

## Configuration

| Flag | Short | Env | Default | Description |
|---|---|---|---|---|
| `--server` | `-s` | `COOKIECLOUD_URL` | *(required)* | CookieCloud server URL |
| `--uuid` | `-u` | `COOKIECLOUD_UUID` | *(required)* | User UUID |
| `--password` | `-p` | `COOKIECLOUD_PASSWORD` | *(required)* | Encryption password |
| `--password-file` | — | — | — | Read password from file (first line, trimmed) |
| `--export` | — | — | *(required)* | Export spec: `[domains:]<path>[:format]` (repeatable). See below. |
| | | `EXPORT_SPECS` | *(required)* | Same as `--export`, semicolon-separated |
| `--format` | `-f` | `OUTPUT_FORMAT` | `netscape` | Default output format: `netscape`, `json`, `both` |
| `--crypto-type` | `-c` | `COOKIECLOUD_CRYPTO_TYPE` | `auto` | Encryption: `legacy`, `aes-128-cbc-fixed`, or `auto` |
| `--interval` | `-i` | `INTERVAL` | `300` | Sync interval in seconds |
| `--timeout` | — | `TIMEOUT` | `30` | HTTP request timeout in seconds |
| `--once` | — | — | `false` | Run once and exit |
| `--dry-run` | — | — | `false` | Decrypt and log without writing files |
| `--verbose` | `-v` | — | — | Enable debug logging |
| `--quiet` | `-q` | — | — | Suppress output except errors |
| `--output-dir` | — | `OUTPUT_DIR` | — | Base directory for relative export paths |
| | | `RUST_LOG` | `info` | Logging filter |

### Export Spec Format

```
[domains:]<path>[:format]

domains  comma-separated list of domains; empty/`all` for all domains
path     output file path
format   netscape, json, or both (optional, overrides --format)
```

Leading dots in domain names are normalized during matching — `youtube.com` and `.youtube.com` are treated as equivalent.

Examples:
```
# all domains, netscape format (default from --format)
all:/output/cookies.txt

# single domain, with format override
youtube.com:/output/youtube.txt:json

# multiple domains
github.com,gitlab.com:/output/dev.txt

# per-domain files with per-format
github.com:/output/gh_cookies.txt:netscape;gitlab.com:/output/gl_cookies.json:json
```

Multiple `--export` flags and `EXPORT_SPECS` env are merged; CLI overrides env on path conflict.

### Output Directory

If `--output-dir` / `OUTPUT_DIR` is set, relative paths in export specs are resolved against it. Absolute paths are unaffected. Without it, relative paths resolve to the current working directory.

Useful for Docker: set once via env, then use short relative paths in export specs.

```bash
docker run -d \
  -e EXPORT_SPECS="youtube.com:cookies.txt;github.com:cookies.txt" \
  -e OUTPUT_DIR=/output \
  -v /path/to/output:/output \
  ...
```

### Crypto Types

- **`legacy`** — OpenSSL-compatible EVP_BytesToKey with random salt (used by CookieCloud docker server)
- **`aes-128-cbc-fixed`** — AES-128-CBC with zero IV (used by full CookieCloud server)
- **`auto`** (default) — try server-reported `crypto_type`, fall back to `legacy`

Priority: CLI flag > env var > server response > `legacy`

## Output Formats

- **netscape** (default): Netscape HTTP cookie file format (`curl`/`wget` compatible)
- **json**: CookieCloud's native JSON structure (`cookie_data`, `local_storage_data`, `update_time`)
- **both**: Both formats simultaneously (writes `.json` alongside the primary file)

## Domain Matching

Domain filters ignore leading dots. Both the requested domain and the stored cookie domain are stripped of a leading `.` before comparison:

| User specifies | Matches data key | Reason |
|---|---|---|
| `youtube.com` | `youtube.com` | exact match |
| `youtube.com` | `.youtube.com` | leading dot normalized |
| `.youtube.com` | `youtube.com` | leading dot normalized |
| `.youtube.com` | `.youtube.com` | both normalized |

## Development

```bash
scripts/dev.sh     # cargo run with passthrough args
scripts/ci.sh      # fmt + clippy + test
```
