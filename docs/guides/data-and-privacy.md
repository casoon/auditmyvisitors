---
title: Data and privacy
description: What auditmyvisitors stores, where it stores it, what it sends, and what the GA4 data it reads covers.
order: 4
---

auditmyvisitors runs entirely on your machine. There is no server and no backend: the only
network traffic goes to the Google APIs and, during login, to the local redirect listener on
`127.0.0.1:9004`.

## Access

The login asks for two OAuth scopes, both read-only:

- `https://www.googleapis.com/auth/analytics.readonly`
- `https://www.googleapis.com/auth/webmasters.readonly`

auditmyvisitors can therefore read GA4 and Search Console data, but cannot change anything in
either.

## Where files live

All state is kept in an `auditmyvisitors` folder inside the platform's config directory:

| Platform | Config directory |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME`, or `~/.config` |
| macOS | `~/Library/Application Support` |
| Windows | `%APPDATA%` (`C:\Users\<name>\AppData\Roaming`) |

Inside it:

| File | Content |
| --- | --- |
| `config.toml` | Selected properties, report defaults, insight thresholds, manual clusters ([Configuration](../configuration/)) |
| `tokens.json` | OAuth tokens; on macOS and Linux readable by the owner only (`0600`) |
| `snapshots/<property>/<YYYY-MM-DD>.json` | Key metrics of each `report overview` run |

PDF exports go to `./output/` in the current directory unless `--output` says otherwise.
`auditmyvisitors auth logout` deletes `tokens.json`.

## Terminal output

Colour follows `NO_COLOR`, and output redirected to a file or a pipe is written as plain text.

## What the data covers

auditmyvisitors works with the GA4 Data API and aggregated GA4 reporting data, plus the Search
Console API. It does not use BigQuery raw exports.

- GA4's data retention setting mainly affects non-aggregated data, such as Explorations and
  funnel analyses. Standard aggregated reports, which auditmyvisitors reads, are not affected
  by it in the same way.
- To keep the longest available retention in a standard GA4 property, go to
  *Admin → Property → Data settings → Data retention*, set *Event data retention* to
  *14 months* and save. The *Editor* role is needed for this.
- Standard GA4 properties offer 2 or 14 months; 26, 38 and 50 months exist only in GA4 360.

Out of scope with this data: raw-event funnels and event-sequence journeys, an exact
query-to-conversion attribution, and joins against CRM, CMS or product databases.

Each Google request is attempted up to three times on rate limits (HTTP 429) and server errors,
with backoff and honouring `Retry-After`, and times out after 30 seconds. An export with a high
`--limit` can therefore fail instead of waiting indefinitely.
