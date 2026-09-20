//! The seam between the reports and Google.
//!
//! `src/reports/` used to call the HTTP clients directly and take an
//! `access_token: &str`, so nothing below `build` could run without a live
//! OAuth login — which is why the whole layer that merges GA4 and Search
//! Console data went untested.
//!
//! This trait is the one boundary. [`HttpGoogleApi`] talks to Google; a test
//! supplies recorded responses instead. Report modules are generic over it and
//! never see a token.

use crate::errors::Result;

use super::analytics_admin::Ga4Property;
use super::analytics_data::{ReportRequest, RunReportResponse};
#[cfg(test)]
use super::analytics_data::ReportRow;
use super::search_console::{
    SearchAnalyticsRequest, SearchAnalyticsResponse, SitemapInfo, UrlInspectionData,
};

/// Everything the reports need from Google.
pub trait GoogleApi {
    /// GA4 Data API `runReport`.
    fn run_report(
        &self,
        request: ReportRequest,
    ) -> impl std::future::Future<Output = Result<RunReportResponse>>;

    /// Search Console `searchAnalytics.query`.
    fn search_analytics(
        &self,
        request: SearchAnalyticsRequest,
    ) -> impl std::future::Future<Output = Result<SearchAnalyticsResponse>>;

    /// Every sitemap Search Console knows for the property.
    fn list_sitemaps(
        &self,
        site_url: &str,
    ) -> impl std::future::Future<Output = Result<Vec<SitemapInfo>>>;

    /// Indexing status for a single URL.
    fn inspect_url(
        &self,
        site_url: &str,
        inspection_url: &str,
    ) -> impl std::future::Future<Output = Result<UrlInspectionData>>;

    /// Every GA4 property the account can read.
    fn list_properties(&self) -> impl std::future::Future<Output = Result<Vec<Ga4Property>>>;

    /// Every Search Console property the account can read.
    fn list_sites(&self) -> impl std::future::Future<Output = Result<Vec<String>>>;
}

/// Talks to the real Google APIs with a bearer token.
pub struct HttpGoogleApi {
    access_token: String,
}

impl HttpGoogleApi {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
        }
    }
}

impl GoogleApi for HttpGoogleApi {
    async fn run_report(&self, request: ReportRequest) -> Result<RunReportResponse> {
        super::analytics_data::run_report(&self.access_token, request).await
    }

    async fn search_analytics(
        &self,
        request: SearchAnalyticsRequest,
    ) -> Result<SearchAnalyticsResponse> {
        super::search_console::query(&self.access_token, request).await
    }

    async fn list_sitemaps(&self, site_url: &str) -> Result<Vec<SitemapInfo>> {
        super::search_console::list_sitemaps(&self.access_token, site_url).await
    }

    async fn inspect_url(
        &self,
        site_url: &str,
        inspection_url: &str,
    ) -> Result<UrlInspectionData> {
        super::search_console::inspect_url(&self.access_token, site_url, inspection_url).await
    }

    async fn list_properties(&self) -> Result<Vec<Ga4Property>> {
        super::analytics_admin::list_properties(&self.access_token).await
    }

    async fn list_sites(&self) -> Result<Vec<String>> {
        super::search_console::list_sites(&self.access_token).await
    }
}

/// A [`GoogleApi`] that answers from canned responses instead of the network.
///
/// Requests are matched on their dimensions, because that is what distinguishes
/// the calls a report makes: `top_pages` asks GA4 for `pagePath` x channel and
/// for `pagePath` x `eventName`, and Search Console for `page` and for
/// `page` x `query`.
///
/// Where a report asks the same question of two periods — `compare` before and
/// after a change date, `decay` this period against the last — the dimensions
/// are identical and only the start date differs. Those answers are registered
/// with [`Self::with_report_at`] and [`Self::with_search_at`], which take
/// precedence over a dimensions-only entry.
///
/// An unregistered request panics rather than returning an empty result — a
/// test that silently asserts over no data proves nothing.
#[cfg(test)]
#[derive(Default)]
pub struct FixtureGoogleApi {
    reports: std::collections::HashMap<String, RunReportResponse>,
    searches: std::collections::HashMap<String, SearchAnalyticsResponse>,
}

#[cfg(test)]
impl FixtureGoogleApi {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(dimensions: &[String]) -> String {
        dimensions.join("+")
    }

    fn dated_key(dimensions: &[String], start_date: &str) -> String {
        format!("{}@{start_date}", dimensions.join("+"))
    }

    /// Answer a GA4 `runReport` for these dimensions with these rows.
    ///
    /// Rows are `(dimension values, metric values)` in the order the request
    /// asks for them, as strings — the Data API returns every metric as a
    /// string, and the parsing of those strings is part of what is under test.
    pub fn with_report(
        self,
        dimensions: &[&str],
        rows: Vec<(Vec<&str>, Vec<&str>)>,
    ) -> Self {
        self.insert_report(Self::key(&to_owned(dimensions)), dimensions, rows)
    }

    /// The same, for a request whose date range starts on `start_date`.
    pub fn with_report_at(
        self,
        dimensions: &[&str],
        start_date: &str,
        rows: Vec<(Vec<&str>, Vec<&str>)>,
    ) -> Self {
        self.insert_report(
            Self::dated_key(&to_owned(dimensions), start_date),
            dimensions,
            rows,
        )
    }

    fn insert_report(
        mut self,
        key: String,
        dimensions: &[&str],
        rows: Vec<(Vec<&str>, Vec<&str>)>,
    ) -> Self {
        let rows: Vec<ReportRow> = rows
            .into_iter()
            .map(|(dims, metrics)| ReportRow {
                dimension_values: dims.into_iter().map(String::from).collect(),
                metric_values: metrics.into_iter().map(String::from).collect(),
            })
            .collect();
        let dimensions: Vec<String> = dimensions.iter().map(|d| d.to_string()).collect();
        let row_count = rows.len() as i64;
        self.reports.insert(
            key,
            RunReportResponse {
                dimension_headers: dimensions,
                metric_headers: Vec::new(),
                rows,
                row_count,
            },
        );
        self
    }

    /// Answer a Search Console query for these dimensions.
    ///
    /// Rows are `(keys, clicks, impressions, ctr, position)`.
    pub fn with_search(
        self,
        dimensions: &[&str],
        rows: Vec<(Vec<&str>, f64, f64, f64, f64)>,
    ) -> Self {
        self.insert_search(Self::key(&to_owned(dimensions)), rows)
    }

    /// The same, for a query starting on `start_date`.
    pub fn with_search_at(
        self,
        dimensions: &[&str],
        start_date: &str,
        rows: Vec<(Vec<&str>, f64, f64, f64, f64)>,
    ) -> Self {
        self.insert_search(Self::dated_key(&to_owned(dimensions), start_date), rows)
    }

    fn insert_search(
        mut self,
        key: String,
        rows: Vec<(Vec<&str>, f64, f64, f64, f64)>,
    ) -> Self {
        let rows = rows
            .into_iter()
            .map(|(keys, clicks, impressions, ctr, position)| {
                super::search_console::SearchAnalyticsRow {
                    keys: keys.into_iter().map(String::from).collect(),
                    clicks,
                    impressions,
                    ctr,
                    position,
                }
            })
            .collect();
        self.searches.insert(key, SearchAnalyticsResponse { rows });
        self
    }
}

#[cfg(test)]
impl GoogleApi for FixtureGoogleApi {
    async fn run_report(&self, request: ReportRequest) -> Result<RunReportResponse> {
        let start = request
            .date_ranges
            .first()
            .map(|r| r.start_date.as_str())
            .unwrap_or("");
        let dated = Self::dated_key(&request.dimensions, start);
        let plain = Self::key(&request.dimensions);
        self.reports
            .get(&dated)
            .or_else(|| self.reports.get(&plain))
            .cloned()
            .ok_or_else(|| panic!("fixture has no GA4 report for `{dated}` or `{plain}`"))
    }

    async fn search_analytics(
        &self,
        request: SearchAnalyticsRequest,
    ) -> Result<SearchAnalyticsResponse> {
        let dated = Self::dated_key(&request.dimensions, &request.start_date);
        let plain = Self::key(&request.dimensions);
        self.searches
            .get(&dated)
            .or_else(|| self.searches.get(&plain))
            .cloned()
            .ok_or_else(|| {
                panic!("fixture has no Search Console response for `{dated}` or `{plain}`")
            })
    }

    async fn list_sitemaps(&self, _site_url: &str) -> Result<Vec<SitemapInfo>> {
        Ok(Vec::new())
    }

    async fn inspect_url(
        &self,
        _site_url: &str,
        _inspection_url: &str,
    ) -> Result<UrlInspectionData> {
        unimplemented!("no test needs URL inspection yet")
    }

    async fn list_properties(&self) -> Result<Vec<Ga4Property>> {
        Ok(Vec::new())
    }

    async fn list_sites(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
fn to_owned(dimensions: &[&str]) -> Vec<String> {
    dimensions.iter().map(|d| d.to_string()).collect()
}
