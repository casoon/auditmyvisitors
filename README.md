# auditmyvisitors

Google Analytics 4 and Search Console reports directly in your terminal.

```
auditmyvisitors report overview
```

```
OVERVIEW  ·  last 28 days
Property: my-website.com

Metric                  Value
Total sessions          12,480
  organic               7,340 (59%)
  direct                3,120 (25%)
Engagement rate         64%

SEARCH CONSOLE
Clicks                  2,105
Impressions             87,400
CTR                     2.4%
Avg. position           14.3

INSIGHTS
⚠ High impressions, but low CTR
   87,400 impressions at only 2.4% CTR. Consider improving title and meta description.
```

## Installation

### macOS / Linux — one-line installer

```bash
curl -fsSL https://raw.githubusercontent.com/casoon/auditmyvisitors/main/install.sh | bash
```

Installs `auditmyvisitors` to `/usr/local/bin` (or `~/bin` if no write access).

### Windows

Download the latest binary from the [releases page](https://github.com/casoon/auditmyvisitors/releases/latest) and add it to your `PATH`.

### Build from source

```bash
# Requires Rust (https://rustup.rs)
git clone https://github.com/casoon/auditmyvisitors
cd auditmyvisitors
cargo build --release
./target/release/auditmyvisitors --help
```

## Getting started

```bash
# 1. Log in with Google (opens browser)
auditmyvisitors auth login

# 2. Select your GA4 and Search Console property
auditmyvisitors properties select

# 3. Run your first report
auditmyvisitors report overview
```

## Commands

### Authentication

```bash
auditmyvisitors auth login     # Log in via browser
auditmyvisitors auth status    # Check login status
auditmyvisitors auth logout    # Remove stored tokens
```

### Properties

```bash
auditmyvisitors properties list    # List all available properties
auditmyvisitors properties select  # Interactively select active property
```

### Reports

```bash
# Site overview (default: last 28 days)
auditmyvisitors report overview
auditmyvisitors report overview --days 90

# Top pages
auditmyvisitors report top-pages
auditmyvisitors report top-pages --limit 50 --sort-by clicks

# Single page detail
auditmyvisitors report page --url https://example.com/my-page

# Before/after comparison around a change date
auditmyvisitors report compare --since 2026-03-01 --before 30 --after 30
auditmyvisitors report compare --url https://example.com/page --since 2026-03-01
```

### Export

```bash
auditmyvisitors export pdf --report latest
auditmyvisitors export pdf --report latest --output ./my-report.pdf
```

## Privacy

The tool runs entirely on your local device. There is no server, no backend, no cloud infrastructure.

- OAuth tokens are stored locally at `~/.config/auditmyvisitors/tokens.json`
- No data is shared with third parties
- Read-only access to GA4 and Search Console

Full privacy policy: https://auditmyvisitors.casoon.de/datenschutz

## License

MIT
