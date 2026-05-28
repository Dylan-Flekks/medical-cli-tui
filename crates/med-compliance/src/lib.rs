use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Date, OffsetDateTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorComplianceRecord {
    pub provider: String,
    pub phi_allowed: bool,
    pub baa: BaaRecord,
    pub approval: ComplianceApproval,
}

impl VendorComplianceRecord {
    pub fn can_send_phi_to_service(&self, service: &str, now: Date) -> Result<(), ComplianceBlock> {
        if !self.phi_allowed {
            return Err(ComplianceBlock::PhiNotAllowed {
                provider: self.provider.clone(),
            });
        }

        if self.baa.status != BaaStatus::Executed {
            return Err(ComplianceBlock::BaaNotExecuted {
                provider: self.provider.clone(),
                status: self.baa.status.clone(),
            });
        }

        if let Some(expires_at) = self.baa.expires_at {
            if expires_at < now {
                return Err(ComplianceBlock::BaaExpired {
                    provider: self.provider.clone(),
                    expired_at: expires_at,
                });
            }
        }

        if !self
            .baa
            .covered_services
            .iter()
            .any(|covered| covered == service)
        {
            return Err(ComplianceBlock::ServiceNotCovered {
                provider: self.provider.clone(),
                service: service.to_owned(),
            });
        }

        if !self.approval.approved {
            return Err(ComplianceBlock::NotApproved {
                provider: self.provider.clone(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaaStatus {
    Missing,
    Pending,
    Executed,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaaRecord {
    pub status: BaaStatus,
    pub signed_at: Option<Date>,
    pub expires_at: Option<Date>,
    pub covered_services: Vec<String>,
    pub document_sha256: Option<String>,
    pub secure_document_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceApproval {
    pub approved: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<OffsetDateTime>,
    pub notes: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ComplianceBlock {
    #[error("provider {provider} is not allowed to receive PHI")]
    PhiNotAllowed { provider: String },

    #[error("provider {provider} does not have executed BAA status: {status:?}")]
    BaaNotExecuted { provider: String, status: BaaStatus },

    #[error("provider {provider} BAA expired at {expired_at}")]
    BaaExpired { provider: String, expired_at: Date },

    #[error("service {service} is not covered by provider {provider} BAA")]
    ServiceNotCovered { provider: String, service: String },

    #[error("provider {provider} has not been approved for PHI use")]
    NotApproved { provider: String },

    #[error("PHI request has no vendor compliance record")]
    MissingVendorRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_phi_when_baa_is_missing() {
        let record = VendorComplianceRecord {
            provider: "example-ai".to_owned(),
            phi_allowed: false,
            baa: BaaRecord {
                status: BaaStatus::Missing,
                signed_at: None,
                expires_at: None,
                covered_services: vec![],
                document_sha256: None,
                secure_document_uri: None,
            },
            approval: ComplianceApproval {
                approved: false,
                approved_by: None,
                approved_at: None,
                notes: None,
            },
        };

        let result = record.can_send_phi_to_service(
            "example-model",
            Date::from_calendar_date(2026, time::Month::May, 28).unwrap(),
        );

        assert!(matches!(result, Err(ComplianceBlock::PhiNotAllowed { .. })));
    }
}
