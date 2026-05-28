use med_compliance::{ComplianceBlock, VendorComplianceRecord};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

pub trait AiProvider {
    fn id(&self) -> &str;
    fn capabilities(&self) -> AiCapabilities;
    fn draft_note(&self, request: AiDraftRequest) -> Result<AiDraftResponse, AiError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCapabilities {
    pub supports_streaming: bool,
    pub supports_structured_output: bool,
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDraftRequest {
    pub contains_phi: bool,
    pub service_name: String,
    pub instruction: String,
    pub clinical_context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDraftResponse {
    pub generated_text: String,
    pub created_at: OffsetDateTime,
    pub requires_human_review: bool,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI request blocked by compliance policy: {0}")]
    Compliance(#[from] ComplianceBlock),

    #[error("provider error: {0}")]
    Provider(String),
}

pub fn preflight_ai_request(
    vendor: Option<&VendorComplianceRecord>,
    request: &AiDraftRequest,
    now: time::Date,
) -> Result<(), ComplianceBlock> {
    if !request.contains_phi {
        return Ok(());
    }

    let vendor = vendor.ok_or(ComplianceBlock::MissingVendorRecord)?;
    vendor.can_send_phi_to_service(&request.service_name, now)
}

pub struct NoopProvider;

impl AiProvider for NoopProvider {
    fn id(&self) -> &str {
        "noop-local"
    }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            supports_streaming: false,
            supports_structured_output: false,
            service_name: "local-noop".to_owned(),
        }
    }

    fn draft_note(&self, _request: AiDraftRequest) -> Result<AiDraftResponse, AiError> {
        Ok(AiDraftResponse {
            generated_text: "Noop provider: connect a compliant provider to generate drafts."
                .to_owned(),
            created_at: OffsetDateTime::now_utc(),
            requires_human_review: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_phi_without_vendor_record() {
        let request = AiDraftRequest {
            contains_phi: true,
            service_name: "api-model".to_owned(),
            instruction: "draft".to_owned(),
            clinical_context: "synthetic test context".to_owned(),
        };

        let result = preflight_ai_request(
            None,
            &request,
            time::Date::from_calendar_date(2026, time::Month::May, 28).unwrap(),
        );

        assert!(matches!(result, Err(ComplianceBlock::MissingVendorRecord)));
    }
}
