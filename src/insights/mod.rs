use crate::config::ThresholdsConfig;
use crate::domain::{
    ComparisonReport, Insight, InsightCategory, InsightSeverity, PageDetailReport,
    SearchPerformanceBreakdown, SiteOverviewReport, TopPagesReport, TrafficSourceBreakdown,
};
use crate::opportunities::expected_ctr;

// ─── Public entry points ─────────────────────────────────────────────────────

pub fn insights_for_overview(report: &mut SiteOverviewReport, th: &ThresholdsConfig) {
    let mut insights: Vec<Insight> = generate_traffic_insights(&report.traffic, th)
        .chain(generate_search_insights(&report.search))
        .collect();

    // ── Engagement insight ───────────────────────────────────────────────────
    if report.engagement_rate < th.low_engagement_rate && report.traffic.total_sessions > th.min_sessions {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("Niedrige Engagement Rate ({:.0}%)", report.engagement_rate * 100.0),
            explanation: format!("Weniger als {:.0}% der Sitzungen gelten als engagiert. Landing-Pages und Ladezeiten pruefen.", th.low_engagement_rate * 100.0),
        });
    } else if report.engagement_rate > th.high_engagement_rate {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Engagement,
            headline: format!("Starke Engagement Rate ({:.0}%)", report.engagement_rate * 100.0),
            explanation: format!("Ueber {:.0}% der Sitzungen sind engagiert — gute Inhaltsqualitaet.", th.high_engagement_rate * 100.0),
        });
    }

    // ── AI traffic insight ───────────────────────────────────────────────────
    let ai_total: i64 = report.ai_sources.iter().map(|s| s.sessions).sum();
    if ai_total > 0 {
        let ai_pct = if report.traffic.total_sessions > 0 {
            ai_total as f64 / report.traffic.total_sessions as f64 * 100.0
        } else { 0.0 };
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Traffic,
            headline: format!("AI-Referral-Traffic: {} Sessions ({:.1}%)", ai_total, ai_pct),
            explanation: format!(
                "Traffic von AI-Tools wie {}. Inhalte werden in AI-Antworten referenziert.",
                report.ai_sources.iter().take(3).map(|s| s.source.as_str()).collect::<Vec<_>>().join(", ")
            ),
        });
    }

    // ── Trend insights ───────────────────────────────────────────────────────
    if let Some(trend) = &report.trend {
        if trend.sessions_pct <= -th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Critical,
                category: InsightCategory::Trend,
                headline: format!("Starker Traffic-Rueckgang ({:+.1}%)", trend.sessions_pct),
                explanation: format!("Sessions sind im Vergleich zum Vorzeitraum um mehr als {:.0}% gefallen. Ursachen pruefen.", th.trend_significant_pct),
            });
        } else if trend.sessions_pct >= th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Positive,
                category: InsightCategory::Trend,
                headline: format!("Starkes Traffic-Wachstum ({:+.1}%)", trend.sessions_pct),
                explanation: format!("Sessions sind im Vergleich zum Vorzeitraum um mehr als {:.0}% gestiegen.", th.trend_significant_pct),
            });
        }

        if trend.clicks_pct <= -th.trend_significant_pct {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Trend,
                headline: format!("Such-Klicks ruecklaeufig ({:+.1}%)", trend.clicks_pct),
                explanation: "Klicks aus der Suche sind deutlich gefallen — Ranking-Veraenderungen pruefen.".into(),
            });
        }
    }

    report.insights = insights;
}

pub fn insights_for_page(report: &mut PageDetailReport, th: &ThresholdsConfig) {
    report.insights = generate_traffic_insights(&report.traffic, th)
        .chain(generate_search_insights(&report.search))
        .collect();
}

pub fn insights_for_top_pages(report: &mut TopPagesReport, th: &ThresholdsConfig) {
    let mut insights = Vec::new();
    let pages = &report.pages;

    if pages.is_empty() {
        report.insights = insights;
        return;
    }

    // ── Dependency: top page concentration ───────────────────────────────────
    let total_sessions: i64 = pages.iter().map(|p| p.sessions).sum();
    if total_sessions > 0 && !pages.is_empty() {
        let top3_sessions: i64 = pages.iter().take(3).map(|p| p.sessions).sum();
        let top3_share = top3_sessions as f64 / total_sessions as f64 * 100.0;
        if top3_share > th.top3_dependency_pct {
            insights.push(Insight {
                severity: InsightSeverity::Warning,
                category: InsightCategory::Traffic,
                headline: format!("Hohe Abhaengigkeit: Top 3 Seiten = {:.0}% des Traffics", top3_share),
                explanation: "Wenige Seiten tragen den Grossteil des Traffics. Ein Rankingverlust auf einer dieser Seiten haette starke Auswirkungen.".into(),
            });
        }
    }

    // ── Pages with CTR opportunity ───────────────────────────────────────────
    let ctr_opp_pages: Vec<&str> = pages.iter()
        .filter(|p| {
            p.search.impressions > 200.0
                && p.search.average_position > 0.0
                && p.search.average_position <= 10.0
                && p.search.ctr < expected_ctr(p.search.average_position) * 0.7
        })
        .map(|p| p.url.as_str())
        .collect();
    if !ctr_opp_pages.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} Seiten mit CTR unter Erwartung", ctr_opp_pages.len()),
            explanation: format!(
                "Diese Seiten ranken auf Seite 1, aber die CTR ist zu niedrig. Snippet-Optimierung pruefen, z.B. {}",
                ctr_opp_pages.first().unwrap_or(&"-")
            ),
        });
    }

    // ── Pages with high impressions but low clicks ───────────────────────────
    let high_impr_low_click: Vec<&str> = pages.iter()
        .filter(|p| p.search.impressions > 500.0 && p.search.clicks < 10.0)
        .map(|p| p.url.as_str())
        .collect();
    if !high_impr_low_click.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("{} Seiten: hohe Sichtbarkeit, kaum Klicks", high_impr_low_click.len()),
            explanation: "Diese Seiten haben starke Impressionen, generieren aber kaum Klicks. Suchintention und Snippet pruefen.".into(),
        });
    }

    // ── Low engagement pages ─────────────────────────────────────────────────
    let low_engagement: Vec<&str> = pages.iter()
        .filter(|p| p.sessions >= 20 && p.engagement_rate < 0.3)
        .map(|p| p.url.as_str())
        .collect();
    if !low_engagement.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Engagement,
            headline: format!("{} Seiten mit schwacher Engagement Rate (<30%)", low_engagement.len()),
            explanation: "Niedrige Engagement Rate kann auf Landing-Page-Probleme hindeuten: langsame Ladezeit, irrelevanter Content oder schlechte UX.".into(),
        });
    }

    // ── SEO winners (high organic share) ─────────────────────────────────────
    let seo_winners: Vec<&str> = pages.iter()
        .filter(|p| {
            p.sessions >= 20
                && (p.organic_sessions as f64 / p.sessions as f64) > 0.7
        })
        .map(|p| p.url.as_str())
        .collect();
    if seo_winners.len() >= 3 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("{} Seiten mit starkem organischem Traffic (>70%)", seo_winners.len()),
            explanation: "Diese Seiten ziehen effektiv organischen Traffic an — gutes SEO-Fundament.".into(),
        });
    }

    // ── Internal linking opportunities ────────────────────────────────────────
    let link_opps: Vec<&str> = pages.iter()
        .filter(|p| {
            p.search.average_position > 0.0
                && p.search.average_position < 10.0
                && p.search.impressions > 50.0
                && p.sessions < 10
        })
        .map(|p| p.url.as_str())
        .collect();
    if !link_opps.is_empty() {
        insights.push(Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Search,
            headline: format!("{} Seiten: gute Suchposition, aber wenig Sessions", link_opps.len()),
            explanation: "Diese Seiten ranken gut in der Suche, bekommen aber wenig Traffic. Interne Verlinkung oder Landing-Page-Qualitaet pruefen.".into(),
        });
    }

    report.insights = insights;
}

pub fn insights_for_comparison(report: &mut ComparisonReport) {
    let mut insights = Vec::new();
    let d = &report.delta;

    // ── Sessions ─────────────────────────────────────────────────────────────
    if d.sessions_pct >= 10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Sessions gestiegen ({:+.1}%)", d.sessions_pct),
            explanation: format!(
                "Im Zeitraum nach {} sind die Sitzungen um {:.1}% gestiegen ({:+} absolut).",
                report.change_date, d.sessions_pct, d.sessions_abs
            ),
        });
    } else if d.sessions_pct <= -10.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("Sessions gesunken ({:+.1}%)", d.sessions_pct),
            explanation: format!(
                "Im Zeitraum nach {} sind die Sitzungen um {:.1}% gesunken ({:+} absolut).",
                report.change_date, d.sessions_pct, d.sessions_abs
            ),
        });
    }

    // ── Organic sessions ─────────────────────────────────────────────────────
    if d.organic_sessions_pct >= 15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Traffic,
            headline: format!("Organischer Traffic gestiegen ({:+.1}%)", d.organic_sessions_pct),
            explanation: "Organische Sessions haben sich positiv entwickelt.".into(),
        });
    } else if d.organic_sessions_pct <= -15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Traffic,
            headline: format!("Organischer Traffic gesunken ({:+.1}%)", d.organic_sessions_pct),
            explanation: "Rueckgang im organischen Traffic — pruefen, ob Ranking-Aenderungen vorliegen.".into(),
        });
    }

    // ── Search clicks ────────────────────────────────────────────────────────
    if d.clicks_pct >= 15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Klicks aus Suche gestiegen ({:+.1}%)", d.clicks_pct),
            explanation: "Aenderung korreliert mit positivem Suchtrend.".into(),
        });
    } else if d.clicks_pct <= -15.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Klicks aus Suche gesunken ({:+.1}%)", d.clicks_pct),
            explanation: "Suchsichtbarkeit nach der Aenderung gesunken.".into(),
        });
    }

    // ── Impressions ──────────────────────────────────────────────────────────
    if d.impressions_pct >= 20.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Impressionen gestiegen ({:+.1}%)", d.impressions_pct),
            explanation: "Hoehere Sichtbarkeit in der Suche nach der Aenderung.".into(),
        });
    } else if d.impressions_pct <= -20.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Impressionen gesunken ({:+.1}%)", d.impressions_pct),
            explanation: "Deutlich weniger Sucheinblendungen — moeglicherweise Ranking-Verlust.".into(),
        });
    }

    // ── CTR change ───────────────────────────────────────────────────────────
    let ctr_pp = d.ctr_abs * 100.0; // percentage points
    if ctr_pp >= 1.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("CTR verbessert ({:+.1} Prozentpunkte)", ctr_pp),
            explanation: "Snippets werden besser geklickt — Title/Description-Aenderungen wirken.".into(),
        });
    } else if ctr_pp <= -1.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("CTR verschlechtert ({:+.1} Prozentpunkte)", ctr_pp),
            explanation: "Klickrate ist gefallen — pruefen, ob Snippets oder Suchergebnisformat sich geaendert haben.".into(),
        });
    }

    // ── Position change ──────────────────────────────────────────────────────
    if d.position_abs <= -2.0 {
        insights.push(Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Search,
            headline: format!("Ranking verbessert ({:+.1} Positionen)", d.position_abs),
            explanation: "Durchschnittsposition hat sich deutlich verbessert.".into(),
        });
    } else if d.position_abs >= 2.0 {
        insights.push(Insight {
            severity: InsightSeverity::Warning,
            category: InsightCategory::Search,
            headline: format!("Ranking verschlechtert ({:+.1} Positionen)", d.position_abs),
            explanation: "Durchschnittsposition ist gefallen — Ursache pruefen.".into(),
        });
    }

    // ── Overall verdict ──────────────────────────────────────────────────────
    let positive_count = insights.iter()
        .filter(|i| i.severity == InsightSeverity::Positive).count();
    let warning_count = insights.iter()
        .filter(|i| i.severity == InsightSeverity::Warning).count();

    if positive_count > 0 && warning_count == 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Positive,
            category: InsightCategory::Trend,
            headline: "Positive Entwicklung nach Stichtag".into(),
            explanation: format!(
                "Alle wesentlichen Metriken haben sich nach dem {} positiv entwickelt.",
                report.change_date
            ),
        });
    } else if warning_count > 0 && positive_count == 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Critical,
            category: InsightCategory::Trend,
            headline: "Negative Entwicklung nach Stichtag".into(),
            explanation: format!(
                "Mehrere Metriken zeigen eine Verschlechterung nach dem {}. Aenderungen pruefen.",
                report.change_date
            ),
        });
    } else if positive_count > 0 && warning_count > 0 {
        insights.insert(0, Insight {
            severity: InsightSeverity::Info,
            category: InsightCategory::Trend,
            headline: "Gemischte Entwicklung nach Stichtag".into(),
            explanation: format!(
                "Einige Metriken haben sich nach dem {} verbessert, andere verschlechtert.",
                report.change_date
            ),
        });
    }

    report.insights = insights;
}

// ─── Rule implementations ────────────────────────────────────────────────────

fn generate_traffic_insights(
    traffic: &TrafficSourceBreakdown,
    th: &ThresholdsConfig,
) -> impl Iterator<Item = Insight> {
    let mut insights = Vec::new();

    let organic_share = traffic.organic_share();
    let direct_share = traffic.direct_share();

    if organic_share < th.low_organic_pct && traffic.total_sessions > th.min_sessions {
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

    if direct_share > th.high_direct_pct {
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

    if organic_share > th.high_organic_pct {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;

    fn make_traffic(organic: i64, direct: i64, referral: i64) -> TrafficSourceBreakdown {
        TrafficSourceBreakdown {
            organic_sessions: organic,
            direct_sessions: direct,
            referral_sessions: referral,
            other_sessions: 0,
            total_sessions: organic + direct + referral,
        }
    }

    fn make_search(clicks: f64, impressions: f64, ctr: f64, position: f64) -> SearchPerformanceBreakdown {
        SearchPerformanceBreakdown {
            clicks, impressions, ctr, average_position: position,
            top_queries: vec![],
        }
    }

    #[test]
    fn low_organic_triggers_warning() {
        let traffic = make_traffic(10, 80, 20);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Warning && i.headline.contains("organisch")));
    }

    #[test]
    fn high_organic_triggers_positive() {
        let traffic = make_traffic(80, 10, 10);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Positive));
    }

    #[test]
    fn high_direct_triggers_info() {
        let traffic = make_traffic(10, 70, 5);
        let th = ThresholdsConfig::default();
        let insights: Vec<_> = generate_traffic_insights(&traffic, &th).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Info && i.headline.contains("direkt")));
    }

    #[test]
    fn high_impressions_low_ctr_warning() {
        let search = make_search(5.0, 1000.0, 0.005, 8.0);
        let insights: Vec<_> = generate_search_insights(&search).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Warning && i.headline.contains("CTR")));
    }

    #[test]
    fn good_search_performance_positive() {
        let search = make_search(200.0, 2000.0, 0.10, 3.0);
        let insights: Vec<_> = generate_search_insights(&search).collect();
        assert!(insights.iter().any(|i| i.severity == InsightSeverity::Positive));
    }

    #[test]
    fn comparison_positive_verdict() {
        let mut report = ComparisonReport {
            url: None,
            property_name: "test".into(),
            change_date: "2026-01-01".into(),
            before_days: 30,
            after_days: 30,
            before: ComparisonPeriod {
                start_date: "2025-12-01".into(),
                end_date: "2025-12-31".into(),
                sessions: 100,
                organic_sessions: 50,
                engagement_rate: 0.5,
                search: make_search(50.0, 1000.0, 0.05, 5.0),
            },
            after: ComparisonPeriod {
                start_date: "2026-01-01".into(),
                end_date: "2026-01-30".into(),
                sessions: 150,
                organic_sessions: 80,
                engagement_rate: 0.6,
                search: make_search(80.0, 1500.0, 0.053, 4.0),
            },
            delta: ComparisonDelta {
                sessions_abs: 50,
                sessions_pct: 50.0,
                organic_sessions_abs: 30,
                organic_sessions_pct: 60.0,
                engagement_rate_abs: 0.1,
                clicks_abs: 30.0,
                clicks_pct: 60.0,
                impressions_abs: 500.0,
                impressions_pct: 50.0,
                ctr_abs: 0.003,
                position_abs: -1.0,
            },
            summary: String::new(),
            insights: vec![],
        };

        insights_for_comparison(&mut report);
        assert!(!report.insights.is_empty());
        assert_eq!(report.insights[0].severity, InsightSeverity::Positive);
        assert!(report.insights[0].headline.contains("Positive"));
    }

    #[test]
    fn comparison_negative_verdict() {
        let mut report = ComparisonReport {
            url: None,
            property_name: "test".into(),
            change_date: "2026-01-01".into(),
            before_days: 30,
            after_days: 30,
            before: ComparisonPeriod::default(),
            after: ComparisonPeriod::default(),
            delta: ComparisonDelta {
                sessions_abs: -50,
                sessions_pct: -30.0,
                organic_sessions_abs: -20,
                organic_sessions_pct: -25.0,
                clicks_abs: -30.0,
                clicks_pct: -40.0,
                impressions_abs: -500.0,
                impressions_pct: -30.0,
                ctr_abs: -0.02,
                position_abs: 3.0,
                ..Default::default()
            },
            summary: String::new(),
            insights: vec![],
        };

        insights_for_comparison(&mut report);
        assert!(!report.insights.is_empty());
        assert_eq!(report.insights[0].severity, InsightSeverity::Critical);
        assert!(report.insights[0].headline.contains("Negative"));
    }

    #[test]
    fn top_pages_dependency_warning() {
        let mut report = TopPagesReport {
            property_name: "test".into(),
            date_range: "test".into(),
            pages: vec![
                PageSummary {
                    url: "/page1".into(), sessions: 700, organic_sessions: 500,
                    direct_sessions: 200, engagement_rate: 0.5,
                    avg_session_duration_secs: 60.0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page2".into(), sessions: 200, organic_sessions: 100,
                    direct_sessions: 100, engagement_rate: 0.5,
                    avg_session_duration_secs: 60.0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page3".into(), sessions: 50, organic_sessions: 25,
                    direct_sessions: 25, engagement_rate: 0.5,
                    avg_session_duration_secs: 60.0,
                    search: SearchPerformanceBreakdown::default(),
                },
                PageSummary {
                    url: "/page4".into(), sessions: 50, organic_sessions: 25,
                    direct_sessions: 25, engagement_rate: 0.5,
                    avg_session_duration_secs: 60.0,
                    search: SearchPerformanceBreakdown::default(),
                },
            ],
            insights: vec![],
        };

        let th = ThresholdsConfig::default();
        insights_for_top_pages(&mut report, &th);
        assert!(report.insights.iter().any(|i|
            i.severity == InsightSeverity::Warning && i.headline.contains("Abhaengigkeit")
        ));
    }
}
