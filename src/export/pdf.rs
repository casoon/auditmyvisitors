//! PDF report generation using renderreport / Typst.

use anyhow::Context;
use renderreport::components::advanced::{Grid, PageBreak};
use renderreport::components::text::{Label, TextBlock};
use renderreport::components::{AuditTable, MetricCard, TableColumn};
use renderreport::prelude::*;

use crate::domain::InsightSeverity;
use super::builder::ReportViewModel;

pub fn generate(vm: &ReportViewModel, output_path: &str) -> anyhow::Result<()> {
    let engine = create_engine()?;
    let mut b = engine
        .report("auditmyvisitors")
        .metadata("date", &vm.created_at)
        .metadata("author", "Audit My Visitors")
        .metadata("footer_prefix", "")
        .metadata("footer_link_url", "");

    // ════════════════════════════════════════════════════════════════════════
    // 1. COVER
    // ════════════════════════════════════════════════════════════════════════

    b = b
        .add_component(Label::new("AUDIT MY VISITORS").with_size("10pt").bold().with_color("#0f766e"))
        .add_component(Label::new(format!("Traffic Audit - {}", vm.property_name)).with_size("26pt").bold())
        .add_component(
            Label::new(format!("{} - Erstellt am {}", vm.date_range, vm.created_at))
                .with_size("11pt").with_color("#475569"),
        );

    let sessions_label = match &vm.exec_sessions_trend {
        Some(t) => format!("{} ({})", vm.exec_sessions, t.label),
        None    => vm.exec_sessions.clone(),
    };
    let clicks_label = match &vm.exec_clicks_trend {
        Some(t) => format!("{} ({})", vm.exec_clicks, t.label),
        None    => vm.exec_clicks.clone(),
    };

    let mut cover_grid = Grid::new(2);
    cover_grid = add_metric(cover_grid, "Sessions",       &sessions_label,       "#0f766e");
    cover_grid = add_metric(cover_grid, "Organisch",      &vm.exec_organic_pct,  "#16a34a");
    cover_grid = add_metric(cover_grid, "Search Clicks",  &clicks_label,         "#0369a1");
    cover_grid = add_metric(cover_grid, "Avg. Position",  &vm.exec_avg_position, "#7c3aed");
    b = b.add_component(cover_grid);

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 2. EXECUTIVE SUMMARY
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Executive Summary").with_level(1));

    // Traffic trend
    if let Some(trend) = &vm.exec_sessions_trend {
        let msg = if trend.is_positive {
            format!(
                "Traffic waechst: {} Sessions ({} gg. Vorperiode). Search Clicks: {}.",
                vm.exec_sessions, trend.label, vm.exec_clicks
            )
        } else {
            format!(
                "Traffic sinkt: {} Sessions ({} gg. Vorperiode). Ursachenanalyse empfohlen.",
                vm.exec_sessions, trend.label
            )
        };
        let c = if trend.is_positive {
            Callout::success(msg).with_title("Traffic-Trend")
        } else {
            Callout::warning(msg).with_title("Traffic-Trend")
        };
        b = b.add_component(c);
    }

    // Top 3 To-Dos
    if !vm.top3_todos.is_empty() {
        let body = vm.top3_todos.iter().enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, t))
            .collect::<Vec<_>>()
            .join("\n");
        b = b.add_component(
            Callout::info(body).with_title("Top 3 To-Dos diese Woche")
        );
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 3. MASSNAHMEN & OPPORTUNITIES
    // ════════════════════════════════════════════════════════════════════════

    b = b
        .add_component(Section::new("Massnahmen & Opportunities").with_level(1))
        .add_component(
            TextBlock::new(
                "Priorisiert nach Score = Impact x Confidence / Aufwand. \
                 Potenzial = geschaetzte zusaetzliche Klicks pro Zeitraum."
            ).with_line_height("1.4em")
        );

    if vm.opportunities.is_empty() {
        b = b.add_component(
            Callout::success("Keine kritischen Luecken identifiziert.")
                .with_title("Gute Ausgangslage")
        );
    } else {
        let mut opp_table = AuditTable::new(vec![
            TableColumn::new("Score").with_width("10%"),
            TableColumn::new("Typ").with_width("20%"),
            TableColumn::new("Keyword / Seite").with_width("25%"),
            TableColumn::new("+Klicks").with_width("10%"),
            TableColumn::new("%").with_width("10%"),
            TableColumn::new("Aufwand").with_width("12%"),
        ]).with_title("Priorisierte Massnahmen");

        for o in &vm.opportunities {
            opp_table = opp_table.add_row(vec![
                o.score.clone(),
                o.type_label.clone(),
                o.keyword_or_url.clone(),
                o.estimated_clicks.clone(),
                o.pct_potential.clone(),
                o.effort.clone(),
            ]);
        }
        b = b.add_component(opp_table);

        // Opportunity Details (top 5)
        b = b.add_component(Section::new("Details Top 5").with_level(2));
        for o in vm.opportunities.iter().take(5) {
            b = b.add_component(
                Callout::info(format!("Massnahme: {}\n\nDaten: {}", o.action, o.context))
                    .with_title(format!("[Score {}] {} - \"{}\"", o.score, o.type_label, o.keyword_or_url))
            );
        }
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 4. GOOGLE ANALYTICS
    // ════════════════════════════════════════════════════════════════════════

    b = b
        .add_component(Section::new("Google Analytics").with_level(1))
        .add_component(Section::new("Traffic nach Kanal").with_level(2))
        .add_component(
            TextBlock::new(format!("Engagement Rate: {}  -  Zeitraum: {}", vm.engagement_rate, vm.date_range))
                .with_line_height("1.4em"),
        );

    if !vm.channel_rows.is_empty() {
        let mut table = AuditTable::new(vec![
            TableColumn::new("Kanal").with_width("50%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Anteil").with_width("25%"),
        ]);
        for row in &vm.channel_rows {
            table = table.add_row(vec![row.channel.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(table);
    }

    // Top Referrer
    b = b.add_component(Section::new("Traffic-Quellen").with_level(2));
    if !vm.top_sources.is_empty() {
        let mut table = AuditTable::new(vec![
            TableColumn::new("Quelle").with_width("55%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Anteil").with_width("20%"),
        ]).with_title("Top Referrer");
        for row in &vm.top_sources {
            table = table.add_row(vec![row.source.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(table);
    }

    // KI-Traffic
    b = b.add_component(Section::new("KI-Traffic").with_level(2));
    if !vm.ai_source_rows.is_empty() {
        let total = vm.total_sessions_raw();
        let ai_pct = if total > 0 {
            format!("{:.1}%", vm.ai_sessions_total as f64 / total as f64 * 100.0)
        } else { "0%".into() };

        b = b.add_component(
            Callout::success(format!(
                "{} KI-Sessions ({} des Gesamttraffics). KI-Tools zitieren Inhalte dieser Website.",
                super::builder::fmt_num(vm.ai_sessions_total), ai_pct
            )).with_title("KI-Traffic erkannt")
        );

        // KI sources
        let mut ai_src_table = AuditTable::new(vec![
            TableColumn::new("KI-Tool").with_width("55%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Anteil").with_width("20%"),
        ]).with_title("KI-Referrer");
        for row in &vm.ai_source_rows {
            ai_src_table = ai_src_table.add_row(vec![row.source.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(ai_src_table);

        // KI traffic per page
        if !vm.ai_page_rows.is_empty() {
            b = b.add_component(Section::new("KI-Traffic nach Seite").with_level(2));
            let mut ai_page_table = AuditTable::new(vec![
                TableColumn::new("Seite").with_width("60%"),
                TableColumn::new("KI-Sessions").with_width("20%"),
                TableColumn::new("Anteil").with_width("20%"),
            ]).with_title("Welche Inhalte werden von KI-Tools zitiert?");
            for row in &vm.ai_page_rows {
                ai_page_table = ai_page_table.add_row(vec![row.url.clone(), row.sessions.clone(), row.share_pct.clone()]);
            }
            b = b.add_component(ai_page_table);
        }
    } else {
        b = b.add_component(
            Callout::info(
                "Kein messbarer KI-Traffic (ChatGPT, Perplexity, Claude, Gemini...) im Zeitraum. \
                 Strukturierte Inhalte und klare Definitionen erhoehen KI-Sichtbarkeit."
            ).with_title("Noch kein KI-Traffic")
        );
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 5. SEARCH CONSOLE
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Search Console").with_level(1));

    let mut sc_grid = Grid::new(2);
    sc_grid = add_metric(sc_grid, "Impressionen", &vm.search_impressions, "#0369a1");
    sc_grid = add_metric(sc_grid, "Klicks",       &vm.exec_clicks,        "#16a34a");
    sc_grid = add_metric(sc_grid, "CTR",          &vm.search_ctr,         "#d97706");
    sc_grid = add_metric(sc_grid, "Avg. Position",&vm.exec_avg_position,  "#7c3aed");
    b = b.add_component(sc_grid);

    if !vm.top_queries.is_empty() {
        let mut q_table = AuditTable::new(vec![
            TableColumn::new("Suchanfrage").with_width("40%"),
            TableColumn::new("Klicks").with_width("15%"),
            TableColumn::new("Impr.").with_width("15%"),
            TableColumn::new("CTR").with_width("15%"),
            TableColumn::new("Position").with_width("15%"),
        ]).with_title("Top Suchanfragen");
        for q in &vm.top_queries {
            q_table = q_table.add_row(vec![q.query.clone(), q.clicks.clone(), q.impressions.clone(), q.ctr.clone(), q.position.clone()]);
        }
        b = b.add_component(q_table);
    }

    if !vm.opportunity_queries.is_empty() {
        b = b.add_component(
            Callout::info("Position 4-20, niedrige CTR - bessere Titles koennen hier Klicks generieren.")
                .with_title("CTR-Potenzial")
        );
        let mut opp_q = AuditTable::new(vec![
            TableColumn::new("Suchanfrage").with_width("40%"),
            TableColumn::new("Impr.").with_width("15%"),
            TableColumn::new("CTR").with_width("15%"),
            TableColumn::new("Position").with_width("15%"),
            TableColumn::new("Klicks").with_width("15%"),
        ]).with_title("Opportunity-Anfragen");
        for q in &vm.opportunity_queries {
            opp_q = opp_q.add_row(vec![q.query.clone(), q.impressions.clone(), q.ctr.clone(), q.position.clone(), q.clicks.clone()]);
        }
        b = b.add_component(opp_q);
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 6. TOP SEITEN
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Top Seiten").with_level(1));
    if !vm.top_pages.is_empty() {
        let mut pages_table = AuditTable::new(vec![
            TableColumn::new("Seite").with_width("40%"),
            TableColumn::new("Sessions").with_width("15%"),
            TableColumn::new("Organisch").with_width("15%"),
            TableColumn::new("Klicks").with_width("15%"),
            TableColumn::new("Pos.").with_width("15%"),
        ]).with_title("Seiten nach Traffic");
        for p in &vm.top_pages {
            pages_table = pages_table.add_row(vec![p.url.clone(), p.sessions.clone(), p.organic.clone(), p.clicks.clone(), p.position.clone()]);
        }
        b = b.add_component(pages_table);
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 7. ALLE INSIGHTS
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Diagnose & Insights").with_level(1));
    if vm.insights.is_empty() {
        b = b.add_component(Callout::success("Keine weiteren Auffaelligkeiten.").with_title("Alles in Ordnung"));
    } else {
        for insight in &vm.insights {
            let c = match insight.severity {
                InsightSeverity::Critical => Callout::error(&insight.explanation).with_title(&insight.headline),
                InsightSeverity::Warning  => Callout::warning(&insight.explanation).with_title(&insight.headline),
                InsightSeverity::Positive => Callout::success(&insight.explanation).with_title(&insight.headline),
                InsightSeverity::Info     => Callout::info(&insight.explanation).with_title(&insight.headline),
            };
            b = b.add_component(c);
        }
    }

    // ── Render ────────────────────────────────────────────────────────────────
    let built = b.build();
    let bytes = engine.render_pdf(&built).context("PDF-Rendering fehlgeschlagen")?;
    std::fs::write(output_path, bytes)
        .with_context(|| format!("Kann PDF nicht schreiben: {}", output_path))?;
    Ok(())
}

fn create_engine() -> anyhow::Result<renderreport::Engine> {
    use renderreport::theme::{Theme, TokenValue};
    let mut engine = renderreport::Engine::new()?;
    let mut theme = Theme::default_theme();
    theme.tokens.set("font.body",    TokenValue::Font("Helvetica".into()));
    theme.tokens.set("font.heading", TokenValue::Font("Georgia".into()));
    engine.set_default_theme(theme);
    Ok(engine)
}

fn add_metric(mut grid: Grid, title: &str, value: &str, color: &str) -> Grid {
    let card = MetricCard::new(title, value).with_accent_color(color);
    grid = grid.add_item(serde_json::json!({ "type": "metric-card", "data": card.to_data() }));
    grid
}
