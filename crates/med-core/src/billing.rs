use serde::{Deserialize, Serialize};

use crate::ids::{EncounterId, PatientId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisCode {
    pub system: DiagnosisSystem,
    pub code: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosisSystem {
    Icd10Cm,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureCode {
    pub system: ProcedureSystem,
    pub code: String,
    pub description: Option<String>,
    pub modifiers: Vec<String>,
    pub units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcedureSystem {
    Cpt,
    Hcpcs,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimDraft {
    pub patient_id: PatientId,
    pub encounter_id: EncounterId,
    pub diagnoses: Vec<DiagnosisCode>,
    pub procedures: Vec<ProcedureCode>,
    pub place_of_service: Option<String>,
    pub rendering_provider_npi: Option<String>,
    pub payer_name: Option<String>,
    pub status: ClaimDraftStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClaimDraftStatus {
    Draft,
    NeedsReview,
    Ready,
    Exported,
    Voided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingAuditFlag {
    pub severity: AuditSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
}
