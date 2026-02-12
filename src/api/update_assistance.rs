use anyhow::Context;

use super::ServWare;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Fields for adding a new assistance item to a request.
///
/// Mirrors the full form captured from the ServWare UI. All optional/unused
/// fields are sent as empty strings to match browser behaviour.
pub struct UpdateAssistanceInput {
    // Required
    pub assistance_type_id: String,
    pub client_id: String,
    pub monetary_value: String,
    pub quantity: String,
    pub date_provided: String,

    // Optional
    pub notes: String,
    pub client_account_name: String,
    pub client_account_holder: String,
    pub client_account_number: String,
    pub payee_name: String,
    pub check_requested: bool,
}

impl UpdateAssistanceInput {
    /// Create a new input with required fields and sensible defaults.
    pub fn new(
        assistance_type_id: impl Into<String>,
        client_id: impl Into<String>,
        monetary_value: impl Into<String>,
        quantity: impl Into<String>,
        date_provided: impl Into<String>,
    ) -> Self {
        Self {
            assistance_type_id: assistance_type_id.into(),
            client_id: client_id.into(),
            monetary_value: monetary_value.into(),
            quantity: quantity.into(),
            date_provided: date_provided.into(),
            notes: String::new(),
            client_account_name: String::new(),
            client_account_holder: String::new(),
            client_account_number: String::new(),
            payee_name: String::new(),
            check_requested: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ServWare {
    /// Add an assistance item to a request.
    ///
    /// Sends the full form matching the browser's POST, including empty
    /// optional fields, to avoid server-side validation issues.
    pub async fn update_assistance(
        &self,
        request_id: u64,
        input: &UpdateAssistanceInput,
    ) -> anyhow::Result<()> {
        let url = Self::assistance_item_url(request_id);

        let mut form: Vec<(&str, &str)> = vec![
            ("assistanceTypeId", &input.assistance_type_id),
            ("clientId", &input.client_id),
            ("housingProviderId", ""),
            ("vendorId", ""),
            ("utilityId", ""),
            ("clientAccountId", ""),
            ("clientAccountName", &input.client_account_name),
            ("clientAccountNumber", &input.client_account_number),
            ("clientAccountHolder", &input.client_account_holder),
            ("specialProgramId", ""),
            ("inKindSubType", ""),
            ("monetaryValue", &input.monetary_value),
            ("accountId", ""),
            ("quantity", &input.quantity),
            ("dateProvided", &input.date_provided),
            ("voucherAsstId", ""),
            ("_pending", "on"),
            ("promisedDate", ""),
        ];

        // Spring MVC checkbox convention
        if input.check_requested {
            form.push(("checkRequested", "true"));
        }
        form.push(("_checkRequested", "on"));

        form.push(("datePaid", ""));
        form.push(("checkNumber", ""));
        form.push(("payeeName", &input.payee_name));
        form.push(("notes", &input.notes));
        form.push(("councilPaymentValue", ""));
        form.push(("councilCheckConfNumber", ""));
        form.push(("districtPaymentValue", ""));
        form.push(("districtCheckConfNumber", ""));
        form.push(("otherPaymentValue", ""));
        form.push(("otherCheckConfNumber", ""));
        form.push(("action", "save"));

        tracing::debug!(url, request_id, "posting new assistance item");

        let response = self
            .client
            .post(&url)
            .form(&form)
            .send()
            .await
            .context("add assistance item POST failed")?;

        let status = response.status();
        tracing::debug!(%status, "add assistance item response");

        if !status.is_success() && !status.is_redirection() {
            anyhow::bail!("add assistance item failed with status {status}");
        }

        tracing::info!(request_id, "assistance item added successfully");
        Ok(())
    }
}
