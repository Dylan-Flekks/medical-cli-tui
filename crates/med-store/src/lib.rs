use std::path::Path;

use med_core::AuditEvent;
use rusqlite::{Connection, OpenFlags};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
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
        Ok(())
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
