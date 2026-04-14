use std::io::Write;

use crate::domain::{
    ChannelsReport, CountriesReport, DecayReport, DevicesReport, OpportunitiesReport,
    QueriesReport, TopPagesReport,
};

pub fn write_top_pages(report: &TopPagesReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record([
        "URL", "Sessions", "Organisch", "Direkt", "Engagement %",
        "Ø Dauer (s)", "Klicks", "Impressionen", "CTR", "Position",
    ])?;
    for p in &report.pages {
        wtr.write_record([
            &p.url,
            &p.sessions.to_string(),
            &p.organic_sessions.to_string(),
            &p.direct_sessions.to_string(),
            &format!("{:.1}", p.engagement_rate * 100.0),
            &format!("{:.1}", p.avg_session_duration_secs),
            &format!("{:.0}", p.search.clicks),
            &format!("{:.0}", p.search.impressions),
            &format!("{:.4}", p.search.ctr),
            &format!("{:.1}", p.search.average_position),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_queries(report: &QueriesReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record(["Query", "Klicks", "Impressionen", "CTR", "Position"])?;
    for q in &report.queries {
        wtr.write_record([
            &q.query,
            &format!("{:.0}", q.clicks),
            &format!("{:.0}", q.impressions),
            &format!("{:.4}", q.ctr),
            &format!("{:.1}", q.position),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_opportunities(report: &OpportunitiesReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record([
        "Score", "Typ", "Keyword", "URL", "+ Klicks", "Aktuelle Klicks", "Aktion",
    ])?;
    for o in &report.opportunities {
        wtr.write_record([
            &format!("{:.1}", o.score),
            &o.type_labels.join(" + "),
            o.keyword.as_deref().unwrap_or(""),
            &o.url,
            &format!("{:.0}", o.estimated_clicks),
            &format!("{:.0}", o.current_clicks),
            &o.action,
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_channels(report: &ChannelsReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record(["Kanal", "Sessions", "Anteil %", "Engagement %", "Ø Dauer (s)"])?;
    for ch in &report.channels {
        wtr.write_record([
            &ch.channel,
            &ch.sessions.to_string(),
            &format!("{:.1}", ch.share_pct),
            &format!("{:.1}", ch.engagement_rate * 100.0),
            &format!("{:.1}", ch.avg_session_duration_secs),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_devices(report: &DevicesReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record(["Gerät", "Sessions", "Anteil %", "Engagement %", "Ø Dauer (s)"])?;
    for d in &report.devices {
        wtr.write_record([
            &d.device,
            &d.sessions.to_string(),
            &format!("{:.1}", d.share_pct),
            &format!("{:.1}", d.engagement_rate * 100.0),
            &format!("{:.1}", d.avg_session_duration_secs),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_countries(report: &CountriesReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record(["Land", "Sessions", "Anteil %", "Engagement %"])?;
    for c in &report.countries {
        wtr.write_record([
            &c.country,
            &c.sessions.to_string(),
            &format!("{:.1}", c.share_pct),
            &format!("{:.1}", c.engagement_rate * 100.0),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_decay(report: &DecayReport, w: impl Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record([
        "URL", "Klicks vorher", "Klicks nachher", "Δ Klicks %",
        "Impressionen vorher", "Impressionen nachher", "Δ Impressionen %",
        "Position vorher", "Position nachher", "Δ Position",
    ])?;
    for p in &report.declining_pages {
        wtr.write_record([
            &p.url,
            &format!("{:.0}", p.clicks_before),
            &format!("{:.0}", p.clicks_after),
            &format!("{:.1}", p.clicks_pct),
            &format!("{:.0}", p.impressions_before),
            &format!("{:.0}", p.impressions_after),
            &format!("{:.1}", p.impressions_pct),
            &format!("{:.1}", p.position_before),
            &format!("{:.1}", p.position_after),
            &format!("{:.1}", p.position_delta),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}
