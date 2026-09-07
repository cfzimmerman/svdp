//! Writes to ServWare, verified by read-back.
//!
//! **HTTP status is not evidence of success.** Spring re-renders a rejected form
//! as 200, and the client follows redirects, so the previous implementation
//! reported "logged $70" for writes ServWare had refused. Every write here is
//! confirmed by re-reading the request. See DECISIONS.md D9.

use crate::domain::policy::ConferenceConfig;
use crate::domain::policy::Slot;
use crate::servware::client::ServWareClient;
use crate::servware::detail;
use crate::servware::detail::RequestDetail;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::form::Form;

/// Every control the completion write touches.
///
/// **One list, read by the write, by the CLI dry run, and by both health
/// checks.** It used to be spelled out in four places with three different
/// memberships: the CLI preview set `requestAssignedToMemberId` unconditionally
/// while the write sets it only when the county's slot is empty, so the dry run
/// -- the maintainer's only gate before an irreversible write -- already
/// described a change the write would not make. See DECISIONS.md D33.
///
/// Also passed to `Form::extract_containing` as the required-control set, so a
/// page missing any of them fails to parse rather than silently omitting a
/// field from the POST.
pub const COMPLETION_FIELDS: &[&str] = &[
    "status",
    "requestAssignedToMemberId",
    "visitAssignedToMemberId",
    "homeVisitRequired",
    "homeVisitCnt",
    "visitCompleted",
    "visitMileageInService",
    "visitScheduledDate",
    "visitNotes",
];

/// What actually happened to one write slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The write was made and confirmed by re-reading the request.
    Applied,
    /// This exact write was already present. Nothing was sent.
    AlreadyDone,
    /// Someone changed the request in ServWare; a human must decide.
    Conflict(String),
}

/// The exact form a completion write will submit.
///
/// The single source of truth for "what does marking this complete change".
/// Both the write and every preview of it call this, so a dry run cannot
/// describe something different from what the write does.
///
/// `requestAssignedToMemberId` is the county's *intake* assignment, not ours.
/// It is claimed only when nobody holds it; overwriting would destroy their
/// record of who took the request. The visit assignment is ours to set either
/// way. See DECISIONS.md D17.
pub fn plan_complete(
    before: &RequestDetail,
    config: &ConferenceConfig,
    volunteer_id: &str,
    visit_date: &str,
) -> Result<Form> {
    let intake_unassigned = before
        .form
        .get("requestAssignedToMemberId")
        .is_none_or(str::is_empty);

    let mut changes = vec![
        ("status", "Completed".to_string()),
        ("visitAssignedToMemberId", volunteer_id.to_string()),
        ("homeVisitRequired", "true".to_string()),
        ("homeVisitCnt", "1".to_string()),
        ("visitCompleted", "true".to_string()),
        ("visitMileageInService", config.visit_mileage.clone()),
        ("visitScheduledDate", visit_date.to_string()),
        ("visitNotes", config.visit_notes_html.clone()),
    ];
    if intake_unassigned {
        changes.push(("requestAssignedToMemberId", volunteer_id.to_string()));
    }
    let after = before.form.overlay(changes)?;

    // Prove we are changing only what we named. This used to be a
    // `debug_assert!` on the *count* of changed fields, which (a) checked a
    // weaker property than "only these fields" and (b) was compiled out of the
    // `--release` binary that actually ships -- so the comment claiming
    // "clobbering becomes impossible by construction" described a check that
    // did not exist in production. See DECISIONS.md D34.
    if let Some((name, _, _)) = before
        .form
        .diff(&after)
        .into_iter()
        .find(|(n, _, _)| !COMPLETION_FIELDS.contains(&n.as_str()))
    {
        return Err(ServWareError::FormChanged { field: name });
    }
    Ok(after)
}

/// Mark a request complete and credit the visit to a volunteer.
///
/// Skips rather than overwrites when the request is already completed: the
/// county may have closed it for an unrelated reason, and blindly rewriting
/// would clobber their assignment and notes.
pub async fn mark_complete(
    client: &ServWareClient,
    config: &ConferenceConfig,
    request_id: u64,
    volunteer_id: &str,
    visit_date: &str,
) -> Result<WriteOutcome> {
    let before = detail::fetch(client, request_id).await?;

    // Someone else closing the request is the conflict that actually happens,
    // and this is what detects it. There used to be an optimistic-lock check
    // beside it reading `before.form.get("version")` -- but `version` comes from
    // the JSON list endpoint and the edit form has no such control (verified:
    // 50 named controls, none of them `version`), so the `&&` chain
    // short-circuited and the comparison never ran. See DECISIONS.md D37.
    if before.is_completed() {
        return Ok(WriteOutcome::AlreadyDone);
    }

    let after = plan_complete(&before, config, volunteer_id, visit_date)?;
    if !before
        .form
        .get("requestAssignedToMemberId")
        .is_none_or(str::is_empty)
    {
        tracing::info!(request_id, "leaving the existing intake assignment alone");
    }

    client.post_form(&detail::detail_path(request_id), &after.pairs()).await?;

    // Read-back verification: the only trustworthy signal.
    let confirmed = detail::fetch(client, request_id).await?;
    if !confirmed.is_completed() {
        return Err(ServWareError::WriteRejected {
            request_id,
            what: "completion".into(),
        });
    }
    Ok(WriteOutcome::Applied)
}

/// Add one assistance item, skipping if this tool already recorded it.
///
/// The form mirrors the captured browser POST field-for-field and in order --
/// 30 fields, not the 13 that `api.md` lists. See DECISIONS.md D9.
///
/// Three guards, in order, because ServWare cannot delete an assistance item:
///
/// 1. **Our own tag.** The exact idempotency key (D7). Already there means we
///    already did this; nothing is sent.
/// 2. **Same kind, same day, whatever the tag.** Somebody may have logged it by
///    hand in the ServWare website. The tag cannot see that, and a second $70 is
///    not recoverable, so this asks a human instead of guessing. This used to
///    live behind `tag_assistance_notes = false`, a config branch nothing
///    selected. See DECISIONS.md D35.
/// 3. **Read-back of the amount and the type**, not merely of the tag. The form
///    is hand-enumerated, so if ServWare renamed `monetaryValue` Spring would
///    re-render 200, create the item at its own default, and the tag would still
///    be there -- reporting success over a wrong number in the county's books.
#[allow(clippy::too_many_arguments)]
pub async fn add_assistance_item(
    client: &ServWareClient,
    config: &ConferenceConfig,
    request_id: u64,
    client_id: u64,
    session: &str,
    slot: Slot,
    dollars: u32,
    date_provided: &str,
) -> Result<WriteOutcome> {
    let (type_id, expected_kind) = match slot {
        Slot::Food => (&config.second_harvest.id, &config.second_harvest.name_contains),
        Slot::GiftCard => (&config.gift_card.id, &config.gift_card.name_contains),
        Slot::Complete => {
            return Err(ServWareError::Malformed(
                "completion is not an assistance item".into(),
            ));
        }
    };

    // Nothing between the model's JSON and this POST bounded the amount. A
    // typo or a hallucinated figure is money in somebody's books that cannot be
    // taken back out. See DECISIONS.md D36.
    if dollars > config.max_item_dollars {
        return Err(ServWareError::AmountRefused {
            dollars,
            max: config.max_item_dollars,
        });
    }

    let tag = config.tag(session, slot);
    let before = detail::fetch(client, request_id).await?;

    // ServWare is the source of truth; the local receipt is only an optimization.
    // Re-read before every write, including the first.
    if before.has_item_tagged(&tag) {
        return Ok(WriteOutcome::AlreadyDone);
    }
    if !before.items_like(expected_kind, date_provided).is_empty() {
        return Ok(WriteOutcome::Conflict(format!(
            "a {expected_kind} item is already logged against this request for \
             {date_provided}, and it was not put there by this tool"
        )));
    }

    let notes = config.item_notes(session, slot, date_provided);
    let form = assistance_form(type_id, client_id, dollars, date_provided, &notes);
    let path = format!("/app/assistancerequests/{request_id}/assistanceitems/new");
    client.post_form(&path, &form).await?;

    let confirmed = detail::fetch(client, request_id).await?;
    let what = format!("${dollars} {} entry", slot.as_str());
    let Some(written) = confirmed
        .assistance_items
        .iter()
        .find(|i| i.tag.as_deref() == Some(tag.as_str()))
    else {
        // Two very different situations, and telling a volunteer the wrong one
        // is how the same $70 gets logged three times. If the item count grew,
        // something landed and we simply cannot recognise it -- saying "nothing
        // was written" there invites a retry that charges again.
        return Err(if confirmed.assistance_items.len() > before.assistance_items.len() {
            ServWareError::WriteUnverifiable { request_id, what }
        } else {
            ServWareError::WriteRejected { request_id, what }
        });
    };

    let recorded: Option<f64> = written.value.replace(',', "").parse().ok();
    if recorded != Some(f64::from(dollars)) {
        return Err(ServWareError::WriteUnverifiable {
            request_id,
            what: format!("{what} (ServWare recorded {:?} instead)", written.value),
        });
    }
    if !written.kind.contains(expected_kind.as_str()) {
        return Err(ServWareError::WriteUnverifiable {
            request_id,
            what: format!("{what} (ServWare recorded it as {:?})", written.kind),
        });
    }
    Ok(WriteOutcome::Applied)
}

/// The assistance-item form, in the exact field order the browser sends.
///
/// Pinned by a golden test; do not reorder or prune. `_pending` and
/// `_checkRequested` are Spring checkbox companions submitted without their
/// controls, meaning "unchecked".
pub fn assistance_form(
    type_id: &str,
    client_id: u64,
    dollars: u32,
    date_provided: &str,
    notes: &str,
) -> Vec<(String, String)> {
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());
    vec![
        pair("assistanceTypeId", type_id),
        pair("clientId", &client_id.to_string()),
        pair("housingProviderId", ""),
        pair("vendorId", ""),
        pair("utilityId", ""),
        pair("clientAccountId", ""),
        pair("clientAccountName", ""),
        pair("clientAccountNumber", ""),
        pair("clientAccountHolder", ""),
        pair("specialProgramId", ""),
        pair("inKindSubType", ""),
        pair("monetaryValue", &dollars.to_string()),
        pair("accountId", ""),
        pair("quantity", "1"),
        pair("dateProvided", date_provided),
        pair("voucherAsstId", ""),
        pair("_pending", "on"),
        pair("promisedDate", ""),
        pair("_checkRequested", "on"),
        pair("datePaid", ""),
        pair("checkNumber", ""),
        pair("payeeName", ""),
        pair("notes", notes),
        pair("councilPaymentValue", ""),
        pair("councilCheckConfNumber", ""),
        pair("districtPaymentValue", ""),
        pair("districtCheckConfNumber", ""),
        pair("otherPaymentValue", ""),
        pair("otherCheckConfNumber", ""),
        pair("action", "save"),
    ]
}

/// The live backend: the real thing behind `DeliveryBackend`.
pub struct ServWareBackend<'a> {
    pub client: &'a ServWareClient,
    pub config: &'a ConferenceConfig,
}

impl crate::domain::submit::DeliveryBackend for ServWareBackend<'_> {
    async fn ensure_item(
        &self,
        request_id: u64,
        client_id: u64,
        session_id: &str,
        slot: Slot,
        dollars: u32,
        date: &str,
    ) -> Result<WriteOutcome> {
        add_assistance_item(
            self.client, self.config, request_id, client_id, session_id, slot, dollars, date,
        )
        .await
    }

    async fn ensure_complete(
        &self,
        request_id: u64,
        volunteer_id: &str,
        date: &str,
    ) -> Result<WriteOutcome> {
        mark_complete(self.client, self.config, request_id, volunteer_id, date).await
    }
}
