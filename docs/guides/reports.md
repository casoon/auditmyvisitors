---
title: Reports
description: The twelve reports of auditmyvisitors report, what each one combines and which flags it takes.
order: 1
---

All reports read the property chosen with `auditmyvisitors properties select`. The period is the
last 28 days unless `--days` (`-d`) says otherwise or `default_days` in
[config.toml](../configuration/) sets another default. `compare` uses its own windows instead.

| Report | Shows | Flags |
| --- | --- | --- |
| `overview` | Traffic, engagement and search performance, insights, summary | `-d` |
| `top-pages` | Pages ranked by sessions, clicks or impressions | `-d`, `-n`, `--sort-by` |
| `page` | One URL in detail | `--url`, `-d` |
| `compare` | Before/after a change date, site-wide or for one URL | `--since`, `--before`, `--after`, `--url` |
| `opportunities` | Prioritised opportunities and an action plan | `-d` |
| `queries` | Query-level search performance with the best-ranking page per query | `-d`, `-n`, `--sort-by` |
| `ai-traffic` | Referrals from AI assistants and answer engines | `-d` |
| `channels` | Traffic channels with engagement metrics | `-d` |
| `clusters` | Topic clusters across pages and queries | `-d` |
| `decay` | Pages losing search performance, period over period | `-d` |
| `devices` | Traffic by device category (desktop, mobile, tablet) | `-d` |
| `countries` | Traffic by country | `-d`, `-n` |

GA4 reports paths, Search Console full URLs. Where a report uses both, the two are matched per
page path.

`top-pages`, `page` and `compare` also work with GA4 alone, their search columns then stay
empty; `ai-traffic`, `channels`, `devices` and `countries` use GA4 only anyway. `queries`, `opportunities`, `clusters` and `decay` need a Search Console property and
stop with `No Search Console property selected` otherwise. So does `overview`, because it
builds opportunities, growth, trends and clusters alongside.

## overview

```sh
auditmyvisitors report overview
auditmyvisitors report overview --days 90
```

Prints the site overview (sessions by channel, engagement rate, clicks, impressions, CTR,
average position, insights), then a management summary, growth and trend highlights and topic
clusters. Each run saves a snapshot of the key metrics under the property's name and date; when
a snapshot from an earlier day exists, the overview compares against the most recent one.
`auditmyvisitors snapshot list` shows the saved snapshots.

## top-pages

```sh
auditmyvisitors report top-pages
auditmyvisitors report top-pages --limit 50 --sort-by clicks
```

`--limit` (`-n`) defaults to 20 (or `top_pages_limit` in the config); `--sort-by` takes
`sessions` (default), `clicks` or `impressions`.

## page

```sh
auditmyvisitors report page --url https://example.com/my-page
```

GA4 and Search Console figures for one URL, with the queries it ranks for.

## compare

```sh
auditmyvisitors report compare --since 2026-03-01
auditmyvisitors report compare --url https://example.com/page --since 2026-03-01 --before 14 --after 14
```

Two windows around a change date such as a deploy: `--before` days before `--since` and
`--after` days from it, 30 each by default. Without `--url` the comparison covers the whole site.

## opportunities

```sh
auditmyvisitors report opportunities
```

Each opportunity is scored as impact × confidence / effort: impact is the estimated additional
clicks, confidence grows with the impressions (`log10(impressions + 1)`), effort is 1, 2 or 3.
The expected CTR behind the impact depends on the position. Opportunities for the same keyword
are merged into one entry, and the result ends in a three-tier action plan.

## queries

```sh
auditmyvisitors report queries --limit 100 --sort-by ctr
```

`--limit` defaults to 30; `--sort-by` takes `clicks` (default), `impressions`, `ctr` or
`position`. Each query is shown with the page it ranks best on (lowest average position), and
queries are classified by intent: informational, navigational, commercial or transactional.
Queries containing one of the `brand_terms` from the config count as navigational.

## ai-traffic

```sh
auditmyvisitors report ai-traffic
```

Sessions referred by a list of 16 known AI domains, among them `chatgpt.com`,
`perplexity.ai`, `claude.ai`, `gemini.google.com` and `copilot.microsoft.com`, and what the
pages they land on have in common.

## clusters

```sh
auditmyvisitors report clusters
```

Pages and queries grouped into topics in three ways: by the first meaningful URL path segment,
by the most frequent terms in the top queries, and by the manual clusters in
[config.toml](../configuration/#clusters).

## decay

```sh
auditmyvisitors report decay --days 28
```

Compares the last `--days` with the period of the same length before it and lists pages with a
significant decline in clicks or impressions, with the estimated click loss.

## channels, devices, countries

```sh
auditmyvisitors report channels
auditmyvisitors report devices
auditmyvisitors report countries --limit 30
```

Traffic broken down by channel, device category and country. `countries` shows 20 rows unless
`--limit` says otherwise.

All flags in one place: [CLI reference](../../reference/cli/).
