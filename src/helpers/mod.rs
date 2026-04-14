//! Shared helper functions used across reports, insights, and exports.

use std::collections::HashMap;
use crate::domain::{PageSummary, SearchPerformanceBreakdown};

// ─── Date helpers ────────────────────────────────────────────────────────────

pub fn days_ago(days: u32) -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(days as i64);
    date.format("%Y-%m-%d").to_string()
}

pub fn yesterday() -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(1);
    date.format("%Y-%m-%d").to_string()
}

// ─── Metric helpers ──────────────────────────────────────────────────────────

/// Percentage change between two values. Returns 0.0 if `prev` is zero.
pub fn pct_change(prev: f64, curr: f64) -> f64 {
    if prev == 0.0 { return 0.0; }
    (curr - prev) / prev * 100.0
}

// ─── URL helpers ─────────────────────────────────────────────────────────────

/// Extract the path component from a URL string.
/// Falls back to the input if parsing fails.
pub fn extract_path(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string())
}

/// Match a full Search Console URL to a GA4 page path in the given map.
/// Tries direct key match first, then falls back to path-only match.
pub fn match_sc_url_to_path<V>(page_url: &str, page_map: &HashMap<String, V>) -> Option<String> {
    if page_map.contains_key(page_url) {
        return Some(page_url.to_string());
    }
    if let Ok(parsed) = url::Url::parse(page_url) {
        let path = parsed.path().to_string();
        if page_map.contains_key(&path) {
            return Some(path);
        }
        // Try with trailing slash stripped
        let trimmed = path.trim_end_matches('/');
        if trimmed != path && page_map.contains_key(trimmed) {
            return Some(trimmed.to_string());
        }
        // Try with trailing slash added
        let with_slash = format!("{}/", trimmed);
        if page_map.contains_key(&with_slash) {
            return Some(with_slash);
        }
    }
    None
}

/// Merge Search Console page-level data into a GA4 page map.
pub fn merge_sc_into_page_map(
    sc_rows: &[crate::google::search_console::SearchAnalyticsRow],
    page_map: &mut HashMap<String, PageSummary>,
) {
    for row in sc_rows {
        let page_url = row.keys.first().cloned().unwrap_or_default();
        if let Some(key) = match_sc_url_to_path(&page_url, page_map) {
            if let Some(entry) = page_map.get_mut(&key) {
                entry.search = SearchPerformanceBreakdown {
                    clicks: row.clicks,
                    impressions: row.impressions,
                    ctr: row.ctr,
                    average_position: row.position,
                    top_queries: vec![],
                };
            }
        }
    }
}

// ─── Brand classification ───────────────────────────────────────────────────

/// Classify a query as brand or non-brand.
/// A query is "brand" if any brand term appears as a substring (case-insensitive).
pub fn is_brand_query(query: &str, brand_terms: &[String]) -> bool {
    if brand_terms.is_empty() {
        return false;
    }
    let q = query.to_lowercase();
    brand_terms.iter().any(|term| q.contains(&term.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_change_positive() {
        assert!((pct_change(100.0, 120.0) - 20.0).abs() < 0.001);
    }

    #[test]
    fn pct_change_negative() {
        assert!((pct_change(100.0, 80.0) - (-20.0)).abs() < 0.001);
    }

    #[test]
    fn pct_change_zero_base() {
        assert_eq!(pct_change(0.0, 50.0), 0.0);
    }

    #[test]
    fn pct_change_no_change() {
        assert!((pct_change(100.0, 100.0)).abs() < 0.001);
    }

    #[test]
    fn extract_path_from_full_url() {
        assert_eq!(extract_path("https://example.com/blog/post"), "/blog/post");
    }

    #[test]
    fn extract_path_from_path() {
        assert_eq!(extract_path("/blog/post"), "/blog/post");
    }

    #[test]
    fn url_matching_direct() {
        let mut map = HashMap::new();
        map.insert("https://example.com/page".into(), 1);
        assert_eq!(
            match_sc_url_to_path("https://example.com/page", &map),
            Some("https://example.com/page".into())
        );
    }

    #[test]
    fn url_matching_path_fallback() {
        let mut map = HashMap::new();
        map.insert("/page".into(), 1);
        assert_eq!(
            match_sc_url_to_path("https://example.com/page", &map),
            Some("/page".into())
        );
    }

    #[test]
    fn url_matching_trailing_slash() {
        let mut map = HashMap::new();
        map.insert("/page".into(), 1);
        assert_eq!(
            match_sc_url_to_path("https://example.com/page/", &map),
            Some("/page".into())
        );
    }

    #[test]
    fn brand_query_match() {
        let terms = vec!["mycompany".into(), "my brand".into()];
        assert!(is_brand_query("mycompany login", &terms));
        assert!(is_brand_query("how to use MyCompany", &terms));
        assert!(is_brand_query("my brand reviews", &terms));
        assert!(!is_brand_query("generic search term", &terms));
    }

    #[test]
    fn brand_query_empty_terms() {
        assert!(!is_brand_query("anything", &[]));
    }

    #[test]
    fn url_matching_no_match() {
        let mut map = HashMap::new();
        map.insert("/other".into(), 1);
        assert_eq!(
            match_sc_url_to_path("https://example.com/page", &map),
            None
        );
    }
}
