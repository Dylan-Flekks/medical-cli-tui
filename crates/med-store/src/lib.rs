use std::path::Path;

use med_core::{
    AuditEvent, ClinicalNote, Encounter, EncounterStatus, EncounterType, NoteId, NoteSection,
    NoteStatus, NoteTemplate, Patient, PatientId, PractitionerId,
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

fn parse_uuid(value: &str) -> StoreResult<Uuid> {
    Ok(Uuid::parse_str(value)?)
}

fn parse_practitioner_id(value: &str) -> StoreResult<PractitionerId> {
    parse_uuid(value)
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
}
