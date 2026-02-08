use anyhow::Context;


use super::ServWare;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Fields for adding a new assistance item to a request.
pub struct UpdateAssistanceInput {
    // Required
    pub assistance_type_id: String,
    pub client_id: String,
    pub monetary_value: String,
    pub quantity: String,
    pub date_provided: String,

    // Optional
    pub promised_date: String,
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
            promised_date: String::new(),
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
    pub async fn update_assistance(
        &self,
        request_id: u64,
        input: &UpdateAssistanceInput,
    ) -> anyhow::Result<()> {
        let url = Self::assistance_item_url(request_id);

        let mut form: Vec<(&str, &str)> = vec![
            ("assistanceTypeId", &input.assistance_type_id),
            ("clientId", &input.client_id),
            ("monetaryValue", &input.monetary_value),
            ("quantity", &input.quantity),
            ("dateProvided", &input.date_provided),
            ("promisedDate", &input.promised_date),
            ("notes", &input.notes),
            ("clientAccountName", &input.client_account_name),
            ("clientAccountHolder", &input.client_account_holder),
            ("clientAccountNumber", &input.client_account_number),
            ("payeeName", &input.payee_name),
        ];

        // Spring MVC checkbox convention
        if input.check_requested {
            form.push(("checkRequested", "true"));
        }
        form.push(("_checkRequested", "on"));
        form.push(("_pending", "on"));
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
