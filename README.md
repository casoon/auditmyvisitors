# audit-my-visitors

Google Analytics 4 and Search Console reports directly in your terminal.

```
audit-my-visitors report overview
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

Installs `audit-my-visitors` to `/usr/local/bin` (or `~/bin` if no write access).

### Windows

Download the latest binary from the [releases page](https://github.com/casoon/auditmyvisitors/releases/latest) and add it to your `PATH`.

### Build from source

```bash
# Requires Rust (https://rustup.rs)
git clone https://github.com/casoon/auditmyvisitors
cd auditmyvisitors
cargo build --release
./target/release/audit-my-visitors --help
```

## Getting started

```bash
# 1. Log in with Google (opens browser)
audit-my-visitors auth login

# 2. Select your GA4 and Search Console property
audit-my-visitors properties select

# 3. Run your first report
audit-my-visitors report overview
```

## Commands

### Authentication

```bash
audit-my-visitors auth login     # Log in via browser
audit-my-visitors auth status    # Check login status
audit-my-visitors auth logout    # Remove stored tokens
```

### Properties

```bash
audit-my-visitors properties list    # List all available properties
audit-my-visitors properties select  # Interactively select active property
```

### Reports

```bash
# Site overview (default: last 28 days)
audit-my-visitors report overview
audit-my-visitors report overview --days 90

# Top pages
audit-my-visitors report top-pages
audit-my-visitors report top-pages --limit 50 --sort-by clicks

# Single page detail
audit-my-visitors report page --url https://example.com/my-page

# Before/after comparison around a change date
audit-my-visitors report compare --since 2026-03-01 --before 30 --after 30
audit-my-visitors report compare --url https://example.com/page --since 2026-03-01
```

### Export

```bash
audit-my-visitors export pdf --report latest
audit-my-visitors export pdf --report latest --output ./my-report.pdf
```

## Privacy

The tool runs entirely on your local device. There is no server, no backend, no cloud infrastructure.

- OAuth tokens are stored locally at `~/.config/audit-my-visitors/tokens.json`
- No data is shared with third parties
- Read-only access to GA4 and Search Console

Full privacy policy: https://auditmyvisitors.casoon.de/datenschutz

## License

MIT
