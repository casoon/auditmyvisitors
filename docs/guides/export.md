---
title: Export
description: Save a report as PDF, JSON or CSV. Every export fetches live data first.
order: 2
---

Exports take `--days` (`-d`) like the reports and fetch live data from Google before writing.
JSON and CSV go to stdout unless `--output` (`-o`) names a file; missing parent directories are
created.

## PDF

```sh
auditmyvisitors export pdf
auditmyvisitors export pdf --limit 200 --output ./my-report.pdf
auditmyvisitors export pdf --days 90 --output ./report-90d.pdf
```

Without `--output`, the file is written to `./output/<property>-YYYY-MM-DD.pdf`, where
`<property>` is the GA4 property name in lower case with spaces replaced by hyphens.

The PDF is built from the overview, up to 500 top pages and up to 500 queries. It contains:

- the overview with a management summary and the top three to-dos,
- the list of all pages by sessions, as many as `--limit` (`-n`, default 20),
- the strongest and the weakest 20 pages and the 10 most isolated pages, whatever `--limit` says,
- click-gap pages: position 4 to 15, CTR below 2 %, more than 100 impressions,
- the keyword table with the best-ranking page per query,
- site health from Search Console: the submitted sitemaps with submitted and indexed URLs,
  warnings and errors, and a URL inspection of up to 10 pages that have more than 10 sessions
  but no search impressions.

Site health and the keyword table need a Search Console property; without one the PDF is
written without them. The layout is rendered with
[renderreport](https://crates.io/crates/renderreport) (Typst).

## JSON

```sh
auditmyvisitors export json
auditmyvisitors export json --output ./report.json
```

The overview and the top 50 pages by sessions, serialised directly from the internal report
models.

## CSV

```sh
auditmyvisitors export csv --report top-pages
auditmyvisitors export csv --report queries --limit 100 --output ./queries.csv
```

`--report` (`-r`) is required and takes one of `top-pages`, `queries`, `opportunities`,
`channels`, `clusters`, `devices`, `countries` or `decay`. `--limit` (`-n`) applies to
`top-pages` (default 20, or `top_pages_limit` from the config), `queries` (default 30) and
`countries` (default 20).
