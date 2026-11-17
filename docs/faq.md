---
title: FAQ
description: Short answers on BigQuery, where credentials are stored, export formats, measuring a deploy and Windows.
order: 5
---

## Do I need BigQuery?

No. auditmyvisitors reads the GA4 Data API and the Search Console API, that is aggregated
standard data. It does not use BigQuery raw exports. See
[Data and privacy](../guides/data-and-privacy/).

## Where are my credentials stored?

The OAuth tokens are in `tokens.json` inside the `auditmyvisitors` folder of your platform's
config directory: `~/.config` on Linux, `~/Library/Application Support` on macOS, `%APPDATA%` on
Windows. On macOS and Linux the file is readable by the owner only. There is no server in between;
`auditmyvisitors auth logout` deletes the file. The login asks for read-only scopes only. Details:
[Data and privacy](../guides/data-and-privacy/).

## Which export formats exist?

PDF, JSON and CSV. Every export fetches live data first. CSV is written per report, for example
`--report top-pages` or `--report queries`. See [Export](../guides/export/).

## Can I measure what a deploy changed?

Yes, with `report compare`: `--since` sets the change date, `--before` and `--after` set the
length of the two windows (30 days each by default), for the whole property or one URL with
`--url`. See [Reports](../guides/reports/).

## Does it run on Windows?

Yes. Download `auditmyvisitors-windows-x86_64.exe` from the latest release and put it in a
directory on your `PATH`. On macOS (Apple Silicon) and Linux (x86_64) the install script does the
download. See [Installation](../getting-started/installation/).
