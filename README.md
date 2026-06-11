# cookiecloud-sync

Periodically fetches encrypted cookies from a [CookieCloud](https://github.com/easychen/CookieCloud) server, decrypts them, and writes to a local file.

## Usage

All options accept CLI flags (local dev) or environment variables (Docker). CLI flags take precedence.

```bash
# with CLI flags (one-shot)
cargo run -- -s http://cookiecloud:8088 -u your-uuid -p your-password --once

# with CLI flags (daemon)
cargo run -- -s http://cookiecloud:8088 -u your-uuid -p your-password -i 60 -o /tmp/cookies.txt

# with env vars (Docker style)
export COOKIECLOUD_URL=http://cookiecloud:8088
export COOKIECLOUD_UUID=your-uuid
export COOKIECLOUD_PASSWORD=your-password
cargo run
```

## Docker

```bash
docker run -d \
  -e COOKIECLOUD_URL=http://cookiecloud:8088 \
  -e COOKIECLOUD_UUID=your-uuid \
  -e COOKIECLOUD_PASSWORD=your-password \
  -v /path/to/output:/output \
  ghcr.io/killbus/cookiecloud-sync:latest
```

## Configuration

| Flag | Short | Env | Default | Description |
|---|---|---|---|---|
| `--server` | `-s` | `COOKIECLOUD_URL` | *(required)* | CookieCloud server URL |
| `--uuid` | `-u` | `COOKIECLOUD_UUID` | *(required)* | User UUID |
| `--password` | `-p` | `COOKIECLOUD_PASSWORD` | *(required)* | Encryption password |
| `--output` | `-o` | `OUTPUT_FILE` | `/output/cookies.txt` | Output file path |
| `--format` | `-f` | `OUTPUT_FORMAT` | `netscape` | Output format: `netscape`, `json`, `both` |
| `--interval` | `-i` | `INTERVAL_SECS` | `300` | Sync interval in seconds |
| `--once` | — | — | `false` | Run once and exit |
| | | `RUST_LOG` | `info` | Logging filter |

## Output Formats

- **netscape** (default): Netscape HTTP cookie file format
- **json**: CookieCloud's native JSON structure (`cookie_data`, `local_storage_data`, `update_time`)
- **both**: Both formats simultaneously

## Development

```bash
scripts/dev.sh     # cargo run with passthrough args
scripts/ci.sh      # fmt + clippy + test
```
