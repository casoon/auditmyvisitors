use crate::config::AppConfig;
use crate::domain::{PageSummary, SiteHealthReport, SitemapEntry, UrlInspectionResult};
use crate::errors::Result;
use crate::google::search_console;

pub async fn build(
    config: &AppConfig,
    access_token: &str,
    problem_pages: &[PageSummary],
) -> Result<SiteHealthReport> {
    let sc_url = config.require_search_console_url()?.to_string();
    let property_name = config
        .properties
        .ga4_property_name
        .clone()
        .unwrap_or_else(|| sc_url.clone());

    // Derive https base URL from sc-domain: or https: SC property URL
    let base_url = if sc_url.starts_with("sc-domain:") {
        format!("https://{}", sc_url.trim_start_matches("sc-domain:"))
    } else {
        sc_url.trim_end_matches('/').to_string()
    };

    // ── 1. Sitemaps ───────────────────────────────────────────────────────────
    let sitemap_data = search_console::list_sitemaps(access_token, &sc_url)
        .await
        .unwrap_or_default();

    let sitemaps: Vec<SitemapEntry> = sitemap_data
        .iter()
        .map(|s| SitemapEntry {
            url:            s.path.clone(),
            submitted_urls: s.submitted,
            indexed_urls:   s.indexed,
            warnings:       s.warnings,
            errors:         s.errors,
            last_submitted: s.last_submitted.clone(),
            is_pending:     s.is_pending,
        })
        .collect();

    let total_submitted: i64 = sitemaps.iter().map(|s| s.submitted_urls).sum();
    let total_indexed:   i64 = sitemaps.iter().map(|s| s.indexed_urls).sum();

    // ── 2. URL Inspection (top problem pages — sequential, rate-limit safe) ───
    let inspect_targets: Vec<String> = problem_pages
        .iter()
        .take(10)
        .map(|p| {
            if p.url.starts_with("http") {
                p.url.clone()
            } else {
                format!("{}{}", base_url.trim_end_matches('/'), p.url)
            }
        })
        .collect();

    let mut url_inspections: Vec<UrlInspectionResult> = Vec::new();
    for url in &inspect_targets {
        match search_console::inspect_url(access_token, &sc_url, url).await {
            Ok(data) => url_inspections.push(UrlInspectionResult {
                url:              data.url,
                verdict:          data.verdict,
                coverage_state:   data.coverage_state,
                robots_allowed:   data.robots_allowed,
                indexing_allowed: data.indexing_allowed,
                last_crawl:       data.last_crawl,
                mobile_verdict:   data.mobile_verdict,
                canonical_ok:     data.canonical_ok,
            }),
            Err(e) => tracing::warn!("URL inspection failed for {}: {}", url, e),
        }
    }

    Ok(SiteHealthReport {
        property_name,
        sitemaps,
        total_submitted,
        total_indexed,
        url_inspections,
    })
}
