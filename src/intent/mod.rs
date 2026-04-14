//! Search Intent Classification
//!
//! Heuristic classification of search queries into four intent classes:
//! - Informational: user wants to learn/understand
//! - Navigational: user wants to reach a specific site/tool
//! - Commercial: user is comparing/evaluating options
//! - Transactional: user wants to take action (buy, download, sign up)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    Informational,
    Navigational,
    Commercial,
    Transactional,
}

#[allow(dead_code)]
impl Intent {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Informational => "Informational",
            Self::Navigational  => "Navigational",
            Self::Commercial    => "Commercial",
            Self::Transactional => "Transactional",
        }
    }

    pub fn label_de(&self) -> &'static str {
        match self {
            Self::Informational => "Informational",
            Self::Navigational  => "Navigational",
            Self::Commercial    => "Comparison",
            Self::Transactional => "Transaction",
        }
    }
}

// ─── Pattern lists ──────────────────────────────────────────────────────────

const TRANSACTIONAL_PATTERNS: &[&str] = &[
    "kaufen", "bestellen", "preis", "kosten", "kostenlos", "gratis", "free",
    "download", "herunterladen", "anmelden", "registrieren", "signup", "sign up",
    "buchen", "mieten", "abo", "subscription", "pricing", "buy", "order",
    "gutschein", "coupon", "rabatt", "discount", "trial",
];

const COMMERCIAL_PATTERNS: &[&str] = &[
    "vergleich", "vergleichen", "vs", "versus", "oder", "alternative",
    "alternativen", "test", "testbericht", "review", "reviews", "erfahrung",
    "erfahrungen", "bewertung", "bewertungen", "beste", "bester", "bestes",
    "top", "empfehlung", "empfehlungen", "ranking", "comparison", "compare",
    "which", "welche", "welcher", "welches", "lohnt sich",
];

const INFORMATIONAL_PATTERNS: &[&str] = &[
    "was ist", "what is", "wie", "how", "warum", "why", "wann", "when",
    "wo ", "where", "wer ", "who", "tutorial", "anleitung", "guide",
    "erklaerung", "erklaert", "definition", "bedeutung", "beispiel",
    "beispiele", "einfuehrung", "lernen", "learn", "grundlagen", "basics",
    "tipps", "tips", "tricks", "uebersicht", "overview", "zusammenfassung",
    "unterschied zwischen", "difference between",
];

/// Classify a search query into an intent category.
///
/// Priority: transactional > commercial > informational > navigational (fallback).
/// Brand queries are classified as navigational regardless of other patterns.
pub fn classify(query: &str, brand_terms: &[String]) -> Intent {
    let q = query.to_lowercase();

    // Brand queries are navigational
    if !brand_terms.is_empty() && brand_terms.iter().any(|t| q.contains(&t.to_lowercase())) {
        return Intent::Navigational;
    }

    // Transactional has highest priority
    if TRANSACTIONAL_PATTERNS.iter().any(|p| q.contains(p)) {
        return Intent::Transactional;
    }

    // Commercial
    if COMMERCIAL_PATTERNS.iter().any(|p| q.contains(p)) {
        return Intent::Commercial;
    }

    // Informational
    if INFORMATIONAL_PATTERNS.iter().any(|p| q.contains(p)) {
        return Intent::Informational;
    }

    // Default: informational (most organic queries are info-seeking)
    Intent::Informational
}

/// Aggregate intent distribution from a set of classified queries.
pub fn distribution(intents: &[Intent]) -> IntentDistribution {
    let total = intents.len() as f64;
    if total == 0.0 {
        return IntentDistribution::default();
    }

    let count = |i: Intent| intents.iter().filter(|&&x| x == i).count() as f64;

    IntentDistribution {
        informational_pct: count(Intent::Informational) / total * 100.0,
        navigational_pct: count(Intent::Navigational) / total * 100.0,
        commercial_pct: count(Intent::Commercial) / total * 100.0,
        transactional_pct: count(Intent::Transactional) / total * 100.0,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentDistribution {
    pub informational_pct: f64,
    pub navigational_pct: f64,
    pub commercial_pct: f64,
    pub transactional_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_informational() {
        assert_eq!(classify("was ist rust", &[]), Intent::Informational);
        assert_eq!(classify("wie funktioniert oauth", &[]), Intent::Informational);
        assert_eq!(classify("tutorial astro js", &[]), Intent::Informational);
        assert_eq!(classify("grundlagen typescript", &[]), Intent::Informational);
    }

    #[test]
    fn classify_commercial() {
        assert_eq!(classify("astro vs next.js", &[]), Intent::Commercial);
        assert_eq!(classify("beste cms 2024", &[]), Intent::Commercial);
        assert_eq!(classify("vergleich hosting anbieter", &[]), Intent::Commercial);
        assert_eq!(classify("erfahrungen mit netlify", &[]), Intent::Commercial);
    }

    #[test]
    fn classify_transactional() {
        assert_eq!(classify("hosting kaufen", &[]), Intent::Transactional);
        assert_eq!(classify("netlify pricing", &[]), Intent::Transactional);
        assert_eq!(classify("tool kostenlos download", &[]), Intent::Transactional);
        assert_eq!(classify("anmelden github", &[]), Intent::Transactional);
    }

    #[test]
    fn classify_navigational_brand() {
        let brands = vec!["casoon".into()];
        assert_eq!(classify("casoon login", &brands), Intent::Navigational);
        assert_eq!(classify("Casoon CMS", &brands), Intent::Navigational);
    }

    #[test]
    fn classify_default_informational() {
        // Ambiguous queries default to informational
        assert_eq!(classify("rust ownership", &[]), Intent::Informational);
        assert_eq!(classify("claude md", &[]), Intent::Informational);
    }

    #[test]
    fn transactional_beats_informational() {
        // "wie kostenlos download" has both informational and transactional signals
        assert_eq!(classify("wie kostenlos download", &[]), Intent::Transactional);
    }

    #[test]
    fn distribution_calculation() {
        let intents = vec![
            Intent::Informational,
            Intent::Informational,
            Intent::Commercial,
            Intent::Transactional,
        ];
        let dist = distribution(&intents);
        assert!((dist.informational_pct - 50.0).abs() < 0.1);
        assert!((dist.commercial_pct - 25.0).abs() < 0.1);
        assert!((dist.transactional_pct - 25.0).abs() < 0.1);
        assert!((dist.navigational_pct - 0.0).abs() < 0.1);
    }

    #[test]
    fn distribution_empty() {
        let dist = distribution(&[]);
        assert_eq!(dist.informational_pct, 0.0);
    }
}
