# Conventions

Dokumentiert sind nur Regeln, die im Repo erkennbar durchgehalten werden.

## Modulstruktur

- Ein Verzeichnis je fachlichem Bereich mit `mod.rs`; einzelne Reports und
  API-Clients sind eigene Dateien darin.
- Ein Report = ein Modul unter `src/reports/`, exportiert über `src/reports/mod.rs`.
- Analyse-Logik gehört in die eigene Engine (`insights/`, `opportunities/`,
  `narrative/`, `intent/`, `clusters/`, `page_audit.rs`), nicht in `ui/` oder
  `export/`.

## Datenfluss

- Google-API-Responses werden im jeweiligen Client in `src/google/` gemappt und
  verlassen ihn nur als Domain-Typ.
- Jede neue Ausgabe (Terminal, PDF, CSV, JSON) liest Domain-Modelle. Für PDF läuft
  das über `export/builder.rs`, das die Formatierung vorwegnimmt — die
  `ReportViewModel`-Felder sind bereits Strings.
- Alle HTTP-Aufrufe gegen Google laufen über `google::send_with_retry()`, nicht über
  einen eigenen Client.
- Reports rufen die Clients nicht direkt auf, sondern nehmen `&impl GoogleApi`. Eine
  neue Google-Operation kommt als Trait-Methode dazu, nicht als freier Aufruf —
  sonst ist der betroffene Report wieder nur mit echtem Login ausführbar.

## Fehlerbehandlung

- Bibliotheksartige Schichten (`google/`, `reports/`, `config/`-Zugriffe) geben
  `errors::Result<T>` mit typisiertem `AppError` zurück.
- `main.rs` und interaktiver Modus arbeiten mit `anyhow::Result` und `.context(…)`
  für Nutzer-lesbare Meldungen.
- Fehlermeldungen nennen den nächsten Schritt, nicht nur die Ursache
  (`"Token not found — run \`auth login\` first"`).

## Konfiguration

- Schwellwerte für Insights stehen in `ThresholdsConfig`, nicht als Literale in der
  Engine. Neue Schwellwerte kommen dort dazu, inklusive Default in `impl Default`.
- Neue Config-Felder werden mit `#[serde(default)]` versehen, damit bestehende
  `config.toml`-Dateien weiter laden.

## Terminal-Ausgabe

- Sämtliche Ausgabe liegt in `src/ui/mod.rs` als `print_*`-Funktion je Report.
- **Kein Modul erzeugt eigene ANSI-Codes.** Farbe und Betonung laufen über das
  `Paint`-Trait aus `src/ui/style.rs`; die Methode benennt die Absicht
  (`heading`, `strong`, `accent`, `ok`, `warn`, `err`, `muted`), nicht die Farbe.
- Tabellen über `comfy-table`, Ladevorgänge über `ui::spinner()`.
- `tracing` wird nur bei `--verbose` initialisiert; reguläre Ausgabe geht über
  `println!`, nicht über Logs.

## Neue Reports

Ein neuer Report wird an beiden Einstiegen angebunden: als `ReportAction`-Variante
in `src/cli/mod.rs` **und** im Menü in `src/interactive/menu.rs`. Sonst ist er für
die Hälfte der Nutzer unsichtbar.

## Tests

- Unit-Tests liegen inline im jeweiligen Modul in einem `#[cfg(test)] mod tests`.
- Getestet werden vor allem reine Funktionen: Scores, Klassifikation, Clustering,
  Schwellwertlogik, Formatierung. Für API-Clients gibt es keine Tests.
- Domain-Structs sind öffentlich und ohne Konstruktor — wird ein Feld ergänzt,
  müssen alle Testfixtures mitgezogen werden.
- Report-Module werden über `FixtureGoogleApi` getestet, das Requests an ihren
  Dimensionen erkennt — und bei Reports, die dieselbe Frage für zwei Zeiträume
  stellen, zusätzlich am Startdatum (`with_report_at` / `with_search_at`). Ein nicht
  hinterlegter Request paniert absichtlich: ein Test, der stillschweigend über leere
  Daten assertet, beweist nichts.

## Commits und Versionierung

- Version in `Cargo.toml` und Git-Tag `vX.Y.Z` gehören zusammen; der Release-Workflow
  triggert auf `v*`.
- Commit-Betreff im Stil `Bump version to 0.2.9: <was sich geändert hat>` für Releases,
  ansonsten kurze imperative Beschreibung.
- Keine `Co-Authored-By`-Trailer.

## Formatierung

**Der Bestand ist nicht rustfmt-formatiert** — an vielen Stellen sind Felder und
Match-Arme von Hand ausgerichtet, was `cargo fmt` auflösen würde. Deshalb läuft in CI
kein `cargo fmt --check`, und ein pauschaler Formatierungslauf über das Repo ist keine
kleine Änderung, sondern eine eigene Entscheidung.

`cargo clippy --all-targets -- -D warnings` läuft sauber und wird in CI erzwungen.
