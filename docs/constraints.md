# Constraints

## Geprüft

- **Keine Server-Komponente.** Das Tool läuft vollständig lokal; es gibt kein Backend
  und keinen Dienst, an den Daten gehen. Der einzige Netzwerkverkehr geht an
  Google-APIs und an den lokalen OAuth-Redirect-Listener.
- **Read-only Google-Zugriff.** Die angeforderten Scopes sind
  `analytics.readonly` und `webmasters.readonly`. Schreibende Google-Operationen sind
  damit ausgeschlossen.
- **Kein BigQuery.** Das Produkt setzt ausschließlich auf die GA4 Data API und die
  Search Console API. BigQuery bleibt bewusst außen vor und dürfte später höchstens
  als optionaler Zusatz dazukommen, ohne den Kern-Workflow zu ersetzen.
- **OAuth-Credentials zur Compile-Zeit.** Ohne `GOOGLE_CLIENT_ID` /
  `GOOGLE_CLIENT_SECRET` beim Build enthält das Binary Platzhalter und schlägt beim
  ersten Login mit einer klaren Meldung fehl. Deshalb `make build` / `make release`
  statt `cargo build` direkt.
- **Release-Plattformen.** Der Workflow baut macOS arm64, Linux x86_64 (musl) und
  Windows x86_64. Intel-macOS wird nicht mehr unterstützt; `install.sh` erkennt
  Darwin/x86_64 und verweist auf den Build aus dem Quellcode, statt ein fehlendes
  Artefakt herunterzuladen.
- **MSRV 1.88**, in CI gegen einen 1.88-Toolchain geprüft. Den Wert setzt nicht
  dieses Crate, sondern der Abhängigkeitsbaum: `image` und `time` verlangen 1.88,
  `runemark` nur 1.85. Er gilt für das committete `Cargo.lock` — ohne Lockfile wäre
  die Zahl eine Momentaufnahme. Die Edition bleibt 2021.
- **Farbausgabe** folgt `NO_COLOR` und schreibt bei Umleitung reinen Text — durchgesetzt
  durch die eine `Console` in `src/ui/style.rs`.
- **GA4-Datenhorizont.** Aggregierte Standardberichte, kein Rohdatenzugriff: keine
  Event-Sequenz-/Funnel-Analysen, keine exakte Query-zu-Conversion-Attribution, keine
  Joins gegen CRM/CMS.
- **Rate Limits.** Google-Aufrufe werden maximal dreimal wiederholt (Backoff,
  `Retry-After`), Timeout 30 Sekunden. Länger laufende Exporte mit hohem `--limit`
  können dadurch scheitern statt beliebig zu warten.

## Annahmen (nicht verifiziert, bei Gelegenheit klären)

- Für die Google-OAuth-Verifizierung braucht die öffentliche Seite eine erreichbare
  Privacy Policy. `https://auditmyvisitors.casoon.de/` antwortet mit 200, die im README
  verlinkte `/datenschutz` mit 404 — die Anforderung ist damit aktuell nicht erfüllt.
  Die Seite liegt in einem anderen Repository. (Issue #1)
