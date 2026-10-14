---
title: Quickstart
description: Log in with Google, choose a property, run the first report.
order: 2
---

## 1. Log in

```sh
auditmyvisitors auth login
```

The browser opens Google's consent screen. auditmyvisitors asks for two read-only scopes,
`analytics.readonly` and `webmasters.readonly`, and receives the answer on a local listener
at `http://127.0.0.1:9004`. The login uses the OAuth authorization code flow with PKCE; the
tokens are stored in your config directory (see
[Data and privacy](../../guides/data-and-privacy/)) and refreshed automatically.

`auditmyvisitors auth status` shows whether you are logged in, `auditmyvisitors auth logout`
deletes the stored tokens.

## 2. Choose a property

```sh
auditmyvisitors properties select
```

Pick the GA4 property from the list, then the Search Console property, or `(skip)`. Without a
Search Console property, some reports do not run, see [Reports](../../guides/reports/). The selection is saved to `config.toml`. `auditmyvisitors properties list` prints all
properties your account can read, without changing the selection.

## 3. Run a report

```sh
auditmyvisitors report overview
```

The overview combines traffic, engagement and search performance for the last 28 days and adds
a management summary, growth and trend highlights and topic clusters. Each run stores a
snapshot of its key metrics for the day; a run on a later day compares against the most
recent earlier snapshot.

Every report except `compare` takes `--days` (`-d`) for a different period:

```sh
auditmyvisitors report overview --days 90
```

## Without flags

```sh
auditmyvisitors
```

Without a subcommand, auditmyvisitors starts an interactive mode. It connects the Google
account if needed, asks for the property and the time range (7, 28 or 90 days, or a custom
number), and runs the full report in one pass: overview, growth, trends, top pages, channels,
queries, opportunities, clusters, AI traffic, devices, countries and decay, followed by the
management summary. A menu then offers *Drill into a page…*, *Export…*, *Switch property*
and *Exit*.

## Next

- [Reports](../../guides/reports/): what each report shows.
- [Export](../../guides/export/): PDF, JSON and CSV.
