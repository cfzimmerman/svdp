use anyhow::Context;
use anyhow::ensure;
use scraper::Html;
use scraper::Selector;
use serde::Deserialize;
use serde::Serialize;

use super::ServWare;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A volunteer member scraped from the request detail page.
#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ServWare {
    /// Fetch the list of volunteer members from a request detail page.
    ///
    /// ServWare has no JSON endpoint for the member list — the IDs are only
    /// available as `<option>` elements inside the
    /// `<select id="requestAssignedToMemberId">` dropdown on the request
    /// detail HTML page.
    pub async fn fetch_members(&self, request_id: u64) -> anyhow::Result<Vec<Member>> {
        let url = Self::request_url(request_id);
        tracing::debug!(%url, "fetching request detail page for member list");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("failed to fetch request detail page")?;

        let status = response.status();
        if !status.is_success() && !status.is_redirection() {
            anyhow::bail!("fetch request detail page failed with status {status}");
        }

        let html = response
            .text()
            .await
            .context("failed to read request detail page body")?;

        let document = Html::parse_document(&html);

        let select_selector = Selector::parse(r#"select#requestAssignedToMemberId option"#)
            .expect("valid CSS selector");

        let members: Vec<Member> = document
            .select(&select_selector)
            .filter_map(|el| {
                let value = el.value().attr("value")?.trim().to_string();
                if value.is_empty() {
                    return None; // skip "-- Select --" placeholder
                }
                let name = el.text().collect::<String>().trim().to_string();
                Some(Member { id: value, name })
            })
            .collect();

        ensure!(
            !members.is_empty(),
            "no members found in request {request_id} — page structure may have changed"
        );

        tracing::debug!(count = members.len(), "scraped member list");
        Ok(members)
    }
}
