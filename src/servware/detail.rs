//! The request detail page.
//!
//! One primitive replaces three previous ones. The detail page carries the real
//! edit form (with member *IDs*, which the list API does not expose), the
//! volunteer dropdown, and the server-rendered assistance-items table — and
//! unlike the list endpoint it works regardless of status, so a completed
//! request can still be read back. See DECISIONS.md D7.

use scraper::Html;
use scraper::Selector;

use crate::servware::client::ServWareClient;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::form::Form;

/// The request edit form is identified by the controls it renders, not by its
/// element id. The page carries seven other forms -- notes, denial-reason,
/// visit-notes, email, map and logout modals -- whose controls are not ours.
const REQUEST_FORM_MARKERS: &[&str] = &["status", "visitNotes", "visitCompleted"];

#[derive(Debug, Clone)]
pub struct RequestDetail {
    pub id: u64,
    pub form: Form,
    pub assistance_items: Vec<AssistanceItemRow>,
    pub members: Vec<Member>,
}

/// A volunteer, as rendered in the assignment dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub name: String,
}

/// An assistance item already recorded against this request.
///
/// Read before writing, so a re-run does not double-log money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistanceItemRow {
    pub kind: String,
    pub value: String,
    pub date_provided: String,
    /// Our machine tag, when this row was written by this tool.
    pub tag: Option<String>,
}

pub fn detail_path(id: u64) -> String {
    format!("/app/assistancerequests/{id}")
}

pub async fn fetch(client: &ServWareClient, id: u64) -> Result<RequestDetail> {
    let html = client.get_html(&detail_path(id)).await?;
    parse(id, &html)
}

pub fn parse(id: u64, html: &str) -> Result<RequestDetail> {
    let form = Form::extract_containing(html, REQUEST_FORM_MARKERS).map_err(|_| {
        ServWareError::FormChanged {
            field: format!("request edit form (looked for {REQUEST_FORM_MARKERS:?})"),
        }
    })?;
    Ok(RequestDetail {
        id,
        members: parse_members(html),
        assistance_items: parse_assistance_items(html),
        form,
    })
}

impl RequestDetail {
    pub fn status(&self) -> &str {
        self.form.get("status").unwrap_or_default()
    }

    pub fn is_completed(&self) -> bool {
        self.status().eq_ignore_ascii_case("Completed")
    }

    /// Whether an item carrying this tag has already been recorded.
    ///
    /// The tag is the only exact idempotency key available: no natural key
    /// works, because `dateProvided` is a date (so a retry across midnight
    /// duplicates) and including the amount turns a correction into a
    /// double-log. See DECISIONS.md D7.
    pub fn has_item_tagged(&self, tag: &str) -> bool {
        self.assistance_items
            .iter()
            .any(|i| i.tag.as_deref() == Some(tag))
    }

    /// Items that look like this kind was already logged today, regardless of
    /// tag. Used to ask a human rather than guess when tagging is disabled.
    pub fn items_like(&self, kind: &str, date: &str) -> Vec<&AssistanceItemRow> {
        self.assistance_items
            .iter()
            .filter(|i| i.kind.contains(kind) && i.date_provided == date)
            .collect()
    }
}

/// Volunteers come from the assignment `<select>`.
///
/// Note `id` and `name` diverge on this page, so selection is by `name`.
fn parse_members(html: &str) -> Vec<Member> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"select[name="requestAssignedToMemberId"] option"#)
        .expect("static selector");
    doc.select(&sel)
        .filter_map(|o| {
            let id = o.value().attr("value")?.trim().to_string();
            if id.is_empty() {
                return None; // the "-- Select --" placeholder
            }
            let name = o.text().collect::<String>().trim().to_string();
            Some(Member { id, name })
        })
        .collect()
}

/// The assistance-items table is server-rendered; no XHR is needed.
///
/// The table carries no `id` — only Bootstrap classes — so it is located by its
/// header text. Columns are: spacer, Assistance, Value, Date Provided, Pending,
/// Promised Date, Chk Req, Chk/Conf Nbr, Notes, actions.
fn parse_assistance_items(html: &str) -> Vec<AssistanceItemRow> {
    let doc = Html::parse_document(html);
    let tables = Selector::parse("table").expect("static selector");
    let header = Selector::parse("th").expect("static selector");
    let row = Selector::parse("tr").expect("static selector");
    let cell = Selector::parse("td").expect("static selector");

    let Some(table) = doc.select(&tables).find(|t| {
        let heads: Vec<String> = t
            .select(&header)
            .map(|h| h.text().collect::<String>().trim().to_lowercase())
            .collect();
        heads.iter().any(|h| h == "assistance") && heads.iter().any(|h| h == "date provided")
    }) else {
        return Vec::new();
    };

    let columns: Vec<String> = table
        .select(&header)
        .map(|h| h.text().collect::<String>().trim().to_lowercase())
        .collect();
    let index_of = |name: &str| columns.iter().position(|c| c == name);
    let (i_kind, i_value, i_date) = match (
        index_of("assistance"),
        index_of("value"),
        index_of("date provided"),
    ) {
        (Some(k), Some(v), Some(d)) => (k, v, d),
        _ => return Vec::new(),
    };
    let i_notes = index_of("notes");

    table
        .select(&row)
        .filter_map(|r| {
            let cells: Vec<String> = r
                .select(&cell)
                .map(|c| c.text().collect::<String>().trim().to_string())
                .collect();
            let kind = cells.get(i_kind)?.clone();
            if kind.is_empty() {
                return None;
            }
            Some(AssistanceItemRow {
                kind,
                value: cells
                    .get(i_value)
                    .map(|v| v.trim_start_matches('$').to_string())
                    .unwrap_or_default(),
                date_provided: cells.get(i_date).cloned().unwrap_or_default(),
                tag: i_notes.and_then(|i| cells.get(i)).and_then(|n| extract_tag(n)),
            })
        })
        .collect()
}

/// Our tag looks like `svdp:s=<session>;i=<slot>` at the start of the notes.
pub fn extract_tag(notes: &str) -> Option<String> {
    let start = notes.find("svdp:")?;
    let rest = &notes[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches(['—', '-', ',']).to_string())
}
