# auditmyvisitors

Local CLI reporting for Google Analytics 4 and Search Console.

`auditmyvisitors` helps website owners and small teams answer practical questions quickly:

- Where is traffic growing or falling?
- Which pages have search visibility but weak on-site performance?
- Which URLs look promising for CTR or landing page improvements?
- What changed before and after a deploy, content update, or relaunch?

The current product is intentionally lightweight:

- local-first
- API-based
- no backend
- no BigQuery setup required

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

## What it is

`auditmyvisitors` is a local CLI for combined GA4 and Search Console reporting.

It is built for:

- fast site overviews
- page-level analysis
- before/after comparisons
- opportunity and risk detection
- exportable reports for sharing

It is not currently:

- a BI platform
- a Looker Studio replacement
- a BigQuery warehouse product
- a raw-event funnel analysis tool

## Data Scope & Retention

`auditmyvisitors` currently works primarily with the GA4 Data API and aggregated GA4 reporting data, plus Search Console API data. It does not currently depend on BigQuery raw exports.

This matters for planning and interpretation:

- GA4 data retention settings mainly affect non-aggregated data, such as Explorations and funnel-style analyses.
- Standard aggregated GA4 reports are not affected by that retention setting in the same way.
- If you want the longest available retention in a standard GA4 property, increase it in `Admin > Property > Data settings > Data retention`, set `Event data retention` to `14 months`, then click `Save`.
- In many setups, `User data retention` is already set to `14 months`; the setting that often still needs attention is `Event data retention`, which is frequently left at `2 months`.
- You need the `Editor` role to change this setting.
- Important limitation: standard GA4 properties allow `2 months` or `14 months`; `26/38/50 months` are only available in GA4 360, and Large/XL properties can be limited to `2 months`.

In practice, that means `auditmyvisitors` is well suited for:

- live reporting
- comparisons around a specific change date
- page and landing page analysis
- opportunity detection on top of standard GA4 and Search Console data

Current limits to keep in mind:

- no BigQuery dependency in the core product
- no raw-event funnels or event-sequence journey analysis
- no exact query-to-conversion attribution model
- no warehouse-style joins against CRM, CMS, or product databases

If BigQuery support is added later, it should extend the product rather than redefine the core workflow.

## Roadmap

The current roadmap is documented in [ziel.md](/Users/jseidel/GitHub/auditmyvisitors/ziel.md:1). In short:

- `Now`: strengthen overview, page, top-pages, compare, and PDF export on top of GA4 Data API and Search Console API.
- `Next`: add better segmentation, directory analysis, page-type logic, and scoring.
- `Later`: optionally add BigQuery-based raw-data and advanced analysis features for teams that need them.

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
