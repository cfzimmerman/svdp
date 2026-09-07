//! The request detail page.
//!
//! One primitive replaces three previous ones. The detail page carries the real
//! edit form (with member *IDs*, which the list API does not expose), the
//! volunteer dropdown, and the server-rendered assistance-items table — and
//! unlike the list endpoint it works regardless of status, so a completed
//! request can still be read back. See DECISIONS.md D7.

use scraper::ElementRef;
use scraper::Html;
use scraper::Selector;

use crate::servware::client::ServWareClient;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::form::Form;

#[derive(Debug, Clone)]
pub struct RequestDetail {
    pub id: u64,
    pub form: Form,
    pub assistance_items: Vec<AssistanceItemRow>,
    pub members: Vec<Member>,
}

/// A person living in the neighbour's household.
///
/// Not to be confused with [`Member`], which is an SVdP *volunteer*. ServWare
/// uses "member" for both, which is exactly why this type spells it out.
///
/// `age` is what ServWare renders — an integer it computes server-side. There is
/// no per-person birthdate on the page, which suits us: exports carry ages and
/// never dates of birth. See DECISIONS.md D21.
///
/// There is no `last_name`: the row's surname was read out of the DOM and then
/// never emitted, and D21 says an identity column that is not exported should
/// not be read at all. Exports carry the *household's* surname, from the
/// request, which is what joins the tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseholdMember {
    pub first_name: String,
    pub relationship: String,
    pub age: Option<u32>,
}

impl HouseholdMember {
    /// Whether this row is the neighbour themselves rather than a dependant.
    ///
    /// **Verified false for every row at this conference.** Across 139 real
    /// households the table listed only *other* people, and
    /// `calculatedHouseholdCount` was the row count plus one in all 139 cases —
    /// so ServWare adds the neighbour separately. See DECISIONS.md D19.
    ///
    /// Another conference might still record one, and because household size is
    /// derived as "rows plus one", such a row would count that family twice and
    /// could move them up a gift-card rung. So it is not merely recognised, it
    /// is *excluded* — see `parse_household_members`.
    pub fn is_self(&self) -> bool {
        self.relationship.eq_ignore_ascii_case("self")
    }
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

/// The full request, including the edit form. Used by the write paths.
pub async fn fetch(client: &ServWareClient, id: u64) -> Result<RequestDetail> {
    let html = client.get_html(&detail_path(id)).await?;
    parse(id, &html)
}

/// Just the household roster.
///
/// Separate from [`fetch`] because the roster pull does not write anything and
/// so has no business failing when the *edit form* changes shape. One bad edit
/// form used to abort a 200-household export.
pub async fn fetch_household_members(
    client: &ServWareClient,
    id: u64,
) -> Result<Vec<HouseholdMember>> {
    let html = client.get_html(&detail_path(id)).await?;
    parse_household_members(&Html::parse_document(&html))
}

pub fn parse(id: u64, html: &str) -> Result<RequestDetail> {
    let doc = Html::parse_document(html);
    // The whole completion field set, not three markers of it. A page that has
    // `status` but has lost `visitNotes` is not a form we can write to, and
    // finding that out here is far better than finding it out mid-submission
    // with money already logged.
    let form = Form::extract_containing(html, crate::servware::write::COMPLETION_FIELDS)?;
    Ok(RequestDetail {
        id,
        members: parse_members(&doc),
        assistance_items: parse_assistance_items(&doc)?,
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

    /// Items of this kind already logged on this date, whoever logged them.
    ///
    /// The secondary guard on the money write: the tag can only recognise this
    /// tool's own work, and a volunteer may have typed the same entry into the
    /// ServWare website by hand. Not an idempotency key — it asks a human. See
    /// DECISIONS.md D35.
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
fn parse_members(doc: &Html) -> Vec<Member> {
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

/// The header cells of the first row that has any, lower-cased.
///
/// Scoped to **one row** on purpose. Reading every `<th>` in the table while
/// addressing body cells by `<td>` index quietly assumes the two agree, and a
/// `<th scope="row">` first cell, a `<tfoot>`, a second header row or a
/// `<th colspan=2>` breaks that. In the household table the columns after Age
/// are Phone, **SSN (Last 4)** and Drivers License, so a two-column skew reads
/// the SSN cell and `"9999".parse::<u32>()` succeeds. See DECISIONS.md D40.
fn header_row(table: ElementRef<'_>) -> Option<Vec<String>> {
    let rows = Selector::parse("tr").expect("static selector");
    let header = Selector::parse("th").expect("static selector");
    table.select(&rows).find_map(|r| {
        let cells: Vec<String> = r
            .select(&header)
            .map(|h| h.text().collect::<String>().trim().to_lowercase())
            .collect();
        (!cells.is_empty()).then_some(cells)
    })
}

/// Body rows as text, each guaranteed to have exactly `width` cells.
///
/// A row that does not is a structural change, and reading it by index would
/// silently return the wrong column. Rows with no `<td>` at all are the header
/// and are skipped.
fn body_rows(table: ElementRef<'_>, width: usize, what: &str) -> Result<Vec<Vec<String>>> {
    let rows = Selector::parse("tr").expect("static selector");
    let cell = Selector::parse("td").expect("static selector");
    let mut out = Vec::new();
    for r in table.select(&rows) {
        let cells: Vec<String> = r
            .select(&cell)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();
        if cells.is_empty() {
            continue;
        }
        if cells.len() != width {
            return Err(ServWareError::FormChanged {
                field: format!(
                    "{what} table row has {} cells where the header has {width}",
                    cells.len()
                ),
            });
        }
        out.push(cells);
    }
    Ok(out)
}

/// The assistance-items table is server-rendered; no XHR is needed.
///
/// The table carries no `id` — only Bootstrap classes — so it is located by its
/// header text. Columns are: spacer, Assistance, Value, Date Provided, Pending,
/// Promised Date, Chk Req, Chk/Conf Nbr, Notes, actions.
///
/// **`notes` is required, like the other three.** It carries the idempotency tag,
/// and it used to be the one optional column: renaming that header to "Note"
/// made `has_item_tagged` permanently false, so every write posted its money,
/// failed to recognise it on read-back, reported "Nothing was written", and
/// invited a retry that posted it again. See DECISIONS.md D41.
fn parse_assistance_items(doc: &Html) -> Result<Vec<AssistanceItemRow>> {
    let tables = Selector::parse("table").expect("static selector");

    let Some(table) = doc.select(&tables).find(|t| {
        header_row(*t).is_some_and(|h| {
            h.iter().any(|c| c == "assistance") && h.iter().any(|c| c == "date provided")
        })
    }) else {
        // No such table at all. An open request that has never been helped has
        // nothing to render, so this is a legitimate empty answer.
        return Ok(Vec::new());
    };

    let columns = header_row(table).expect("the finder above matched on it");
    let index_of = |name: &str| {
        columns
            .iter()
            .position(|c| c == name)
            .ok_or_else(|| ServWareError::FormChanged {
                field: format!("assistance items table has no `{name}` column"),
            })
    };
    let i_kind = index_of("assistance")?;
    let i_value = index_of("value")?;
    let i_date = index_of("date provided")?;
    let i_notes = index_of("notes")?;

    Ok(body_rows(table, columns.len(), "assistance items")?
        .into_iter()
        .filter(|cells| !cells[i_kind].is_empty())
        .map(|cells| AssistanceItemRow {
            kind: cells[i_kind].clone(),
            value: cells[i_value].trim_start_matches('$').to_string(),
            date_provided: cells[i_date].clone(),
            tag: extract_tag(&cells[i_notes]),
        })
        .collect())
}

/// The largest age this parser will believe.
///
/// A plausibility bound used to exist only in the opt-in local-capture test,
/// which meant it never ran anywhere a wrong value could do harm.
const MAX_PLAUSIBLE_AGE: u32 = 120;

/// The household-members table, from the "Household Members" tab.
///
/// Every tab on the detail page is server-rendered inline, so this needs no XHR
/// and costs nothing beyond the page we already fetch. See DECISIONS.md D19.
///
/// The table also carries SSN (Last 4), Drivers License/ID, Phone and Notes
/// columns. Those cells are **never read out of the DOM** — only the wanted
/// columns are addressed by index, so their contents cannot reach a string, a
/// log line, or a CSV even by accident. `tests/exports.rs` pins that.
///
/// Three outcomes, and keeping them apart is the point:
///
/// * **Rows.** The household's dependants.
/// * **No rows.** ServWare accepts a head count *or* individual people, not
///   both, and 19 of 158 households in one window had only the head count. A
///   real and common answer.
/// * **Error.** The tab or a column it must have is gone. This used to return
///   an empty roster too — indistinguishable from the case above — so a renamed
///   tab id would have had the tool announce, with total confidence, that every
///   household in the conference has nobody living in it. See DECISIONS.md D32.
pub fn parse_household_members(doc: &Html) -> Result<Vec<HouseholdMember>> {
    let tab_sel = Selector::parse("#tabs-familymembers").expect("static selector");
    let table_sel = Selector::parse("table").expect("static selector");

    let Some(tab) = doc.select(&tab_sel).next() else {
        return Err(ServWareError::FormChanged {
            field: "the Household Members tab (#tabs-familymembers)".into(),
        });
    };
    // The tab is there but renders no table: nobody is listed.
    let Some(table) = tab.select(&table_sel).next() else {
        return Ok(Vec::new());
    };
    let Some(columns) = header_row(table) else {
        return Err(ServWareError::FormChanged {
            field: "the Household Members table has no header row".into(),
        });
    };

    let index_of = |name: &str| {
        columns
            .iter()
            .position(|c| c == name)
            .ok_or_else(|| ServWareError::FormChanged {
                field: format!("household members table has no `{name}` column"),
            })
    };
    // Columns are found by header text, never by position: the page has no
    // per-row ids, and a column inserted upstream must not silently turn the
    // driver's-licence column into "age".
    let i_first = index_of("first name")?;
    let i_relationship = index_of("relationship")?;
    let i_age = index_of("age")?;

    let mut out = Vec::new();
    for cells in body_rows(table, columns.len(), "household members")? {
        let first_name = cells[i_first].clone();
        if first_name.is_empty() {
            continue;
        }
        let raw_age = &cells[i_age];
        let age = match raw_age.parse::<u32>() {
            Ok(a) if a <= MAX_PLAUSIBLE_AGE => Some(a),
            // Blank means ServWare does not know, which is normal.
            Err(_) if raw_age.is_empty() => None,
            // Anything else means this cell is not an age. Refusing beats
            // exporting it: the columns just past Age are Phone and SSN.
            _ => {
                return Err(ServWareError::FormChanged {
                    field: format!(
                        "the household members `age` column holds something that is not an \
                         age ({} characters); the columns may have shifted",
                        raw_age.len()
                    ),
                });
            }
        };
        let member = HouseholdMember {
            first_name,
            relationship: cells[i_relationship].clone(),
            age,
        };
        // "Rows plus one" is the documented household size, so the neighbour
        // must not also appear as a row. See `HouseholdMember::is_self`.
        if member.is_self() {
            tracing::warn!("household roster listed the neighbour themselves; excluding the row");
            continue;
        }
        out.push(member);
    }
    Ok(out)
}

/// Our tag looks like `svdp:s=<session>;i=<slot>` at the start of the notes.
pub fn extract_tag(notes: &str) -> Option<String> {
    let start = notes.find("svdp:")?;
    let rest = &notes[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches(['—', '-', ',']).to_string())
}
