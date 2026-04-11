use crate::domain::{
    ComparisonReport, Insight, InsightCategory, InsightSeverity, PageDetailReport,
    SearchPerformanceBreakdown, SiteOverviewReport, TopPagesReport, TrafficSourceBreakdown,
};

// ─── Public entry points ─────────────────────────────────────────────────────

pub fn insights_for_overview(report: &mut SiteOverviewReport) {
    report.insights = generate_traffic_insights(&report.traffic)
        .chain(generate_search_insights(&report.search))
        .collect();
}

pub fn insights_for_page(report: &mut PageDetailReport) {
    report.insights = generate_traffic_insights(&report.traffic)
        .chain(generate_search_insights(&report.search))
        .collect();
}

pub fn insights_for_top_pages(report: &mut TopPagesReport) {
    // Aggregate-level insights on the overview traffic/search fields would go here.
    // Individual page insights can be generated per-row if needed.
    report.insights = vec![];
}

pub fn insights_for_comparison(report: &mut ComparisonReport) {
    let mut insights = Vec::new();

    let sessions_pct = report.delta.sessions_pct;
    if sessions_pct >= 10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Sessions gestiegen ({:+.1}%)", sessions_pct),
            explanation: format!(
                "Im Zeitraum nach {} sind die Sitzungen um {:.1}% gestiegen.",
                report.change_date, sessions_pct
            ),
        });
    } else if sessions_pct <= -10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("Sessions gesunken ({:+.1}%)", sessions_pct),
            explanation: format!(
                "Im Zeitraum nach {} sind die Sitzungen um {:.1}% gesunken.",
                report.change_date, sessions_pct
            ),
        });
    }

    let clicks_pct = report.delta.clicks_pct;
    if clicks_pct >= 15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Klicks aus Suche gestiegen ({:+.1}%)", clicks_pct),
            explanation: "Technische Änderung korreliert mit positivem Suchtrend.".into(),
        });
    } else if clicks_pct <= -15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Klicks aus Suche gesunken ({:+.1}%)", clicks_pct),
            explanation: "Suchsichtbarkeit nach der Änderung gesunken.".into(),
        });
    }

    report.insights = insights;
}

// ─── Rule implementations ────────────────────────────────────────────────────

fn generate_traffic_insights(
    traffic: &TrafficSourceBreakdown,
) -> impl Iterator<Item = Insight> + '_ {
    let mut insights = Vec::new();

    let organic_share = traffic.organic_share();
    let direct_share = traffic.direct_share();

    if organic_share < 20.0 && traffic.total_sessions > 100 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: "Wenig organischer Traffic".into(),
            explanation: format!(
                "Nur {:.0}% der Sitzungen kommen aus organischer Suche. \
                Das deutet auf SEO-Potenzial hin.",
                organic_share
            ),
        });
    }

    if direct_share > 60.0 {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: "Überdurchschnittlich viel direkter Traffic".into(),
            explanation: format!(
                "{:.0}% der Sitzungen kommen direkt. \
                Das kann auf starke Markenbekanntheit oder fehlende UTM-Tracking-Parameter hinweisen.",
                direct_share
            ),
        });
    }

    if organic_share > 70.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: "Starker organischer Traffic".into(),
            explanation: format!(
                "{:.0}% des Traffics kommt aus organischer Suche — gutes SEO-Fundament.",
                organic_share
            ),
        });
    }

    insights.into_iter()
}

fn generate_search_insights(
    search: &SearchPerformanceBreakdown,
) -> impl Iterator<Item = Insight> + '_ {
    let mut insights = Vec::new();

    // High impressions, low CTR
    if search.impressions > 500.0 && search.ctr < 0.02 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: "Hohe Impressionen, aber niedrige CTR".into(),
            explanation: format!(
                "{:.0} Impressionen bei nur {:.1}% CTR. \
                Title und Meta Description könnten ansprechender formuliert werden.",
                search.impressions,
                search.ctr * 100.0
            ),
        });
    }

    // Good position but few clicks
    if search.average_position < 5.0 && search.clicks < 10.0 && search.impressions > 100.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: "Gute Ranking-Position, aber wenig Klicks".into(),
            explanation: format!(
                "Durchschnittsposition {:.1} — aber nur {:.0} Klicks. \
                Der Snippet könnte den Suchintent besser treffen.",
                search.average_position, search.clicks
            ),
        });
    }

    // Solid search performance
    if search.ctr > 0.05 && search.clicks > 100.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: "Gute Suchperformance".into(),
            explanation: format!(
                "{:.1}% CTR bei {:.0} Klicks — Seite zieht organischen Traffic effektiv an.",
                search.ctr * 100.0, search.clicks
            ),
        });
    }

    insights.into_iter()
}
