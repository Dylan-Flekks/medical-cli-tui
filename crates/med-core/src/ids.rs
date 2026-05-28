use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type PatientId = Uuid;
pub type EncounterId = Uuid;
pub type NoteId = Uuid;
pub type PractitionerId = Uuid;
pub type AttachmentId = Uuid;
pub type VendorId = String;

pub fn new_id() -> Uuid {
    Uuid::new_v4()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MedicalRecordNumber(pub String);
