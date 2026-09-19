# Decisions

Aktuell gültige Entscheidungen — kein Archiv abgelöster Varianten.

> **API-Layer → Domain → Ausgabe**
> Google-Responses werden in `src/google/` in interne Domain-Modelle gemappt und
> erreichen `ui/` und `export/` nie direkt.
> *Grund:* entkoppelt die Darstellung von API-Formaten und hält Terminal-, PDF-,
> CSV- und JSON-Ausgabe auf derselben Datenbasis.
> *Konsequenz:* eine neue Kennzahl entsteht zuerst als Domain-Feld, danach in den
> Ausgaben.

> **Kein BigQuery im Kern**
> Das Produkt arbeitet mit GA4 Data API und Search Console API, ohne Warehouse.
> *Grund:* keine Einrichtung, keine Kosten, sofort nutzbar für Website-Betreiber ohne
> Google-Cloud-Kenntnisse.
> *Konsequenz:* Funktionen, die zwingend Rohdaten brauchen (Event-Funnels,
> Journey-Analysen), gehören nicht ins Produkt. BigQuery bliebe ein optionaler
> Zusatz-Provider, kein Ersatz des bestehenden Pfads.

> **Eingebettete OAuth-Credentials, PKCE-Login**
> Client-ID und Secret sind zur Compile-Zeit über `option_env!` im Binary
> (`src/auth/credentials.rs`), der Login läuft als Authorization-Code-Flow mit PKCE
> über einen lokalen Redirect-Listener.
> *Grund:* Nutzer brauchen kein eigenes Google-Cloud-Projekt und keine
> Service-Account-Datei. Für Desktop-Apps dokumentiert Google das Secret ausdrücklich
> als nicht geheim.
> *Konsequenz:* Builds ohne gesetzte Credentials sind funktionsfähig, aber nicht
> login-fähig — deshalb der Umweg über `make build` / `make release`.

> **Lokaler Zustand unter `~/.config/auditmyvisitors/`**
> Tokens, Konfiguration und Snapshots liegen als Dateien im Config-Verzeichnis des
> Nutzers.
> *Grund:* passt zum Local-First-Versprechen; es gibt keinen Ort, an dem Daten sonst
> liegen könnten.
> *Konsequenz:* `tokens.json` liegt im Klartext, wird unter Unix aber beim Schreiben
> auf `0600` gesetzt. Ein System-Keychain (`keyring`) wurde bewusst verworfen: er
> bräuchte unter Linux ohne laufenden Secret-Service wieder einen Datei-Fallback und
> löst den realistischen Fall — ein anderes Konto auf derselben Maschine — nicht besser
> als der Dateimodus.

> **Schwellwerte sind Konfiguration, keine Konstanten**
> Alle Grenzwerte der Insight-Engine stehen in `ThresholdsConfig` und lassen sich in
> `config.toml` überschreiben.
> *Grund:* die sinnvollen Grenzen hängen von Website-Typ und Größe ab.
> *Konsequenz:* neue Regeln bringen ihren Schwellwert in der Config mit, inklusive
> Default.

> **Regelbasierte Insights und Narrative, kein LLM**
> Management-Summary, Abschnittstexte und Empfehlungen entstehen aus expliziten Regeln
> in `src/narrative/` und `src/insights/`.
> *Grund:* deterministisch, offline, kostenlos, nachvollziehbar.
> *Konsequenz:* neue Aussagen sind neue Regeln mit klarer Auslösebedingung, keine
> Prompt-Arbeit.

> **PDF über `renderreport`/Typst statt direktem PDF-Layout**
> `export/builder.rs` erzeugt ein flaches `ReportViewModel`, `export/pdf.rs` rendert
> es mit `renderreport`-Komponenten.
> *Grund:* Layoutarbeit bleibt in der Bibliothek; der Export-Code beschreibt nur noch
> Inhalte.
> *Konsequenz:* Formatierung (Zahlen, Prozente) passiert im Builder, nicht im
> Renderer — `ReportViewModel`-Felder sind Strings.

> **Interaktiver Modus als Default ohne Subcommand**
> `auditmyvisitors` ohne Argumente startet das `inquire`-Menü
> (`src/interactive/`), das dieselben Report-Funktionen aufruft wie die Subcommands.
> *Grund:* Zielgruppe sind auch Nutzer, die keine CLI-Flags nachschlagen wollen.
> *Konsequenz:* ein neuer Report wird an beiden Stellen angebunden — Clap-Enum und
> Menü —, sonst ist er nur halb erreichbar.

> **runemark als einzige Präsentationsschicht**
> Farbe, Symbole und die Insight-Ausgabe laufen über `runemark`; `src/ui/style.rs`
> hält eine prozessweite `Console` und bietet das `Paint`-Trait. Kein anderes Modul
> erzeugt ANSI-Codes.
> *Grund:* eine Farbpolitik statt zweier. `colored` färbte auch in Pipes und ignorierte
> `NO_COLOR`; runemarks `ColorMode::Auto` tut beides richtig, und die Insights bekommen
> mit `Report`/`FindingGroup`/`Verdict` eine Struktur, die zu ihrer Severity und
> Kategorie passt.
> *Konsequenz:* neue Ausgabe wählt eine `Paint`-Methode nach Absicht, nicht nach Farbe.
> Tabellen bleiben bei `comfy-table` (runemark hat bewusst keine), Prompts bei
> `inquire` (runemarks `select` ist Unix-only, Windows ist ein Release-Target), und der
> Lade-Spinner bei `indicatif` (runemarks Progress-Sink modelliert zählbare Arbeit,
> diese Wartezeiten haben nichts zu zählen).

> **`Cargo.lock` ist versioniert**
> Das Lockfile liegt im Repository.
> *Grund:* das Crate liefert ein Binary aus. Ohne Lockfile baut der Release-Workflow
> gegen andere Dependency-Versionen als der lokale Build.
> *Konsequenz:* Dependency-Updates sind ein sichtbarer Commit, kein Nebeneffekt des
> Build-Zeitpunkts.
