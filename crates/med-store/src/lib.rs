use std::path::Path;

use med_core::{
    AuditEvent, ClaimDraft, ClaimDraftStatus, ClinicalNote, DiagnosisCode, Encounter,
    EncounterStatus, EncounterType, NoteId, NoteSection, NoteStatus, NoteTemplate, Patient,
    PatientId, PractitionerId, ProcedureCode,
};
use rusqlite::{params, Connection, OpenFlags};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("time parse error: {0}")]
    TimeParse(#[from] time::error::Parse),

    #[error("time format error: {0}")]
    TimeFormat(#[from] time::error::Format),

    #[error("uuid parse error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("invalid date value: {0}")]
    InvalidDate(String),

    #[error("invalid note version value: {0}")]
    InvalidNoteVersion(i64),

    #[error("note not found: {0}")]
    NoteNotFound(NoteId),

    #[error("signed note is immutable: {0}")]
    SignedNoteImmutable(NoteId),

    #[error("note {note_id} cannot be signed because status is {status}")]
    NoteNotSignable { note_id: NoteId, status: String },
}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct LocalStore {
    connection: Connection,
}

impl LocalStore {
    pub fn open_encrypted(path: impl AsRef<Path>, key: &str) -> StoreResult<Self> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )?;

        #[cfg(feature = "sqlcipher")]
        connection.pragma_update(None, "key", key)?;

        #[cfg(not(feature = "sqlcipher"))]
        let _ = key;

        connection.pragma_update(None, "foreign_keys", "ON")?;

        let store = Self { connection };
        store.apply_schema()?;
        Ok(store)
    }

    pub fn apply_schema(&self) -> StoreResult<()> {
        self.connection.execute_batch(SCHEMA)?;
        self.ensure_column("notes", "author_id", "text")?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> StoreResult<()> {
        let mut statement = self
            .connection
            .prepare(&format!("pragma table_info({table})"))?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(());
            }
        }

        self.connection.execute(
            &format!("alter table {table} add column {column} {definition}"),
            [],
        )?;
        Ok(())
    }

    pub fn insert_patient(&self, patient: &Patient) -> StoreResult<()> {
        self.connection.execute(
            "insert into patients (
                id, medical_record_number, display_name, date_of_birth, sex_at_birth, created_at, updated_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            on conflict(id) do update set
                medical_record_number = excluded.medical_record_number,
                display_name = excluded.display_name,
                date_of_birth = excluded.date_of_birth,
                sex_at_birth = excluded.sex_at_birth,
                updated_at = excluded.updated_at",
            params![
                patient.id.to_string(),
                patient.medical_record_number.as_deref(),
                patient.display_name.as_str(),
                patient.date_of_birth.map(format_date),
                patient.sex_at_birth.as_deref(),
                format_offset_date_time(patient.created_at)?,
                format_offset_date_time(patient.updated_at)?,
            ],
        )?;

        Ok(())
    }

    pub fn get_patient(&self, id: PatientId) -> StoreResult<Option<Patient>> {
        let mut statement = self.connection.prepare(
            "select id, medical_record_number, display_name, date_of_birth, sex_at_birth, created_at, updated_at
             from patients
             where id = ?1",
        )?;
        let mut rows = statement.query(params![id.to_string()])?;

        if let Some(row) = rows.next()? {
            return Ok(Some(patient_from_row(row)?));
        }

        Ok(None)
    }

    pub fn list_patients(&self) -> StoreResult<Vec<Patient>> {
        let mut statement = self.connection.prepare(
            "select id, medical_record_number, display_name, date_of_birth, sex_at_birth, created_at, updated_at
             from patients
             order by display_name collate nocase",
        )?;
        let mut rows = statement.query([])?;
        let mut patients = Vec::new();

        while let Some(row) = rows.next()? {
            patients.push(patient_from_row(row)?);
        }

        Ok(patients)
    }

    pub fn insert_encounter(&self, encounter: &Encounter) -> StoreResult<()> {
        self.connection.execute(
            "insert into encounters (
                id, patient_id, practitioner_id, encounter_type, status, started_at, ended_at, reason
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            on conflict(id) do update set
                patient_id = excluded.patient_id,
                practitioner_id = excluded.practitioner_id,
                encounter_type = excluded.encounter_type,
                status = excluded.status,
                started_at = excluded.started_at,
                ended_at = excluded.ended_at,
                reason = excluded.reason",
            params![
                encounter.id.to_string(),
                encounter.patient_id.to_string(),
                encounter.practitioner_id.map(|id| id.to_string()),
                serde_json::to_string(&encounter.encounter_type)?,
                serde_json::to_string(&encounter.status)?,
                format_offset_date_time(encounter.started_at)?,
                encounter
                    .ended_at
                    .map(format_offset_date_time)
                    .transpose()?,
                encounter.reason.as_deref(),
            ],
        )?;

        Ok(())
    }

    pub fn list_encounters_for_patient(
        &self,
        patient_id: PatientId,
    ) -> StoreResult<Vec<Encounter>> {
        let mut statement = self.connection.prepare(
            "select id, patient_id, practitioner_id, encounter_type, status, started_at, ended_at, reason
             from encounters
             where patient_id = ?1
             order by started_at desc",
        )?;
        let mut rows = statement.query(params![patient_id.to_string()])?;
        let mut encounters = Vec::new();

        while let Some(row) = rows.next()? {
            encounters.push(encounter_from_row(row)?);
        }

        Ok(encounters)
    }

    pub fn upsert_note(&self, note: &ClinicalNote) -> StoreResult<()> {
        if let Some(existing_status) = self.note_status(note.id)? {
            if matches!(existing_status, NoteStatus::Signed) {
                return Err(StoreError::SignedNoteImmutable(note.id));
            }
        }

        self.connection.execute(
            "insert into notes (
                id, patient_id, encounter_id, author_id, template, status, version, created_at, updated_at, signed_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            on conflict(id) do update set
                patient_id = excluded.patient_id,
                encounter_id = excluded.encounter_id,
                author_id = excluded.author_id,
                template = excluded.template,
                status = excluded.status,
                version = excluded.version,
                updated_at = excluded.updated_at,
                signed_at = excluded.signed_at",
            params![
                note.id.to_string(),
                note.patient_id.to_string(),
                note.encounter_id.to_string(),
                note.author_id.map(|id| id.to_string()),
                serde_json::to_string(&note.template)?,
                serde_json::to_string(&note.status)?,
                i64::from(note.version),
                format_offset_date_time(note.created_at)?,
                format_offset_date_time(note.updated_at)?,
                note.signed_at.map(format_offset_date_time).transpose()?,
            ],
        )?;

        self.connection.execute(
            "delete from note_sections where note_id = ?1",
            params![note.id.to_string()],
        )?;

        for (index, section) in note.sections.iter().enumerate() {
            self.connection.execute(
                "insert into note_sections (
                    note_id, section_order, heading, body, required
                ) values (?1, ?2, ?3, ?4, ?5)",
                params![
                    note.id.to_string(),
                    i64::try_from(index).unwrap_or(i64::MAX),
                    section.heading.as_str(),
                    section.body.as_str(),
                    if section.required { 1 } else { 0 },
                ],
            )?;
        }

        Ok(())
    }

    pub fn sign_note_draft(
        &self,
        note_id: NoteId,
        signed_at: OffsetDateTime,
    ) -> StoreResult<ClinicalNote> {
        let note = self
            .get_note(note_id)?
            .ok_or(StoreError::NoteNotFound(note_id))?;

        if !matches!(note.status, NoteStatus::Draft) {
            return Err(StoreError::NoteNotSignable {
                note_id,
                status: note_status_label(&note.status).to_owned(),
            });
        }

        self.connection.execute(
            "update notes
             set status = ?1, updated_at = ?2, signed_at = ?3
             where id = ?4",
            params![
                serde_json::to_string(&NoteStatus::Signed)?,
                format_offset_date_time(signed_at)?,
                format_offset_date_time(signed_at)?,
                note_id.to_string(),
            ],
        )?;

        self.get_note(note_id)?
            .ok_or(StoreError::NoteNotFound(note_id))
    }

    pub fn get_note(&self, id: NoteId) -> StoreResult<Option<ClinicalNote>> {
        let mut statement = self.connection.prepare(
            "select id, patient_id, encounter_id, author_id, template, status, version, created_at, updated_at, signed_at
             from notes
             where id = ?1",
        )?;
        let mut rows = statement.query(params![id.to_string()])?;

        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let note_id: String = row.get(0)?;
        let patient_id: String = row.get(1)?;
        let encounter_id: String = row.get(2)?;
        let author_id: Option<String> = row.get(3)?;
        let template: String = row.get(4)?;
        let status: String = row.get(5)?;
        let version: i64 = row.get(6)?;
        let created_at: String = row.get(7)?;
        let updated_at: String = row.get(8)?;
        let signed_at: Option<String> = row.get(9)?;

        Ok(Some(ClinicalNote {
            id: parse_uuid(&note_id)?,
            patient_id: parse_uuid(&patient_id)?,
            encounter_id: parse_uuid(&encounter_id)?,
            author_id: author_id
                .as_deref()
                .map(parse_practitioner_id)
                .transpose()?,
            template: serde_json::from_str::<NoteTemplate>(&template)?,
            status: serde_json::from_str::<NoteStatus>(&status)?,
            sections: self.list_note_sections(id)?,
            created_at: parse_offset_date_time(&created_at)?,
            updated_at: parse_offset_date_time(&updated_at)?,
            signed_at: signed_at
                .as_deref()
                .map(parse_offset_date_time)
                .transpose()?,
            version: u32::try_from(version).map_err(|_| StoreError::InvalidNoteVersion(version))?,
        }))
    }

    pub fn list_notes_for_encounter(
        &self,
        encounter_id: med_core::EncounterId,
    ) -> StoreResult<Vec<ClinicalNote>> {
        let mut statement = self.connection.prepare(
            "select id, patient_id, encounter_id, author_id, template, status, version, created_at, updated_at, signed_at
             from notes
             where encounter_id = ?1
             order by updated_at desc, created_at desc",
        )?;
        let mut rows = statement.query(params![encounter_id.to_string()])?;
        let mut notes = Vec::new();

        while let Some(row) = rows.next()? {
            notes.push(self.clinical_note_from_row(row)?);
        }

        Ok(notes)
    }

    pub fn latest_draft_note_for_encounter(
        &self,
        encounter_id: med_core::EncounterId,
    ) -> StoreResult<Option<ClinicalNote>> {
        let draft_status = serde_json::to_string(&NoteStatus::Draft)?;
        let mut statement = self.connection.prepare(
            "select id, patient_id, encounter_id, author_id, template, status, version, created_at, updated_at, signed_at
             from notes
             where encounter_id = ?1 and status = ?2
             order by updated_at desc, created_at desc
             limit 1",
        )?;
        let mut rows = statement.query(params![encounter_id.to_string(), draft_status])?;

        if let Some(row) = rows.next()? {
            return Ok(Some(self.clinical_note_from_row(row)?));
        }

        Ok(None)
    }

    fn clinical_note_from_row(&self, row: &rusqlite::Row<'_>) -> StoreResult<ClinicalNote> {
        let note_id: String = row.get(0)?;
        let patient_id: String = row.get(1)?;
        let encounter_id: String = row.get(2)?;
        let author_id: Option<String> = row.get(3)?;
        let template: String = row.get(4)?;
        let status: String = row.get(5)?;
        let version: i64 = row.get(6)?;
        let created_at: String = row.get(7)?;
        let updated_at: String = row.get(8)?;
        let signed_at: Option<String> = row.get(9)?;

        let id = parse_uuid(&note_id)?;

        Ok(ClinicalNote {
            id,
            patient_id: parse_uuid(&patient_id)?,
            encounter_id: parse_uuid(&encounter_id)?,
            author_id: author_id
                .as_deref()
                .map(parse_practitioner_id)
                .transpose()?,
            template: serde_json::from_str::<NoteTemplate>(&template)?,
            status: serde_json::from_str::<NoteStatus>(&status)?,
            sections: self.list_note_sections(id)?,
            created_at: parse_offset_date_time(&created_at)?,
            updated_at: parse_offset_date_time(&updated_at)?,
            signed_at: signed_at
                .as_deref()
                .map(parse_offset_date_time)
                .transpose()?,
            version: u32::try_from(version).map_err(|_| StoreError::InvalidNoteVersion(version))?,
        })
    }

    fn note_status(&self, note_id: NoteId) -> StoreResult<Option<NoteStatus>> {
        let mut statement = self
            .connection
            .prepare("select status from notes where id = ?1")?;
        let mut rows = statement.query(params![note_id.to_string()])?;

        if let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            return Ok(Some(serde_json::from_str::<NoteStatus>(&status)?));
        }

        Ok(None)
    }

    fn list_note_sections(&self, note_id: NoteId) -> StoreResult<Vec<NoteSection>> {
        let mut statement = self.connection.prepare(
            "select heading, body, required
             from note_sections
             where note_id = ?1
             order by section_order asc",
        )?;
        let mut rows = statement.query(params![note_id.to_string()])?;
        let mut sections = Vec::new();

        while let Some(row) = rows.next()? {
            let required: i64 = row.get(2)?;
            sections.push(NoteSection {
                heading: row.get(0)?,
                body: row.get(1)?,
                required: required != 0,
            });
        }

        Ok(sections)
    }

    pub fn append_audit_event(&self, event: &AuditEvent) -> StoreResult<()> {
        self.connection.execute(
            "insert into audit_events (
                id, actor_id, patient_id, encounter_id, note_id, action, occurred_at, details_json
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                event.id.to_string(),
                event.actor_id.map(|id| id.to_string()),
                event.patient_id.map(|id| id.to_string()),
                event.encounter_id.map(|id| id.to_string()),
                event.note_id.map(|id| id.to_string()),
                format!("{:?}", event.action),
                event.occurred_at.to_string(),
                event.details.to_string(),
            ),
        )?;

        Ok(())
    }

    pub fn audit_event_count(&self) -> StoreResult<usize> {
        let count: i64 =
            self.connection
                .query_row("select count(*) from audit_events", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    pub fn upsert_claim_draft(&self, claim: &ClaimDraft) -> StoreResult<()> {
        self.connection.execute(
            "insert into claim_drafts (
                encounter_id, patient_id, diagnoses_json, procedures_json,
                place_of_service, rendering_provider_npi, payer_name, status, updated_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            on conflict(encounter_id) do update set
                patient_id = excluded.patient_id,
                diagnoses_json = excluded.diagnoses_json,
                procedures_json = excluded.procedures_json,
                place_of_service = excluded.place_of_service,
                rendering_provider_npi = excluded.rendering_provider_npi,
                payer_name = excluded.payer_name,
                status = excluded.status,
                updated_at = excluded.updated_at",
            params![
                claim.encounter_id.to_string(),
                claim.patient_id.to_string(),
                serde_json::to_string(&claim.diagnoses)?,
                serde_json::to_string(&claim.procedures)?,
                claim.place_of_service.as_deref(),
                claim.rendering_provider_npi.as_deref(),
                claim.payer_name.as_deref(),
                serde_json::to_string(&claim.status)?,
                format_offset_date_time(OffsetDateTime::now_utc())?,
            ],
        )?;

        Ok(())
    }

    pub fn get_claim_draft(
        &self,
        encounter_id: med_core::EncounterId,
    ) -> StoreResult<Option<ClaimDraft>> {
        let mut statement = self.connection.prepare(
            "select patient_id, encounter_id, diagnoses_json, procedures_json,
                    place_of_service, rendering_provider_npi, payer_name, status
             from claim_drafts
             where encounter_id = ?1",
        )?;
        let mut rows = statement.query(params![encounter_id.to_string()])?;

        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        claim_draft_from_row(row).map(Some)
    }
}

fn patient_from_row(row: &rusqlite::Row<'_>) -> StoreResult<Patient> {
    let id: String = row.get(0)?;
    let date_of_birth: Option<String> = row.get(3)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    Ok(Patient {
        id: parse_uuid(&id)?,
        medical_record_number: row.get(1)?,
        display_name: row.get(2)?,
        date_of_birth: date_of_birth.as_deref().map(parse_date).transpose()?,
        sex_at_birth: row.get(4)?,
        created_at: parse_offset_date_time(&created_at)?,
        updated_at: parse_offset_date_time(&updated_at)?,
    })
}

fn encounter_from_row(row: &rusqlite::Row<'_>) -> StoreResult<Encounter> {
    let id: String = row.get(0)?;
    let patient_id: String = row.get(1)?;
    let practitioner_id: Option<String> = row.get(2)?;
    let encounter_type: String = row.get(3)?;
    let status: String = row.get(4)?;
    let started_at: String = row.get(5)?;
    let ended_at: Option<String> = row.get(6)?;

    Ok(Encounter {
        id: parse_uuid(&id)?,
        patient_id: parse_uuid(&patient_id)?,
        practitioner_id: practitioner_id.as_deref().map(parse_uuid).transpose()?,
        encounter_type: serde_json::from_str::<EncounterType>(&encounter_type)?,
        status: serde_json::from_str::<EncounterStatus>(&status)?,
        started_at: parse_offset_date_time(&started_at)?,
        ended_at: ended_at
            .as_deref()
            .map(parse_offset_date_time)
            .transpose()?,
        reason: row.get(7)?,
    })
}

fn claim_draft_from_row(row: &rusqlite::Row<'_>) -> StoreResult<ClaimDraft> {
    let patient_id: String = row.get(0)?;
    let encounter_id: String = row.get(1)?;
    let diagnoses_json: String = row.get(2)?;
    let procedures_json: String = row.get(3)?;
    let status: String = row.get(7)?;

    Ok(ClaimDraft {
        patient_id: parse_uuid(&patient_id)?,
        encounter_id: parse_uuid(&encounter_id)?,
        diagnoses: serde_json::from_str::<Vec<DiagnosisCode>>(&diagnoses_json)?,
        procedures: serde_json::from_str::<Vec<ProcedureCode>>(&procedures_json)?,
        place_of_service: row.get(4)?,
        rendering_provider_npi: row.get(5)?,
        payer_name: row.get(6)?,
        status: serde_json::from_str::<ClaimDraftStatus>(&status)?,
    })
}

fn parse_uuid(value: &str) -> StoreResult<Uuid> {
    Ok(Uuid::parse_str(value)?)
}

fn parse_practitioner_id(value: &str) -> StoreResult<PractitionerId> {
    parse_uuid(value)
}

fn note_status_label(status: &NoteStatus) -> &'static str {
    match status {
        NoteStatus::Draft => "Draft",
        NoteStatus::Reviewed => "Reviewed",
        NoteStatus::Signed => "Signed",
        NoteStatus::Amended => "Amended",
        NoteStatus::Voided => "Voided",
    }
}

fn format_offset_date_time(value: OffsetDateTime) -> StoreResult<String> {
    Ok(value.format(&Rfc3339)?)
}

fn parse_offset_date_time(value: &str) -> StoreResult<OffsetDateTime> {
    Ok(OffsetDateTime::parse(value, &Rfc3339)?)
}

fn format_date(value: Date) -> String {
    value.to_string()
}

fn parse_date(value: &str) -> StoreResult<Date> {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .ok_or_else(|| StoreError::InvalidDate(value.to_owned()))?
        .parse::<i32>()
        .map_err(|_| StoreError::InvalidDate(value.to_owned()))?;
    let month = parts
        .next()
        .ok_or_else(|| StoreError::InvalidDate(value.to_owned()))?
        .parse::<u8>()
        .map_err(|_| StoreError::InvalidDate(value.to_owned()))?;
    let day = parts
        .next()
        .ok_or_else(|| StoreError::InvalidDate(value.to_owned()))?
        .parse::<u8>()
        .map_err(|_| StoreError::InvalidDate(value.to_owned()))?;

    if parts.next().is_some() {
        return Err(StoreError::InvalidDate(value.to_owned()));
    }

    let month =
        time::Month::try_from(month).map_err(|_| StoreError::InvalidDate(value.to_owned()))?;

    Date::from_calendar_date(year, month, day)
        .map_err(|_| StoreError::InvalidDate(value.to_owned()))
}

const SCHEMA: &str = r#"
create table if not exists patients (
    id text primary key,
    medical_record_number text,
    display_name text not null,
    date_of_birth text,
    sex_at_birth text,
    created_at text not null,
    updated_at text not null
);

create table if not exists encounters (
    id text primary key,
    patient_id text not null references patients(id),
    practitioner_id text,
    encounter_type text not null,
    status text not null,
    started_at text not null,
    ended_at text,
    reason text
);

create table if not exists notes (
    id text primary key,
    patient_id text not null references patients(id),
    encounter_id text not null references encounters(id),
    author_id text,
    template text not null,
    status text not null,
    version integer not null,
    created_at text not null,
    updated_at text not null,
    signed_at text
);

create table if not exists note_sections (
    note_id text not null references notes(id),
    section_order integer not null,
    heading text not null,
    body text not null,
    required integer not null,
    primary key (note_id, section_order)
);

create table if not exists compliance_vendors (
    provider text primary key,
    phi_allowed integer not null,
    baa_status text not null,
    covered_services_json text not null,
    approved integer not null,
    updated_at text not null
);

create table if not exists audit_events (
    id text primary key,
    actor_id text,
    patient_id text,
    encounter_id text,
    note_id text,
    action text not null,
    occurred_at text not null,
    details_json text not null
);

create table if not exists claim_drafts (
    encounter_id text primary key references encounters(id),
    patient_id text not null references patients(id),
    diagnoses_json text not null,
    procedures_json text not null,
    place_of_service text,
    rendering_provider_npi text,
    payer_name text,
    status text not null,
    updated_at text not null
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use med_core::{new_id, EncounterStatus, EncounterType};

    fn open_test_store() -> LocalStore {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        let store = LocalStore { connection };
        store.apply_schema().unwrap();
        store
    }

    #[test]
    fn stores_and_lists_patients() {
        let store = open_test_store();
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: Some("MRN-SYNTH-001".to_owned()),
            display_name: "Synthetic Patient".to_owned(),
            date_of_birth: Some(Date::from_calendar_date(1984, time::Month::April, 12).unwrap()),
            sex_at_birth: Some("unknown".to_owned()),
            created_at: now,
            updated_at: now,
        };

        store.insert_patient(&patient).unwrap();
        let patients = store.list_patients().unwrap();

        assert_eq!(patients.len(), 1);
        assert_eq!(patients[0].id, patient.id);
        assert_eq!(patients[0].display_name, "Synthetic Patient");
        assert_eq!(patients[0].date_of_birth, patient.date_of_birth);
    }

    #[test]
    fn stores_and_lists_encounters_for_patient() {
        let store = open_test_store();
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: None,
            display_name: "Encounter Test Patient".to_owned(),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };
        let encounter = Encounter {
            id: new_id(),
            patient_id: patient.id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: now,
            ended_at: None,
            reason: Some("Synthetic follow-up".to_owned()),
        };

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();
        let encounters = store.list_encounters_for_patient(patient.id).unwrap();

        assert_eq!(encounters.len(), 1);
        assert_eq!(encounters[0].id, encounter.id);
        assert!(matches!(
            encounters[0].encounter_type,
            EncounterType::OfficeVisit
        ));
    }

    #[test]
    fn stores_and_updates_claim_draft_for_encounter() {
        let store = open_test_store();
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: None,
            display_name: "Billing Test Patient".to_owned(),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };
        let encounter = Encounter {
            id: new_id(),
            patient_id: patient.id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: now,
            ended_at: None,
            reason: Some("Synthetic billing review".to_owned()),
        };
        let mut claim = ClaimDraft::placeholder(patient.id, encounter.id);

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();
        store.upsert_claim_draft(&claim).unwrap();
        claim.diagnoses[0].code = "M54.50".to_owned();
        claim.status = ClaimDraftStatus::Ready;
        store.upsert_claim_draft(&claim).unwrap();

        let loaded = store.get_claim_draft(encounter.id).unwrap().unwrap();

        assert_eq!(loaded.patient_id, patient.id);
        assert_eq!(loaded.encounter_id, encounter.id);
        assert_eq!(loaded.diagnoses[0].code, "M54.50");
        assert_eq!(loaded.status, ClaimDraftStatus::Ready);
    }

    #[test]
    fn lists_encounter_notes_and_selects_latest_draft() {
        let store = open_test_store();
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: None,
            display_name: "Note Query Patient".to_owned(),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };
        let encounter = Encounter {
            id: new_id(),
            patient_id: patient.id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: now,
            ended_at: None,
            reason: Some("Synthetic note query".to_owned()),
        };
        let old_draft = ClinicalNote {
            id: new_id(),
            patient_id: patient.id,
            encounter_id: encounter.id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Draft,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Old draft".to_owned(),
                required: true,
            }],
            created_at: now,
            updated_at: now,
            signed_at: None,
            version: 1,
        };
        let signed_note = ClinicalNote {
            id: new_id(),
            patient_id: patient.id,
            encounter_id: encounter.id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Signed,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Signed note".to_owned(),
                required: true,
            }],
            created_at: now + time::Duration::minutes(1),
            updated_at: now + time::Duration::minutes(3),
            signed_at: Some(now + time::Duration::minutes(3)),
            version: 1,
        };
        let latest_draft = ClinicalNote {
            id: new_id(),
            patient_id: patient.id,
            encounter_id: encounter.id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Draft,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Latest draft".to_owned(),
                required: true,
            }],
            created_at: now + time::Duration::minutes(2),
            updated_at: now + time::Duration::minutes(4),
            signed_at: None,
            version: 2,
        };

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();
        store.upsert_note(&old_draft).unwrap();
        store.upsert_note(&signed_note).unwrap();
        store.upsert_note(&latest_draft).unwrap();

        let notes = store.list_notes_for_encounter(encounter.id).unwrap();
        let loaded_latest_draft = store
            .latest_draft_note_for_encounter(encounter.id)
            .unwrap()
            .unwrap();

        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].id, latest_draft.id);
        assert_eq!(loaded_latest_draft.id, latest_draft.id);
        assert_eq!(loaded_latest_draft.sections[0].body, "Latest draft");
    }

    #[test]
    fn signs_draft_note_and_blocks_later_updates() {
        let store = open_test_store();
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: None,
            display_name: "Sign Test Patient".to_owned(),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };
        let encounter = Encounter {
            id: new_id(),
            patient_id: patient.id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: now,
            ended_at: None,
            reason: Some("Synthetic signing".to_owned()),
        };
        let mut note = ClinicalNote {
            id: new_id(),
            patient_id: patient.id,
            encounter_id: encounter.id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Draft,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Draft body".to_owned(),
                required: true,
            }],
            created_at: now,
            updated_at: now,
            signed_at: None,
            version: 1,
        };

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();
        store.upsert_note(&note).unwrap();

        let signed_at = now + time::Duration::minutes(2);
        let signed = store.sign_note_draft(note.id, signed_at).unwrap();

        assert!(matches!(signed.status, NoteStatus::Signed));
        assert_eq!(signed.signed_at, Some(signed_at));

        note.sections[0].body = "Changed after signing".to_owned();
        let error = store.upsert_note(&note).unwrap_err();

        assert!(matches!(error, StoreError::SignedNoteImmutable(id) if id == note.id));
    }
}
