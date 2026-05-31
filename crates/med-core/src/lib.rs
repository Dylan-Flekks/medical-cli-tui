pub mod audit;
pub mod billing;
pub mod clinical;
pub mod ids;

pub use audit::{
    audit_documentation, AuditAction, AuditEvent, DocumentationAuditFlag, DocumentationAuditReport,
    DocumentationAuditSeverity,
};
pub use billing::{BillingAuditFlag, ClaimDraft, DiagnosisCode, ProcedureCode};
pub use clinical::{
    Allergy, ClinicalNote, Encounter, EncounterStatus, EncounterType, Medication, NoteSection,
    NoteStatus, NoteTemplate, Observation, Patient, Problem,
};
pub use ids::{new_id, AttachmentId, EncounterId, NoteId, PatientId, PractitionerId, VendorId};
