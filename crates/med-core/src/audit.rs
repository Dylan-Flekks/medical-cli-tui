use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

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
    BillingCodeChanged,
    ExportCreated,
    BackupCreated,
    AiRequestAllowed,
    AiRequestBlocked,
    ComplianceVendorChanged,
    FailedUnlock,
}
