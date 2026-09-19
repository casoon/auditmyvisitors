# Project State

## Zweck

`auditmyvisitors` ist ein lokales CLI-Tool, das Google Analytics 4 und Google Search
Console zu einem gemeinsamen Reporting zusammenführt: Traffic-Überblick, Seiten- und
Query-Analyse, Vorher/Nachher-Vergleiche, regelbasierte Insights und PDF-Export.

Kein Backend, kein Server, kein BigQuery. Alles läuft auf dem Rechner des Nutzers
gegen die Google-APIs, mit dessen eigenem OAuth-Login.

## Stack

| Bereich | Umsetzung |
|---|---|
| Sprache / Edition | Rust, Edition 2021, MSRV 1.88 |
| Async-Runtime | `tokio` (Feature `full`) |
| HTTP | `reqwest` mit `rustls-tls`, ohne default features |
| CLI | `clap` (derive) + `inquire` für den interaktiven Modus |
| Terminal-Ausgabe | `runemark` (Farbpolitik, Insight-Reports), `comfy-table` (Tabellen), `indicatif` (Spinner) |
| PDF | `renderreport` 0.2.4 (Typst-basiert) |
| Konfiguration | `toml` in `~/.config/auditmyvisitors/config.toml` |
| Fehler | `thiserror` (`AppError`) + `anyhow` (Binary-Ebene) |
| Logging | `tracing` / `tracing-subscriber`, nur bei `--verbose` |

## Einstiegspunkte

- `src/main.rs` — Binary-Entry, Command-Dispatch, Orchestrierung der Report-Läufe
- `src/cli/mod.rs` — vollständige Clap-Kommandostruktur
- `src/interactive/mod.rs` — Menümodus, wenn `auditmyvisitors` ohne Subcommand startet

## Authentifizierung

OAuth2 Authorization Code Flow mit PKCE (`src/auth/`). Der Browser öffnet den
Google-Consent, ein lokaler TCP-Listener (`src/auth/server.rs`) nimmt den Redirect
entgegen, Tokens landen als JSON unter `~/.config/auditmyvisitors/tokens.json`
(`src/storage/mod.rs`). Refresh läuft automatisch über `auth::ensure_valid_token()`. Die Token-Datei wird
unter Unix beim Schreiben auf `0600` gesetzt.

Scopes (minimal, read-only):
`analytics.readonly`, `webmasters.readonly`.

Client-ID und Client-Secret werden über `option_env!` zur Compile-Zeit eingebettet
(`src/auth/credentials.rs`); ohne gesetzte Variablen baut das Binary mit Platzhaltern.

## Externe Dienste

- `analyticsadmin.googleapis.com/v1beta` — Property-Listing
- `analyticsdata.googleapis.com/v1beta` — `runReport` / `batchRunReports`
- `searchconsole.googleapis.com/v1` + `www.googleapis.com/webmasters/v3` —
  Search Analytics, Sitemaps, URL-Inspection

## Persistenz

Ausschließlich lokal unter `~/.config/auditmyvisitors/`:

- `config.toml` — ausgewählte Properties, Report-Defaults, Insight-Schwellwerte, manuelle Cluster
- `tokens.json` — OAuth-Tokens
- `snapshots/<property-slug>/<YYYY-MM-DD>.json` — Metrik-Snapshots für Trendvergleiche

PDF-Exporte landen per Default unter `./output/<property-slug>-<datum>.pdf`.

## Tests

66 Unit-Tests, inline in den jeweiligen Modulen (`#[cfg(test)]`), kein separates
`tests/`-Verzeichnis. Getestet werden reine Funktionen: Scores, Intent-Klassifikation,
Clustering, Schwellwertlogik, Insight-Rendering, Formatierung. Für die API-Clients gibt
es keine Tests.

`cargo test` und `cargo clippy --all-targets -- -D warnings` laufen sauber durch.

## Build & Release

- `make build` / `make release` — sourcen `.env.local` (OAuth-Credentials) und bauen
- `.github/workflows/ci.yml` — auf Push und PR gegen `main`: Clippy mit
  `-D warnings` und `cargo test` auf Linux, `cargo check --all-targets` auf Windows
  für die `#[cfg(not(unix))]`-Pfade, und derselbe Check gegen einen auf die MSRV
  gepinnten 1.88-Toolchain
- `.github/workflows/release.yml` — baut auf Tag `v*` für macOS arm64, Linux x86_64
  (musl) und Windows x86_64 und hängt die Binaries ans Release. Ein Tag mit
  Bindestrich (`v1.2.3-rc.1`) erscheint als Pre-Release.
- `Cargo.lock` ist versioniert, damit Release-Builds reproduzierbar sind

## Weiterführend

- [architecture.md](architecture.md) — Schichten, Datenfluss, Modulgrenzen
- [conventions.md](conventions.md) — Regeln für künftige Änderungen
- [constraints.md](constraints.md) — harte Rahmenbedingungen
- [decisions.md](decisions.md) — aktuell gültige Entscheidungen
