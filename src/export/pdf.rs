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
            Label::new(format!("{} - Created {}", vm.date_range, vm.created_at))
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
    cover_grid = add_metric(cover_grid, "Organic",         &vm.exec_organic_pct,  "#16a34a");
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
                "Traffic is growing: {} sessions ({} vs. previous period). Search clicks: {}.",
                vm.exec_sessions, trend.label, vm.exec_clicks
            )
        } else {
            format!(
                "Traffic is declining: {} sessions ({} vs. previous period). Root cause analysis recommended.",
                vm.exec_sessions, trend.label
            )
        };
        let c = if trend.is_positive {
            Callout::success(msg).with_title("Traffic Trend")
        } else {
            Callout::warning(msg).with_title("Traffic Trend")
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
            Callout::info(body).with_title("Top 3 To-Dos This Week")
        );
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 3. ACTIONS & OPPORTUNITIES
    // ════════════════════════════════════════════════════════════════════════

    b = b
        .add_component(Section::new("Actions & Opportunities").with_level(1))
        .add_component(
            TextBlock::new(
                "Prioritized by Score = Impact x Confidence / Effort. \
                 Potential = estimated additional clicks per period."
            ).with_line_height("1.4em")
        );

    if vm.opportunities.is_empty() {
        b = b.add_component(
            Callout::success("No critical gaps identified.")
                .with_title("Good Starting Position")
        );
    } else {
        let mut opp_table = AuditTable::new(vec![
            TableColumn::new("Score").with_width("10%"),
            TableColumn::new("Type").with_width("20%"),
            TableColumn::new("Keyword / Page").with_width("25%"),
            TableColumn::new("+Clicks").with_width("10%"),
            TableColumn::new("%").with_width("10%"),
            TableColumn::new("Effort").with_width("12%"),
        ]).with_title("Prioritized Actions");

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
                Callout::info(format!("Action: {}\n\nData: {}", o.action, o.context))
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
        .add_component(Section::new("Traffic by Channel").with_level(2))
        .add_component(
            TextBlock::new(format!("Engagement Rate: {}  -  Zeitraum: {}", vm.engagement_rate, vm.date_range))
                .with_line_height("1.4em"),
        );

    if !vm.channel_rows.is_empty() {
        let mut table = AuditTable::new(vec![
            TableColumn::new("Channel").with_width("50%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Share").with_width("25%"),
        ]);
        for row in &vm.channel_rows {
            table = table.add_row(vec![row.channel.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(table);
    }

    // Top Referrer
    b = b.add_component(Section::new("Traffic Sources").with_level(2));
    if !vm.top_sources.is_empty() {
        let mut table = AuditTable::new(vec![
            TableColumn::new("Source").with_width("55%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Share").with_width("20%"),
        ]).with_title("Top Referrer");
        for row in &vm.top_sources {
            table = table.add_row(vec![row.source.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(table);
    }

    // AI Traffic
    b = b.add_component(Section::new("AI Traffic").with_level(2));
    if !vm.ai_source_rows.is_empty() {
        let total = vm.total_sessions_raw();
        let ai_pct = if total > 0 {
            format!("{:.1}%", vm.ai_sessions_total as f64 / total as f64 * 100.0)
        } else { "0%".into() };

        b = b.add_component(
            Callout::success(format!(
                "{} AI sessions ({} of total traffic). AI tools are citing content from this website.",
                super::builder::fmt_num(vm.ai_sessions_total), ai_pct
            )).with_title("AI Traffic Detected")
        );

        // AI sources
        let mut ai_src_table = AuditTable::new(vec![
            TableColumn::new("AI Tool").with_width("55%"),
            TableColumn::new("Sessions").with_width("25%"),
            TableColumn::new("Share").with_width("20%"),
        ]).with_title("AI Referrer");
        for row in &vm.ai_source_rows {
            ai_src_table = ai_src_table.add_row(vec![row.source.clone(), row.sessions.clone(), row.share.clone()]);
        }
        b = b.add_component(ai_src_table);

        // AI traffic per page
        if !vm.ai_page_rows.is_empty() {
            b = b.add_component(Section::new("AI Traffic by Page").with_level(2));
            let mut ai_page_table = AuditTable::new(vec![
                TableColumn::new("Page").with_width("60%"),
                TableColumn::new("AI Sessions").with_width("20%"),
                TableColumn::new("Share").with_width("20%"),
            ]).with_title("Which content is being cited by AI tools?");
            for row in &vm.ai_page_rows {
                ai_page_table = ai_page_table.add_row(vec![row.url.clone(), row.sessions.clone(), row.share_pct.clone()]);
            }
            b = b.add_component(ai_page_table);
        }
    } else {
        b = b.add_component(
            Callout::info(
                "No measurable AI traffic (ChatGPT, Perplexity, Claude, Gemini...) in this period. \
                 Structured content and clear definitions increase AI visibility."
            ).with_title("No AI Traffic Yet")
        );
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 5. SEARCH CONSOLE
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Search Console").with_level(1));

    let mut sc_grid = Grid::new(2);
    sc_grid = add_metric(sc_grid, "Impressions",  &vm.search_impressions, "#0369a1");
    sc_grid = add_metric(sc_grid, "Clicks",       &vm.exec_clicks,        "#16a34a");
    sc_grid = add_metric(sc_grid, "CTR",          &vm.search_ctr,         "#d97706");
    sc_grid = add_metric(sc_grid, "Avg. Position",&vm.exec_avg_position,  "#7c3aed");
    b = b.add_component(sc_grid);

    if !vm.top_queries.is_empty() {
        let mut q_table = AuditTable::new(vec![
            TableColumn::new("Search Query").with_width("40%"),
            TableColumn::new("Clicks").with_width("15%"),
            TableColumn::new("Impr.").with_width("15%"),
            TableColumn::new("CTR").with_width("15%"),
            TableColumn::new("Position").with_width("15%"),
        ]).with_title("Top Search Queries");
        for q in &vm.top_queries {
            q_table = q_table.add_row(vec![q.query.clone(), q.clicks.clone(), q.impressions.clone(), q.ctr.clone(), q.position.clone()]);
        }
        b = b.add_component(q_table);
    }

    if !vm.opportunity_queries.is_empty() {
        b = b.add_component(
            Callout::info("Position 4-20, low CTR - better titles can generate more clicks here.")
                .with_title("CTR Potential")
        );
        let mut opp_q = AuditTable::new(vec![
            TableColumn::new("Search Query").with_width("40%"),
            TableColumn::new("Impr.").with_width("15%"),
            TableColumn::new("CTR").with_width("15%"),
            TableColumn::new("Position").with_width("15%"),
            TableColumn::new("Clicks").with_width("15%"),
        ]).with_title("Opportunity Queries");
        for q in &vm.opportunity_queries {
            opp_q = opp_q.add_row(vec![q.query.clone(), q.impressions.clone(), q.ctr.clone(), q.position.clone(), q.clicks.clone()]);
        }
        b = b.add_component(opp_q);
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 6. PAGE PERFORMANCE
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Page Performance").with_level(1));

    // ── 6a. All pages (sessions-sorted, up to limit) ─────────────────────────
    if !vm.all_pages.is_empty() {
        let title = format!("Top {} Pages by Sessions", vm.all_pages.len());
        let mut pages_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("26%"),
            TableColumn::new("Sessions").with_width("9%"),
            TableColumn::new("Organic").with_width("8%"),
            TableColumn::new("Bounce").with_width("8%"),
            TableColumn::new("Eng.").with_width("8%"),
            TableColumn::new("Impr.").with_width("8%"),
            TableColumn::new("Clicks").with_width("8%"),
            TableColumn::new("CTR").with_width("7%"),
            TableColumn::new("Pos.").with_width("7%"),
            TableColumn::new("Top Queries").with_width("11%"),
        ]).with_title(&title);
        for p in &vm.all_pages {
            pages_table = pages_table.add_row(vec![
                p.url.clone(),
                p.sessions.clone(),
                p.organic_share.clone(),
                p.bounce.clone(),
                p.engagement.clone(),
                p.impressions.clone(),
                p.clicks.clone(),
                p.ctr.clone(),
                p.position.clone(),
                p.queries.clone(),
            ]);
        }
        b = b.add_component(pages_table);
    }

    b = b.add_component(PageBreak::new());
    b = b.add_component(Section::new("Focused Analyses").with_level(2));

    // ── 6b. Top 20 Strengths ─────────────────────────────────────────────────
    if !vm.top_pages.is_empty() {
        b = b.add_component(
            Callout::info(
                "Pages ranking in position 1–10 with engagement ≥ 45%. \
                 These are your traffic assets — expand their internal linking and CTR."
            ).with_title("Top 20 Strengths: Pos 1–10, Good Engagement")
        );
        let mut pages_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("30%"),
            TableColumn::new("Sessions").with_width("10%"),
            TableColumn::new("Pos.").with_width("8%"),
            TableColumn::new("CTR").with_width("8%"),
            TableColumn::new("Engagement").with_width("10%"),
            TableColumn::new("Issue").with_width("14%"),
            TableColumn::new("Top Queries").with_width("20%"),
        ]).with_title("Strengths");
        for p in &vm.top_pages {
            pages_table = pages_table.add_row(vec![
                p.url.clone(),
                p.sessions.clone(),
                p.position.clone(),
                p.ctr.clone(),
                p.engagement.clone(),
                p.diagnosis.clone(),
                p.queries.clone(),
            ]);
        }
        b = b.add_component(pages_table);
    }

    // ── 6c. Top 20 Weaknesses ─────────────────────────────────────────────────
    if !vm.weakest_pages.is_empty() {
        b = b.add_component(
            Callout::warning(
                "Pages with high impressions but poor CTR, or high traffic but immediate bounce. \
                 Fix title, meta description, or content opening."
            ).with_title("Top 20 Weaknesses: Fix Title / Content")
        );
        let mut weak_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("28%"),
            TableColumn::new("Sessions").with_width("9%"),
            TableColumn::new("Bounce").with_width("8%"),
            TableColumn::new("Eng.").with_width("8%"),
            TableColumn::new("Impr.").with_width("9%"),
            TableColumn::new("CTR").with_width("8%"),
            TableColumn::new("Pos.").with_width("8%"),
            TableColumn::new("Issue").with_width("12%"),
            TableColumn::new("Queries").with_width("10%"),
        ]).with_title("Weaknesses");
        for p in &vm.weakest_pages {
            weak_table = weak_table.add_row(vec![
                p.url.clone(),
                p.sessions.clone(),
                p.bounce.clone(),
                p.engagement.clone(),
                p.impressions.clone(),
                p.ctr.clone(),
                p.position.clone(),
                p.diagnosis.clone(),
                p.queries.clone(),
            ]);
        }
        b = b.add_component(weak_table);
    }

    // ── 6d. Click-Gap pages ───────────────────────────────────────────────────
    if !vm.click_gap_pages.is_empty() {
        b = b.add_component(
            Callout::warning(
                "Position 4–15, CTR < 2%, Impressions > 100. \
                 Improving title and meta description can generate clicks immediately."
            ).with_title(&format!("Click-Gap: {} Pages with Pos 4–15 and CTR < 2%", vm.click_gap_pages.len()))
        );
        let mut gap_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("32%"),
            TableColumn::new("Impr.").with_width("10%"),
            TableColumn::new("CTR").with_width("9%"),
            TableColumn::new("Pos.").with_width("9%"),
            TableColumn::new("Clicks").with_width("9%"),
            TableColumn::new("Sessions").with_width("9%"),
            TableColumn::new("Top Queries").with_width("22%"),
        ]).with_title("Click-Gap Pages");
        for p in &vm.click_gap_pages {
            gap_table = gap_table.add_row(vec![
                p.url.clone(),
                p.impressions.clone(),
                p.ctr.clone(),
                p.position.clone(),
                p.clicks.clone(),
                p.sessions.clone(),
                p.queries.clone(),
            ]);
        }
        b = b.add_component(gap_table);
    }

    // ── 6e. Isolated articles ─────────────────────────────────────────────────
    if !vm.isolated_pages.is_empty() {
        b = b.add_component(
            Callout::warning(
                "High traffic, but no internal link click to any service page. \
                 Add a contextual ServiceHint or internal link."
            ).with_title("Top 10 Isolated Articles: Traffic Without Conversion Path")
        );
        let mut isolated_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("40%"),
            TableColumn::new("Sessions").with_width("12%"),
            TableColumn::new("Eng.").with_width("10%"),
            TableColumn::new("Clicks").with_width("10%"),
            TableColumn::new("Queries").with_width("28%"),
        ]).with_title("Isolated Articles");
        for p in &vm.isolated_pages {
            isolated_table = isolated_table.add_row(vec![
                p.url.clone(),
                p.sessions.clone(),
                p.engagement.clone(),
                p.clicks.clone(),
                p.queries.clone(),
            ]);
        }
        b = b.add_component(isolated_table);
    }

    // ── 6f. Invisible pages (traffic but no GSC impressions) ─────────────────
    if !vm.invisible_pages.is_empty() {
        b = b.add_component(
            Callout::info(
                "These pages receive traffic (direct/referral/social) but have zero impressions \
                 in Search Console — they are not ranking in Google at all. \
                 Check indexing status and consider improving content or internal linking."
            ).with_title(&format!("{} Pages: Traffic but Not Indexed / Not Ranking", vm.invisible_pages.len()))
        );
        let mut inv_table = AuditTable::new(vec![
            TableColumn::new("Page").with_width("45%"),
            TableColumn::new("Sessions").with_width("15%"),
            TableColumn::new("Organic").with_width("12%"),
            TableColumn::new("Bounce").with_width("12%"),
            TableColumn::new("Engagement").with_width("16%"),
        ]).with_title("Invisible in Search");
        for p in &vm.invisible_pages {
            inv_table = inv_table.add_row(vec![
                p.url.clone(),
                p.sessions.clone(),
                p.organic_share.clone(),
                p.bounce.clone(),
                p.engagement.clone(),
            ]);
        }
        b = b.add_component(inv_table);
    }

    // ── 6g. Top Page Diagnoses ────────────────────────────────────────────────
    if !vm.top_page_diagnoses.is_empty() {
        b = b.add_component(Section::new("Recommended Next Steps (Top 10 Strengths)").with_level(2));
        for page in &vm.top_page_diagnoses {
            b = b.add_component(
                Callout::info(format!(
                    "Top queries: {}\n\nNext step: {}",
                    page.queries, page.recommendation
                ))
                .with_title(format!("{} — {}", page.url, page.diagnosis))
            );
        }
    }

    b = b.add_component(PageBreak::new());

    // ════════════════════════════════════════════════════════════════════════
    // 7. ALL INSIGHTS
    // ════════════════════════════════════════════════════════════════════════

    b = b.add_component(Section::new("Diagnosis & Insights").with_level(1));
    if vm.insights.is_empty() {
        b = b.add_component(Callout::success("No further anomalies detected.").with_title("All Clear"));
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
    let bytes = engine.render_pdf(&built).context("PDF rendering failed")?;
    std::fs::write(output_path, bytes)
        .with_context(|| format!("Cannot write PDF: {}", output_path))?;
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
