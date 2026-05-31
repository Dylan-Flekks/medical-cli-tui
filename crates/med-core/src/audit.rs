use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::clinical::{ClinicalNote, Encounter, NoteStatus, NoteTemplate};
use crate::ids::{EncounterId, NoteId, PatientId, PractitionerId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub actor_id: Option<PractitionerId>,
    pub patient_id: Option<PatientId>,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub action: AuditAction,
    pub occurred_at: OffsetDateTime,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    ChartOpened,
    PatientSearched,
    NoteCreated,
    NoteEdited,
    NoteSigned,
    NoteAmended,
    DocumentationAuditRan,
    BillingCodeChanged,
    ExportCreated,
    BackupCreated,
    AiRequestAllowed,
    AiRequestBlocked,
    ComplianceVendorChanged,
    FailedUnlock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentationAuditReport {
    pub patient_id: PatientId,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub flags: Vec<DocumentationAuditFlag>,
    pub billing_ready_percent: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentationAuditFlag {
    pub severity: DocumentationAuditSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentationAuditSeverity {
    Info,
    Warning,
    Error,
}

pub fn audit_documentation(
    patient_id: PatientId,
    encounter: Option<&Encounter>,
    note: Option<&ClinicalNote>,
    now: OffsetDateTime,
) -> DocumentationAuditReport {
    let mut flags = Vec::new();

    if encounter.is_none() {
        flags.push(DocumentationAuditFlag::error(
            "no_encounter",
            "Create or select an encounter before documentation review",
        ));
    }

    let note_id = note.map(|note| note.id);

    match (encounter, note) {
        (Some(_), None) => flags.push(DocumentationAuditFlag::error(
            "no_note",
            "No clinical note exists for the active encounter",
        )),
        (Some(_), Some(note)) => audit_note(note, now, &mut flags),
        (None, Some(note)) => audit_note(note, now, &mut flags),
        (None, None) => {}
    }

    let billing_ready_percent = billing_ready_percent(note, &flags);

    DocumentationAuditReport {
        patient_id,
        encounter_id: encounter.map(|encounter| encounter.id),
        note_id,
        flags,
        billing_ready_percent,
    }
}

impl DocumentationAuditFlag {
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DocumentationAuditSeverity::Info,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DocumentationAuditSeverity::Warning,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DocumentationAuditSeverity::Error,
            code: code.into(),
            message: message.into(),
        }
    }
}

fn audit_note(note: &ClinicalNote, now: OffsetDateTime, flags: &mut Vec<DocumentationAuditFlag>) {
    for heading in required_note_headings(note) {
        match note
            .sections
            .iter()
            .find(|section| section.heading.eq_ignore_ascii_case(heading))
        {
            Some(section) if !section.body.trim().is_empty() => {}
            Some(_) => flags.push(DocumentationAuditFlag::error(
                format!("blank_section_{}", audit_code_part(heading)),
                format!("{heading} section is blank"),
            )),
            None => flags.push(DocumentationAuditFlag::error(
                format!("missing_section_{}", audit_code_part(heading)),
                format!("{heading} section is missing"),
            )),
        }
    }

    match note.status {
        NoteStatus::Draft => {
            flags.push(DocumentationAuditFlag::warning(
                "unsigned_draft",
                "Clinical note is still unsigned",
            ));
            if now - note.updated_at >= Duration::hours(24) {
                flags.push(DocumentationAuditFlag::warning(
                    "stale_draft",
                    "Clinical note draft is older than 24 hours",
                ));
            }
        }
        NoteStatus::Reviewed => flags.push(DocumentationAuditFlag::warning(
            "reviewed_unsigned",
            "Reviewed clinical note is not signed",
        )),
        NoteStatus::Signed => flags.push(DocumentationAuditFlag::info(
            "signed_note_ready",
            "Signed note is immutable and ready for billing review",
        )),
        NoteStatus::Amended => flags.push(DocumentationAuditFlag::info(
            "amended_note",
            "Amended note is available for review",
        )),
        NoteStatus::Voided => flags.push(DocumentationAuditFlag::error(
            "voided_note",
            "Voided note cannot support billing readiness",
        )),
    }
}

fn required_note_headings(note: &ClinicalNote) -> Vec<&str> {
    if matches!(note.template, NoteTemplate::Soap) {
        return vec!["Subjective", "Objective", "Assessment", "Plan"];
    }

    note.sections
        .iter()
        .filter(|section| section.required)
        .map(|section| section.heading.as_str())
        .collect()
}

fn billing_ready_percent(note: Option<&ClinicalNote>, flags: &[DocumentationAuditFlag]) -> u16 {
    if note.is_none()
        || flags
            .iter()
            .any(|flag| flag.code == "no_encounter" || flag.code == "no_note")
    {
        return 0;
    }

    if flags
        .iter()
        .any(|flag| flag.severity == DocumentationAuditSeverity::Error)
    {
        return 25;
    }

    let has_warnings = flags
        .iter()
        .any(|flag| flag.severity == DocumentationAuditSeverity::Warning);
    let Some(note) = note else {
        return 0;
    };

    if matches!(note.status, NoteStatus::Signed) {
        if has_warnings {
            75
        } else {
            100
        }
    } else {
        50
    }
}

fn audit_code_part(heading: &str) -> String {
    heading
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{new_id, EncounterStatus, EncounterType, NoteSection};

    fn soap_note(status: NoteStatus, updated_at: OffsetDateTime) -> ClinicalNote {
        ClinicalNote {
            id: new_id(),
            patient_id: new_id(),
            encounter_id: new_id(),
            author_id: None,
            template: NoteTemplate::Soap,
            status,
            sections: vec![
                section("Subjective", "Patient feels better"),
                section("Objective", "Vitals stable"),
                section("Assessment", "Improving"),
                section("Plan", "Continue home exercises"),
            ],
            created_at: updated_at,
            updated_at,
            signed_at: None,
            version: 1,
        }
    }

    fn section(heading: &str, body: &str) -> NoteSection {
        NoteSection {
            heading: heading.to_owned(),
            body: body.to_owned(),
            required: true,
        }
    }

    fn encounter_for(note: &ClinicalNote) -> Encounter {
        Encounter {
            id: note.encounter_id,
            patient_id: note.patient_id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: note.created_at,
            ended_at: None,
            reason: None,
        }
    }

    #[test]
    fn complete_signed_note_is_billing_ready() {
        let now = OffsetDateTime::now_utc();
        let mut note = soap_note(NoteStatus::Signed, now);
        note.signed_at = Some(now);
        let encounter = encounter_for(&note);

        let report = audit_documentation(note.patient_id, Some(&encounter), Some(&note), now);

        assert_eq!(report.billing_ready_percent, 100);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "signed_note_ready"));
    }

    #[test]
    fn blank_required_soap_section_limits_readiness() {
        let now = OffsetDateTime::now_utc();
        let mut note = soap_note(NoteStatus::Signed, now);
        note.sections[2].body.clear();
        let encounter = encounter_for(&note);

        let report = audit_documentation(note.patient_id, Some(&encounter), Some(&note), now);

        assert_eq!(report.billing_ready_percent, 25);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "blank_section_assessment"));
    }

    #[test]
    fn missing_required_soap_section_is_flagged() {
        let now = OffsetDateTime::now_utc();
        let mut note = soap_note(NoteStatus::Draft, now);
        note.sections.retain(|section| section.heading != "Plan");
        let encounter = encounter_for(&note);

        let report = audit_documentation(note.patient_id, Some(&encounter), Some(&note), now);

        assert_eq!(report.billing_ready_percent, 25);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "missing_section_plan"));
    }

    #[test]
    fn stale_unsigned_draft_is_warning_but_half_ready() {
        let now = OffsetDateTime::now_utc();
        let note = soap_note(NoteStatus::Draft, now - Duration::hours(25));
        let encounter = encounter_for(&note);

        let report = audit_documentation(note.patient_id, Some(&encounter), Some(&note), now);

        assert_eq!(report.billing_ready_percent, 50);
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "unsigned_draft"));
        assert!(report.flags.iter().any(|flag| flag.code == "stale_draft"));
    }
}
