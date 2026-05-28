use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::ids::{EncounterId, NoteId, PatientId, PractitionerId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patient {
    pub id: PatientId,
    pub medical_record_number: Option<String>,
    pub display_name: String,
    pub date_of_birth: Option<Date>,
    pub sex_at_birth: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encounter {
    pub id: EncounterId,
    pub patient_id: PatientId,
    pub practitioner_id: Option<PractitionerId>,
    pub encounter_type: EncounterType,
    pub status: EncounterStatus,
    pub started_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncounterType {
    OfficeVisit,
    Telehealth,
    Procedure,
    Phone,
    Administrative,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncounterStatus {
    Planned,
    InProgress,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClinicalNote {
    pub id: NoteId,
    pub patient_id: PatientId,
    pub encounter_id: EncounterId,
    pub author_id: Option<PractitionerId>,
    pub template: NoteTemplate,
    pub status: NoteStatus,
    pub sections: Vec<NoteSection>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub signed_at: Option<OffsetDateTime>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteTemplate {
    Soap,
    HistoryAndPhysical,
    Progress,
    Procedure,
    Discharge,
    Telephone,
    BillingAddendum,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteStatus {
    Draft,
    Reviewed,
    Signed,
    Amended,
    Voided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSection {
    pub heading: String,
    pub body: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub patient_id: PatientId,
    pub code: Option<String>,
    pub description: String,
    pub status: ProblemStatus,
    pub onset_date: Option<Date>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProblemStatus {
    Active,
    Inactive,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Medication {
    pub patient_id: PatientId,
    pub name: String,
    pub dose: Option<String>,
    pub route: Option<String>,
    pub frequency: Option<String>,
    pub status: MedicationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MedicationStatus {
    Active,
    Discontinued,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allergy {
    pub patient_id: PatientId,
    pub substance: String,
    pub reaction: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub patient_id: PatientId,
    pub encounter_id: Option<EncounterId>,
    pub code: String,
    pub display: String,
    pub value: ObservationValue,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObservationValue {
    Quantity {
        value: f64,
        unit: String,
    },
    Text(String),
    Code {
        code: String,
        display: Option<String>,
    },
}
