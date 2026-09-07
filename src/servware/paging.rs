//! ServWare's DataTables server-side pagination contract, in one place.
//!
//! Both list endpoints speak it, and both used to implement it separately: the
//! envelope, the page size, the null-strip, the decode, and the
//! compare-against-`iTotalDisplayRecords` loop existed twice over, with a third
//! copy of the decode inside `list::fetch_window`.
//!
//! They had already drifted. One reported an exhausted page budget as
//! `TooBroad`, whose message is written for a volunteer and reaches them
//! verbatim ("Narrow the date range"); the other two reported the same
//! situation as `Malformed`, which replaces it with "ServWare sent back
//! something this tool did not understand. Nothing was written." — the exact
//! regression DECISIONS.md D25 exists to prevent. See DECISIONS.md D42.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::servware::error::Result;
use crate::servware::error::ServWareError;

/// What every DataTables endpoint wraps its rows in.
#[derive(Debug, Deserialize)]
pub struct Envelope {
    #[serde(rename = "iTotalDisplayRecords")]
    pub total_display_records: u32,
    #[serde(rename = "aaData")]
    pub data: Vec<serde_json::Value>,
}

/// Rows per request. ServWare accepts more, but a hundred keeps any single
/// response small enough to parse without a spike in memory.
pub const PAGE_SIZE: u32 = 100;

/// Remove null-valued keys so `#[serde(default)]` applies to optional fields.
/// Required fields are unaffected and still fail when genuinely absent.
pub fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

/// Decode one page of rows, with the message a volunteer should see when
/// ServWare's shape has changed under us.
pub fn decode_rows<T: DeserializeOwned>(
    rows: Vec<serde_json::Value>,
    noun: &str,
) -> Result<Vec<T>> {
    rows.into_iter()
        .map(|mut raw| {
            strip_nulls(&mut raw);
            serde_json::from_value(raw).map_err(|e| {
                ServWareError::Malformed(format!(
                    "ServWare's {noun} format changed — {e}. \
                     This tool needs an update; nothing was written."
                ))
            })
        })
        .collect()
}

/// Walk every page of a listing, following `iTotalDisplayRecords` to completion.
///
/// `max_pages` of 0 means no limit, and should be used only by a caller that has
/// thought about the request count: ServWare is production and every page is a
/// request against it.
pub async fn paginate<T, F, Fut>(noun: &str, max_pages: u32, fetch: F) -> Result<Vec<T>>
where
    T: DeserializeOwned,
    F: Fn(u32) -> Fut,
    Fut: Future<Output = Result<Envelope>>,
{
    let mut out: Vec<T> = Vec::new();
    let mut start = 0u32;
    let mut pages = 0u32;

    loop {
        let envelope = fetch(start).await?;
        pages += 1;
        let total = envelope.total_display_records;
        let returned = envelope.data.len();
        out.extend(decode_rows::<T>(envelope.data, noun)?);

        if returned == 0 || out.len() as u32 >= total {
            if (out.len() as u32) < total {
                // A short read: the server says there are more but stopped
                // sending them. Silently returning a partial list is how a
                // family gets left off a Christmas programme.
                return Err(ServWareError::Malformed(format!(
                    "ServWare reported {total} {noun} but only {} could be read",
                    out.len()
                )));
            }
            return Ok(out);
        }
        if max_pages != 0 && pages >= max_pages {
            // Nothing is wrong here — the caller asked for more than the budget
            // allows — so this is guidance for a person, not a fault. It must
            // stay `TooBroad`, whose message reaches them unaltered.
            return Err(ServWareError::TooBroad(format!(
                "There are {total} matching {noun}, which is more than this will read in \
                 one go ({} read so far). Narrow it down rather than fetching everything.",
                out.len()
            )));
        }
        start += PAGE_SIZE;
    }
}
