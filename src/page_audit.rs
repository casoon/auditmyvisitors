use crate::domain::PageSummary;
use crate::opportunities::expected_ctr;

pub fn strength_score(page: &PageSummary, tracking_enabled: bool) -> Option<f64> {
    if page.search.average_position <= 0.0 || page.search.average_position > 10.0 {
        return None;
    }
    if page.engagement_rate < 0.45 {
        return None;
    }
    if page.search.impressions < 100.0 && page.sessions < 30 {
        return None;
    }

    let mut score = page.search.impressions / 20.0;
    score += page.sessions as f64 / 10.0;
    score += (11.0 - page.search.average_position).max(0.0) * 6.0;
    score += page.engagement_rate * 40.0;

    let exp_ctr = expected_ctr(page.search.average_position);
    if exp_ctr > page.search.ctr {
        score += (exp_ctr - page.search.ctr) * page.search.impressions;
    }

    if tracking_enabled && page.service_hint_clicks == 0 && page.sessions >= 30 {
        score += 20.0;
    }

    Some(score)
}

pub fn weakness_score(page: &PageSummary) -> Option<f64> {
    if page.sessions < 20 && page.search.impressions < 200.0 {
        return None;
    }

    let mut score = 0.0;

    if page.engagement_rate < 0.30 && page.sessions >= 20 {
        score += (0.30 - page.engagement_rate) * 200.0;
    }

    if page.bounce_rate > 0.70 && page.sessions >= 20 {
        score += (page.bounce_rate - 0.70) * 120.0;
    }

    if page.search.impressions > 200.0
        && page.search.average_position > 0.0
        && page.search.average_position <= 15.0
    {
        let exp_ctr = expected_ctr(page.search.average_position);
        if exp_ctr > page.search.ctr {
            score += (exp_ctr - page.search.ctr) * page.search.impressions;
        }
    }

    if page.search.impressions > 500.0 && page.search.ctr < 0.01 {
        score += 30.0;
    }

    if page.search.average_position > 10.0
        && page.search.average_position <= 20.0
        && page.search.impressions > 200.0
    {
        score += 15.0;
    }

    if score > 0.0 { Some(score) } else { None }
}

pub fn isolated_score(page: &PageSummary, tracking_enabled: bool) -> Option<f64> {
    if !tracking_enabled {
        return None;
    }
    if page.sessions < 40 && page.search.clicks < 20.0 {
        return None;
    }
    if page.internal_link_clicks > 0 || page.service_hint_clicks > 0 {
        return None;
    }

    let mut score = page.sessions as f64 / 5.0 + page.search.clicks;
    if page.engagement_rate > 0.40 {
        score += 15.0;
    }
    Some(score)
}

pub fn ranking(pages: &[PageSummary], limit: usize, score_fn: impl Fn(&PageSummary) -> Option<f64>) -> Vec<PageSummary> {
    let mut ranked: Vec<(f64, &PageSummary)> = pages
        .iter()
        .filter_map(|page| score_fn(page).map(|score| (score, page)))
        .collect();

    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.into_iter().take(limit).map(|(_, page)| page.clone()).collect()
}

pub fn issue_label(page: &PageSummary, tracking_enabled: bool) -> String {
    let exp_ctr = if page.search.average_position > 0.0 {
        expected_ctr(page.search.average_position)
    } else {
        0.0
    };

    if tracking_enabled && page.internal_link_clicks == 0 && page.service_hint_clicks == 0 && page.sessions >= 40 {
        "Isolated article".into()
    } else if page.search.average_position > 0.0
        && page.search.average_position <= 10.0
        && page.search.ctr < exp_ctr * 0.7
        && page.engagement_rate < 0.3
    {
        "Intent mismatch".into()
    } else if page.search.average_position > 0.0
        && page.search.average_position <= 15.0
        && page.search.ctr < exp_ctr * 0.7
    {
        "Click gap".into()
    } else if page.search.average_position > 10.0
        && page.search.average_position <= 20.0
        && page.search.impressions > 200.0
    {
        "Ranking push".into()
    } else if page.engagement_rate < 0.3 {
        "Weak engagement".into()
    } else if page.bounce_rate > 0.7 {
        "High bounce".into()
    } else {
        "Opportunity".into()
    }
}

pub fn recommendation(page: &PageSummary, tracking_enabled: bool) -> String {
    let exp_ctr = if page.search.average_position > 0.0 {
        expected_ctr(page.search.average_position)
    } else {
        0.0
    };

    if tracking_enabled && page.internal_link_clicks == 0 && page.service_hint_clicks == 0 && page.sessions >= 40 {
        "Add a contextual ServiceHint and internal links from this article to the matching service page.".into()
    } else if page.search.average_position > 0.0
        && page.search.average_position <= 10.0
        && page.search.ctr < exp_ctr * 0.7
        && page.engagement_rate < 0.3
    {
        "Rewrite title/opening to match the actual query intent; the page is found, but users do not feel addressed.".into()
    } else if page.search.average_position > 0.0
        && page.search.average_position <= 15.0
        && page.search.ctr < exp_ctr * 0.7
    {
        "Improve title and meta description first; this page already ranks well enough to win clicks quickly.".into()
    } else if page.search.average_position > 10.0
        && page.search.average_position <= 20.0
        && page.search.impressions > 200.0
    {
        "Strengthen internal links and deepen the content to push this page into the top 10.".into()
    } else if page.engagement_rate < 0.3 {
        "Rework the article intro and structure so the main answer appears earlier in the page.".into()
    } else {
        "Review the top queries and tighten the article around the strongest search demand.".into()
    }
}

pub fn top_query_summary(page: &PageSummary) -> String {
    let queries: Vec<&str> = page
        .search
        .top_queries
        .iter()
        .take(3)
        .map(|q| q.query.as_str())
        .collect();

    if queries.is_empty() {
        "-".into()
    } else {
        queries.join(", ")
    }
}
