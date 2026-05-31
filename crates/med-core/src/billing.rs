use serde::{Deserialize, Serialize};

use crate::audit::{DocumentationAuditReport, DocumentationAuditSeverity};
use crate::clinical::{ClinicalNote, NoteStatus};
use crate::ids::{EncounterId, PatientId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosisCode {
    pub system: DiagnosisSystem,
    pub code: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosisSystem {
    Icd10Cm,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureCode {
    pub system: ProcedureSystem,
    pub code: String,
    pub description: Option<String>,
    pub modifiers: Vec<String>,
    pub units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcedureSystem {
    Cpt,
    Hcpcs,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimDraftStatus {
    Draft,
    NeedsReview,
    Ready,
    Exported,
    Voided,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingAuditFlag {
    pub severity: AuditSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimReadinessReport {
    pub patient_id: PatientId,
    pub encounter_id: EncounterId,
    pub status: ClaimDraftStatus,
    pub flags: Vec<ClaimReadinessFlag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimReadinessFlag {
    pub severity: ClaimReadinessSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimReadinessSeverity {
    Info,
    Warning,
    Error,
}

impl ClaimDraft {
    pub fn placeholder(patient_id: PatientId, encounter_id: EncounterId) -> Self {
        Self {
            patient_id,
            encounter_id,
            diagnoses: vec![DiagnosisCode {
                system: DiagnosisSystem::Icd10Cm,
                code: "TBD".to_owned(),
                description: Some("Diagnosis placeholder; requires human coding review".to_owned()),
            }],
            procedures: vec![ProcedureCode {
                system: ProcedureSystem::Cpt,
                code: "TBD".to_owned(),
                description: Some("Procedure placeholder; requires human coding review".to_owned()),
                modifiers: Vec::new(),
                units: 1,
            }],
            place_of_service: None,
            rendering_provider_npi: None,
            payer_name: None,
            status: ClaimDraftStatus::NeedsReview,
        }
    }

    pub fn ensure_placeholders(&mut self) {
        if self.diagnoses.is_empty() {
            self.diagnoses.push(DiagnosisCode {
                system: DiagnosisSystem::Icd10Cm,
                code: "TBD".to_owned(),
                description: Some("Diagnosis placeholder; requires human coding review".to_owned()),
            });
        }

        if self.procedures.is_empty() {
            self.procedures.push(ProcedureCode {
                system: ProcedureSystem::Cpt,
                code: "TBD".to_owned(),
                description: Some("Procedure placeholder; requires human coding review".to_owned()),
                modifiers: Vec::new(),
                units: 1,
            });
        }
    }
}

impl ClaimReadinessFlag {
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ClaimReadinessSeverity::Info,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ClaimReadinessSeverity::Warning,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ClaimReadinessSeverity::Error,
            code: code.into(),
            message: message.into(),
        }
    }
}

pub fn assess_claim_readiness(
    claim: &ClaimDraft,
    note: Option<&ClinicalNote>,
    documentation_audit: Option<&DocumentationAuditReport>,
) -> ClaimReadinessReport {
    let mut flags = Vec::new();

    match note {
        Some(note) if matches!(note.status, NoteStatus::Signed) => flags.push(
            ClaimReadinessFlag::info("signed_note_present", "Signed note is available"),
        ),
        Some(_) => flags.push(ClaimReadinessFlag::error(
            "missing_signed_note",
            "A signed note is required before billing review",
        )),
        None => flags.push(ClaimReadinessFlag::error(
            "missing_signed_note",
            "No signed note is available for this encounter",
        )),
    }

    if documentation_audit.is_some_and(|report| {
        report
            .flags
            .iter()
            .any(|flag| flag.severity == DocumentationAuditSeverity::Error)
    }) {
        flags.push(ClaimReadinessFlag::error(
            "documentation_audit_blocker",
            "Documentation audit has blocking findings",
        ));
    }

    if claim
        .diagnoses
        .iter()
        .any(|diagnosis| is_placeholder(&diagnosis.code))
    {
        flags.push(ClaimReadinessFlag::warning(
            "diagnosis_placeholder",
            "Diagnosis code placeholder requires human coding review",
        ));
    }

    if claim
        .procedures
        .iter()
        .any(|procedure| is_placeholder(&procedure.code))
    {
        flags.push(ClaimReadinessFlag::warning(
            "procedure_placeholder",
            "Procedure code placeholder requires human coding review",
        ));
    }

    if claim.diagnoses.is_empty() {
        flags.push(ClaimReadinessFlag::warning(
            "missing_diagnosis",
            "Add at least one diagnosis code placeholder",
        ));
    }

    if claim.procedures.is_empty() {
        flags.push(ClaimReadinessFlag::warning(
            "missing_procedure",
            "Add at least one procedure code placeholder",
        ));
    }

    let status = if flags
        .iter()
        .any(|flag| flag.severity == ClaimReadinessSeverity::Error)
    {
        ClaimDraftStatus::NeedsReview
    } else if flags
        .iter()
        .any(|flag| flag.severity == ClaimReadinessSeverity::Warning)
    {
        ClaimDraftStatus::NeedsReview
    } else {
        ClaimDraftStatus::Ready
    };

    ClaimReadinessReport {
        patient_id: claim.patient_id,
        encounter_id: claim.encounter_id,
        status,
        flags,
    }
}

fn is_placeholder(code: &str) -> bool {
    code.trim().is_empty() || code.eq_ignore_ascii_case("TBD")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{new_id, ClinicalNote, NoteSection, NoteTemplate};
    use time::OffsetDateTime;

    fn claim() -> ClaimDraft {
        ClaimDraft::placeholder(new_id(), new_id())
    }

    fn signed_note_for(claim: &ClaimDraft) -> ClinicalNote {
        let now = OffsetDateTime::now_utc();
        ClinicalNote {
            id: new_id(),
            patient_id: claim.patient_id,
            encounter_id: claim.encounter_id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Signed,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Synthetic text".to_owned(),
                required: true,
            }],
            created_at: now,
            updated_at: now,
            signed_at: Some(now),
            version: 1,
        }
    }

    #[test]
    fn missing_signed_note_blocks_claim_readiness() {
        let claim = claim();

        let report = assess_claim_readiness(&claim, None, None);

        assert_eq!(report.status, ClaimDraftStatus::NeedsReview);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "missing_signed_note"));
    }

    #[test]
    fn placeholder_codes_keep_claim_in_review() {
        let claim = claim();
        let note = signed_note_for(&claim);

        let report = assess_claim_readiness(&claim, Some(&note), None);

        assert_eq!(report.status, ClaimDraftStatus::NeedsReview);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "diagnosis_placeholder"));
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "procedure_placeholder"));
    }

    #[test]
    fn coded_signed_claim_is_ready() {
        let mut claim = claim();
        claim.diagnoses[0].code = "M54.50".to_owned();
        claim.procedures[0].code = "97110".to_owned();
        let note = signed_note_for(&claim);

        let report = assess_claim_readiness(&claim, Some(&note), None);

        assert_eq!(report.status, ClaimDraftStatus::Ready);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "signed_note_present"));
    }
}
