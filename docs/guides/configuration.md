---
title: Configuration
description: config.toml — the selected properties, report defaults, brand terms, insight thresholds and manual clusters.
order: 3
---

auditmyvisitors reads `config.toml` from its config directory (see
[Data and privacy](../data-and-privacy/#where-files-live)). Without the file it uses the
defaults below. `auditmyvisitors properties select` writes the file; the other sections can be
added by hand. Every section is optional.

```toml
[properties]
ga4_property_id = "properties/123456789"
ga4_property_name = "My website"
search_console_url = "sc-domain:example.com"

[report]
default_days = 28
top_pages_limit = 20
brand_terms = ["mycompany", "my company"]

[thresholds]
low_organic_pct = 20.0
high_direct_pct = 60.0
high_organic_pct = 70.0
low_engagement_rate = 0.3
high_engagement_rate = 0.7
trend_significant_pct = 20.0
top3_dependency_pct = 60.0
min_sessions = 100

[clusters]
pricing = ["price", "pricing", "cost"]
```

The values shown in `[report]` and `[thresholds]` are the defaults. If you write one of these two
sections, give all of its keys except `brand_terms`; a section with only some keys stops every
command with `Error: Cannot load config`.

## properties

Set by `properties select`. `ga4_property_id` and `ga4_property_name` identify the GA4
property, `search_console_url` the Search Console property, as Search Console names it (a
URL-prefix property such as `https://example.com/` or a domain property such as
`sc-domain:example.com`).

## report

| Key | Default | Effect |
| --- | --- | --- |
| `default_days` | `28` | Period of every report and export without `--days` |
| `top_pages_limit` | `20` | Rows of `report top-pages` and `export csv --report top-pages` without `--limit` |
| `brand_terms` | `[]` | Queries containing one of these terms count as navigational in the intent classification |

## thresholds

The insight rules read their limits from here. Percentages are absolute (`20.0` is 20 %),
engagement rates are fractions (`0.3` is 30 %).

| Key | Default | Triggers |
| --- | --- | --- |
| `low_organic_pct` | `20.0` | a warning when the organic share is below it |
| `high_direct_pct` | `60.0` | a note when the direct share is above it |
| `high_organic_pct` | `70.0` | a positive insight when the organic share is above it |
| `low_engagement_rate` | `0.3` | a warning when the engagement rate is below it |
| `high_engagement_rate` | `0.7` | a positive insight when the engagement rate is above it |
| `trend_significant_pct` | `20.0` | a significant-trend insight for changes beyond it |
| `top3_dependency_pct` | `60.0` | a dependency warning when the top three pages carry more than this share |
| `min_sessions` | `100` | the minimum sessions before session-based insights are generated |

## clusters

Manual topic clusters for `report clusters`: a cluster name and the terms that assign a page or
query to it. They come in addition to the clusters derived from URL paths and query terms.
