---
title: Overview
description: What auditmyvisitors is, what it covers, and how this documentation is organised.
order: 0
---

`auditmyvisitors` is a local CLI for combined reporting on Google Analytics 4 and Google
Search Console. It answers practical questions for website owners and small teams: where
traffic grows or falls, which pages have search visibility but weak on-site performance, which
URLs are worth a better title or landing page, and what changed after a deploy.

It runs on your machine against the Google APIs, with your own Google login and read-only
access. There is no backend and no BigQuery setup.

**Status:** `0.3.1`. Release binaries for macOS (Apple Silicon), Linux x86_64 and Windows
x86_64. The product website is [auditmyvisitors.casoon.de](https://auditmyvisitors.casoon.de/).

## Where to start

- [Installation](getting-started/installation/): the install script, Windows, building from source.
- [Quickstart](getting-started/quickstart/): log in, pick a property, run the first report.
- [Reports](guides/reports/): the twelve reports and what each one shows.
- [Export](guides/export/): PDF, JSON and CSV.
- [Configuration](guides/configuration/): `config.toml`, defaults, insight thresholds, clusters.
- [Data and privacy](guides/data-and-privacy/): what is stored where, what the GA4 data covers.
- [How it works](guides/how-it-works/): the login, one report run, the Google APIs it reads.
- [CLI reference](reference/cli/): every command and flag.

## What it is not

Not a BI platform, not a Looker Studio replacement, not a BigQuery warehouse product and not a
raw-event funnel tool. It works with aggregated GA4 reporting data and Search Console data;
analyses that need raw events (funnels, event sequences, exact query-to-conversion attribution)
are outside its scope.
