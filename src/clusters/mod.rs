//! Topic Clustering
//!
//! Three strategies for grouping pages and queries into topic clusters:
//! A. URL-path-based (automatic) — first meaningful segment after /blog/, /docs/, etc.
//! B. Query-term-based (automatic) — most frequent terms from top queries
//! C. Manual (config) — user-defined cluster → term mappings

use std::collections::{HashMap, HashSet};

/// Assign a page URL to a cluster based on its path structure.
/// Returns `None` if no meaningful segment can be extracted.
pub fn cluster_from_url(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .map(|u| u.splitn(2, '/').nth(1).unwrap_or(u))
        .unwrap_or(url);

    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return None;
    }

    // Skip known top-level prefixes to get the topic segment
    const PREFIXES: &[&str] = &["blog", "docs", "artikel", "articles", "post", "posts", "guide", "guides", "tutorial", "tutorials", "wiki"];

    let topic_idx = if segments.len() > 1 && PREFIXES.contains(&segments[0].to_lowercase().as_str()) {
        1
    } else {
        0
    };

    segments.get(topic_idx).map(|s| {
        // Normalize: lowercase, trim numeric suffixes
        s.to_lowercase()
    })
}

/// Assign a query to a cluster using manual definitions.
/// Returns the first matching cluster name, or `None`.
pub fn cluster_from_manual(
    query: &str,
    manual_clusters: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let lower = query.to_lowercase();
    for (name, terms) in manual_clusters {
        for term in terms {
            if lower.contains(&term.to_lowercase()) {
                return Some(name.clone());
            }
        }
    }
    None
}

/// Extract topic terms from a query by simple tokenization.
/// Returns individual tokens (lowercased, deduplicated).
pub fn query_tokens(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| t.len() >= 3)
        .filter(|t| !STOP_WORDS.contains(t))
        .map(String::from)
        .collect()
}

/// Build automatic query-term clusters from a set of queries.
/// Returns a map: cluster_name → set of query strings belonging to it.
pub fn auto_cluster_queries(
    queries: &[(String, f64)], // (query, impressions)
    min_queries: usize,
) -> HashMap<String, HashSet<String>> {
    // Count term frequency weighted by impressions
    let mut term_freq: HashMap<String, f64> = HashMap::new();
    let mut term_queries: HashMap<String, HashSet<String>> = HashMap::new();

    for (query, impressions) in queries {
        let tokens = query_tokens(query);
        for token in &tokens {
            *term_freq.entry(token.clone()).or_default() += impressions;
            term_queries
                .entry(token.clone())
                .or_default()
                .insert(query.clone());
        }
    }

    // Keep terms that appear in at least `min_queries` queries
    let mut clusters: HashMap<String, HashSet<String>> = term_queries
        .into_iter()
        .filter(|(_, qs)| qs.len() >= min_queries)
        .collect();

    // Sort by frequency, keep top 20 clusters
    let mut sorted: Vec<(String, HashSet<String>)> = clusters.drain().collect();
    sorted.sort_by(|a, b| {
        let freq_a = term_freq.get(&a.0).copied().unwrap_or(0.0);
        let freq_b = term_freq.get(&b.0).copied().unwrap_or(0.0);
        freq_b.partial_cmp(&freq_a).unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(20);

    sorted.into_iter().collect()
}

/// Merge all clustering strategies into a unified assignment.
/// Returns: page_url → cluster_name, query → cluster_name
pub fn assign_clusters(
    page_urls: &[String],
    queries_with_impressions: &[(String, f64)],
    manual_clusters: &HashMap<String, Vec<String>>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut page_clusters: HashMap<String, String> = HashMap::new();
    let mut query_clusters: HashMap<String, String> = HashMap::new();

    // 1. Manual clusters first (highest priority)
    for url in page_urls {
        if let Some(cluster) = cluster_from_manual(url, manual_clusters) {
            page_clusters.insert(url.clone(), cluster);
        }
    }
    for (query, _) in queries_with_impressions {
        if let Some(cluster) = cluster_from_manual(query, manual_clusters) {
            query_clusters.insert(query.clone(), cluster);
        }
    }

    // 2. URL-path clustering for unassigned pages
    for url in page_urls {
        if !page_clusters.contains_key(url) {
            if let Some(cluster) = cluster_from_url(url) {
                page_clusters.insert(url.clone(), cluster);
            }
        }
    }

    // 3. Auto query-term clustering for unassigned queries
    let unassigned: Vec<(String, f64)> = queries_with_impressions
        .iter()
        .filter(|(q, _)| !query_clusters.contains_key(q))
        .cloned()
        .collect();

    let auto = auto_cluster_queries(&unassigned, 2);
    for (cluster_name, members) in &auto {
        for query in members {
            query_clusters
                .entry(query.clone())
                .or_insert_with(|| cluster_name.clone());
        }
    }

    (page_clusters, query_clusters)
}

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "are", "was", "were",
    "how", "what", "why", "who", "when", "where", "which",
    "der", "die", "das", "und", "ist", "mit", "von", "den", "dem", "ein",
    "eine", "fuer", "auf", "wie", "was", "nicht", "sich",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_cluster_blog_prefix() {
        assert_eq!(cluster_from_url("/blog/astro-components"), Some("astro-components".into()));
    }

    #[test]
    fn url_cluster_docs_prefix() {
        assert_eq!(cluster_from_url("/docs/setup/install"), Some("setup".into()));
    }

    #[test]
    fn url_cluster_no_prefix() {
        assert_eq!(cluster_from_url("/pricing"), Some("pricing".into()));
    }

    #[test]
    fn url_cluster_full_url() {
        assert_eq!(cluster_from_url("https://example.com/blog/claude-code"), Some("claude-code".into()));
    }

    #[test]
    fn url_cluster_root() {
        assert_eq!(cluster_from_url("/"), None);
    }

    #[test]
    fn manual_cluster_match() {
        let mut manual = HashMap::new();
        manual.insert("claude".into(), vec!["claude".into(), "claude-code".into()]);
        assert_eq!(cluster_from_manual("how to use claude code", &manual), Some("claude".into()));
    }

    #[test]
    fn manual_cluster_no_match() {
        let manual = HashMap::new();
        assert_eq!(cluster_from_manual("some random query", &manual), None);
    }

    #[test]
    fn auto_clustering_groups_queries() {
        let queries = vec![
            ("astro components tutorial".into(), 100.0),
            ("astro setup guide".into(), 80.0),
            ("astro deployment".into(), 60.0),
            ("react hooks tutorial".into(), 50.0),
        ];
        let clusters = auto_cluster_queries(&queries, 2);
        assert!(clusters.contains_key("astro"));
        assert!(clusters["astro"].len() >= 2);
    }

    #[test]
    fn query_tokens_filters_stopwords() {
        let tokens = query_tokens("how to use astro");
        assert!(tokens.contains(&"astro".to_string()));
        assert!(!tokens.contains(&"how".to_string()));
    }
}
