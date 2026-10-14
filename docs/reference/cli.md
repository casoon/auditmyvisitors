---
title: CLI reference
description: Every command and flag of auditmyvisitors 0.3.1, as the binary's help prints them.
order: 1
---

```text
auditmyvisitors [OPTIONS] [COMMAND]
```

Without a command, auditmyvisitors starts the interactive mode (see the
[Quickstart](../../getting-started/quickstart/#without-flags)).

| Global option | Effect |
| --- | --- |
| `--verbose` | Debug logging (accepted by every command) |
| `-h`, `--help` | Help for the command it follows |
| `-V`, `--version` | Print the version |

## auth

| Command | Effect |
| --- | --- |
| `auth login` | Log in with your Google account (opens the browser) |
| `auth status` | Show the current authentication status |
| `auth logout` | Log out and remove the stored tokens |

## properties

| Command | Effect |
| --- | --- |
| `properties list` | List the GA4 and Search Console properties your account can read |
| `properties select` | Choose the active properties interactively and save them to `config.toml` |

## report

Every report except `compare` takes `-d`, `--days <DAYS>`: the number of days to look back,
default 28 (or `default_days` from [config.toml](../../guides/configuration/)).

| Command | Further options |
| --- | --- |
| `report overview` | – |
| `report top-pages` | `-n`, `--limit <LIMIT>` (default 20); `--sort-by <SORT_BY>`: `sessions` (default), `clicks`, `impressions` |
| `report page` | `--url <URL>` (required) |
| `report compare` | `--since <SINCE>` (required, `YYYY-MM-DD`); `--before <BEFORE>` (default 30); `--after <AFTER>` (default 30); `--url <URL>` (omit for the whole site) |
| `report opportunities` | – |
| `report queries` | `-n`, `--limit <LIMIT>` (default 30); `--sort-by <SORT_BY>`: `clicks` (default), `impressions`, `ctr`, `position` |
| `report ai-traffic` | – |
| `report channels` | – |
| `report clusters` | – |
| `report decay` | `-d` is the length of each of the two compared periods |
| `report devices` | – |
| `report countries` | `-n`, `--limit <LIMIT>` (default 20) |

What each report shows: [Reports](../../guides/reports/).

## export

Every export takes `-d`, `--days <DAYS>` (default 28).

| Command | Further options |
| --- | --- |
| `export pdf` | `-n`, `--limit <LIMIT>`: pages in the full page list (default 20; the help text names 200 as the maximum); `-o`, `--output <OUTPUT>` (default `./output/<property>-YYYY-MM-DD.pdf`) |
| `export json` | `-o`, `--output <OUTPUT>` (default stdout) |
| `export csv` | `-r`, `--report <REPORT>` (required): `top-pages`, `queries`, `opportunities`, `channels`, `clusters`, `devices`, `countries`, `decay`; `-n`, `--limit <LIMIT>` (default depends on the report); `-o`, `--output <OUTPUT>` (default stdout) |

Details: [Export](../../guides/export/).

## snapshot

| Command | Effect |
| --- | --- |
| `snapshot list` | List the snapshots saved by `report overview` for the selected property |

## Help output

```text
$ auditmyvisitors --help
Google Analytics 4 & Search Console reporting for website owners

Usage: auditmyvisitors [OPTIONS] [COMMAND]

Commands:
  auth        Manage Google authentication
  properties  List or select GA4 / Search Console properties
  report      Generate reports
  export      Export reports
  snapshot    Manage local snapshots for trend tracking
  help        Print this message or the help of the given subcommand(s)

Options:
      --verbose  Enable verbose logging
  -h, --help     Print help
  -V, --version  Print version
```

The help of every subcommand: `auditmyvisitors <command> --help`, or the
[showcase](../../../showcase/).
