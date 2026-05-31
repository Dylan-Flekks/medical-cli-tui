use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use med_agent::{
    MedicalApprovalPolicy, MedicalToolContext, MedicalToolInvocation, MedicalToolPayload,
    MedicalToolRuntimeRegistry, SaveNoteDraftRequest,
};
use med_core::{
    new_id, Encounter, EncounterId, EncounterStatus, EncounterType, NoteId, NoteSection,
    NoteTemplate, Patient, PatientId,
};
use med_store::LocalStore;
use time::{Date, OffsetDateTime};
use tui_textarea::{CursorMove, Input, Key, TextArea};

#[derive(Debug, Clone)]
pub struct App {
    pub focus: FocusArea,
    pub selected_patient: usize,
    pub selected_tab: WorkspaceTab,
    pub data: DashboardData,
    pub note_editor: TextArea<'static>,
    pub note_draft_id: Option<NoteId>,
    pub note_dirty: bool,
    pub last_message: String,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            focus: FocusArea::PatientQueue,
            selected_patient: 0,
            selected_tab: WorkspaceTab::Chart,
            data: DashboardData::empty(),
            note_editor: default_note_editor(),
            note_draft_id: None,
            note_dirty: false,
            last_message: "Local database not loaded".to_owned(),
            should_quit: false,
        }
    }
}

impl App {
    pub fn from_store(store: &LocalStore) -> Result<Self> {
        let mut app = Self::default();
        app.refresh_from_store(store)?;
        app.last_message = format!("Loaded {} local patients", app.data.patients.len());
        Ok(app)
    }

    #[cfg(test)]
    fn with_data(data: DashboardData) -> Self {
        Self {
            data,
            last_message: "Synthetic dashboard data loaded".to_owned(),
            ..Self::default()
        }
    }

    pub fn refresh_from_store(&mut self, store: &LocalStore) -> Result<()> {
        let preferred_patient_id = self.active_patient().map(|patient| patient.id);
        self.refresh_from_store_with_selection(store, preferred_patient_id)
    }

    pub fn handle_key_with_store(&mut self, key: KeyEvent, store: &LocalStore) -> Result<()> {
        if self.note_editor_active() && is_note_save_key(key) {
            self.save_note_draft(store)?;
            return Ok(());
        }

        if self.note_editor_active() && is_note_editor_input_key(key) {
            self.note_dirty |= self.note_editor.input(note_editor_input(key));
            return Ok(());
        }

        match key.code {
            KeyCode::Char('r') => {
                self.refresh_from_store(store)?;
                self.last_message = "Refreshed local records".to_owned();
            }
            KeyCode::Char('n') => self.create_local_patient(store)?,
            KeyCode::Char('e') => self.create_encounter_for_selected_patient(store)?,
            _ => self.handle_key(key),
        }

        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.note_editor_active() && is_note_editor_input_key(key) {
            self.note_dirty |= self.note_editor.input(note_editor_input(key));
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.focus_next(),
            KeyCode::BackTab => self.focus_previous(),
            KeyCode::Char('1') => self.selected_tab = WorkspaceTab::Chart,
            KeyCode::Char('2') => self.selected_tab = WorkspaceTab::Note,
            KeyCode::Char('3') => self.selected_tab = WorkspaceTab::Audit,
            KeyCode::Char('4') => self.selected_tab = WorkspaceTab::Billing,
            KeyCode::Char('j') | KeyCode::Down => self.select_next_patient(),
            KeyCode::Char('k') | KeyCode::Up => self.select_previous_patient(),
            _ => {}
        }
    }

    pub fn active_patient(&self) -> Option<&PatientQueueItem> {
        self.data.patients.get(self.selected_patient)
    }

    pub fn active_encounter(&self) -> Option<&EncounterItem> {
        self.data.encounters.first()
    }

    pub fn note_editor_active(&self) -> bool {
        self.selected_tab == WorkspaceTab::Note && self.focus == FocusArea::Workspace
    }

    fn refresh_from_store_with_selection(
        &mut self,
        store: &LocalStore,
        preferred_patient_id: Option<PatientId>,
    ) -> Result<()> {
        let patients = store.list_patients()?;
        let mut records = Vec::with_capacity(patients.len());

        for patient in patients {
            let encounters = store.list_encounters_for_patient(patient.id)?;
            records.push((patient, encounters));
        }

        let previous_patient_id = self.active_patient().map(|patient| patient.id);
        self.selected_patient =
            selected_patient_index(&records, preferred_patient_id, self.selected_patient);
        self.data = DashboardData::from_local_records(&records, self.selected_patient);
        let current_patient_id = self.active_patient().map(|patient| patient.id);
        if previous_patient_id != current_patient_id {
            self.reset_note_editor();
        }

        Ok(())
    }

    fn create_local_patient(&mut self, store: &LocalStore) -> Result<()> {
        let id = new_id();
        let now = OffsetDateTime::now_utc();
        let id_text = id.to_string();
        let patient = Patient {
            id,
            medical_record_number: None,
            display_name: format!("New Local Patient {}", &id_text[..8]),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };

        store.insert_patient(&patient)?;
        self.refresh_from_store_with_selection(store, Some(patient.id))?;
        self.last_message = format!("Created local patient {}", patient.display_name);

        Ok(())
    }

    fn create_encounter_for_selected_patient(&mut self, store: &LocalStore) -> Result<()> {
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Create a patient before adding an encounter".to_owned();
            return Ok(());
        };

        let encounter = Encounter {
            id: new_id(),
            patient_id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: OffsetDateTime::now_utc(),
            ended_at: None,
            reason: None,
        };

        store.insert_encounter(&encounter)?;
        self.refresh_from_store_with_selection(store, Some(patient_id))?;
        self.reset_note_editor();
        self.last_message = "Created local encounter".to_owned();

        Ok(())
    }

    fn save_note_draft(&mut self, store: &LocalStore) -> Result<()> {
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Create or select a patient before saving a note".to_owned();
            return Ok(());
        };
        let Some(encounter_id) = self.active_encounter().map(|encounter| encounter.id) else {
            self.last_message = "Create an encounter before saving a note".to_owned();
            return Ok(());
        };

        let tool_name = if self.note_draft_id.is_some() {
            med_agent::MedicalToolName::UpdateNoteDraft
        } else {
            med_agent::MedicalToolName::CreateNoteDraft
        };
        let registry = MedicalToolRuntimeRegistry::default();
        let output = registry.dispatch(MedicalToolInvocation {
            store,
            call_id: format!("tui-note-save-{}", short_id(new_id())),
            tool_name,
            payload: MedicalToolPayload::SaveNoteDraft(SaveNoteDraftRequest {
                note_id: self.note_draft_id,
                patient_id,
                encounter_id,
                template: NoteTemplate::Soap,
                sections: note_sections_from_lines(self.note_editor.lines()),
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::local_default(),
        })?;

        let note_id = output
            .structured
            .get("note_id")
            .cloned()
            .ok_or_else(|| anyhow!("save note tool did not return note_id"))
            .and_then(|value| serde_json::from_value(value).map_err(Into::into))?;
        self.note_draft_id = Some(note_id);
        self.note_dirty = false;
        self.last_message = output.tui_summary;

        Ok(())
    }

    fn reset_note_editor(&mut self) {
        self.note_editor = default_note_editor();
        self.note_draft_id = None;
        self.note_dirty = false;
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            FocusArea::PatientQueue => FocusArea::Workspace,
            FocusArea::Workspace => FocusArea::Context,
            FocusArea::Context => FocusArea::Status,
            FocusArea::Status => FocusArea::PatientQueue,
        };
    }

    fn focus_previous(&mut self) {
        self.focus = match self.focus {
            FocusArea::PatientQueue => FocusArea::Status,
            FocusArea::Workspace => FocusArea::PatientQueue,
            FocusArea::Context => FocusArea::Workspace,
            FocusArea::Status => FocusArea::Context,
        };
    }

    fn select_next_patient(&mut self) {
        if self.data.patients.is_empty() {
            return;
        }

        self.selected_patient = (self.selected_patient + 1) % self.data.patients.len();
        self.reset_note_editor();
        self.last_message = self.selection_message();
    }

    fn select_previous_patient(&mut self) {
        if self.data.patients.is_empty() {
            return;
        }

        self.selected_patient = if self.selected_patient == 0 {
            self.data.patients.len() - 1
        } else {
            self.selected_patient - 1
        };
        self.reset_note_editor();
        self.last_message = self.selection_message();
    }

    fn selection_message(&self) -> String {
        self.active_patient()
            .map(|patient| format!("Selected {}", patient.display_name))
            .unwrap_or_else(|| "No patient selected".to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    PatientQueue,
    Workspace,
    Context,
    Status,
}

impl FocusArea {
    pub fn title(self) -> &'static str {
        match self {
            Self::PatientQueue => "patients",
            Self::Workspace => "workspace",
            Self::Context => "context",
            Self::Status => "status",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTab {
    Chart,
    Note,
    Audit,
    Billing,
}

impl WorkspaceTab {
    pub const ALL: [Self; 4] = [Self::Chart, Self::Note, Self::Audit, Self::Billing];

    pub fn title(self) -> &'static str {
        match self {
            Self::Chart => "Chart",
            Self::Note => "Note",
            Self::Audit => "Audit",
            Self::Billing => "Billing",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Chart => 0,
            Self::Note => 1,
            Self::Audit => 2,
            Self::Billing => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DashboardData {
    pub patients: Vec<PatientQueueItem>,
    pub encounters: Vec<EncounterItem>,
    pub tasks: Vec<TaskItem>,
    pub problems: Vec<String>,
    pub medications: Vec<String>,
    pub allergies: Vec<String>,
    pub audit_flags: Vec<AuditFlagItem>,
    pub billing_rows: Vec<BillingRow>,
    pub vitals_trend: Vec<u64>,
    pub billing_ready_percent: u16,
    pub ai_status: AiStatus,
}

impl DashboardData {
    pub fn empty() -> Self {
        let ai_status = ai_status_from_env();

        Self {
            patients: Vec::new(),
            encounters: Vec::new(),
            tasks: vec![
                TaskItem::info("Local patients", 0),
                TaskItem::info("Open encounters", 0),
                TaskItem::error("Blocked AI calls", ai_blocked_count(ai_status)),
            ],
            problems: Vec::new(),
            medications: Vec::new(),
            allergies: Vec::new(),
            audit_flags: vec![AuditFlagItem::info("No local chart selected")],
            billing_rows: Vec::new(),
            vitals_trend: vec![0],
            billing_ready_percent: 0,
            ai_status,
        }
    }

    fn from_local_records(records: &[(Patient, Vec<Encounter>)], selected_patient: usize) -> Self {
        let ai_status = ai_status_from_env();
        let today = OffsetDateTime::now_utc().date();
        let patients = records
            .iter()
            .map(|(patient, encounters)| patient_queue_item(patient, encounters, today))
            .collect::<Vec<_>>();
        let encounters = records
            .get(selected_patient)
            .map(|(_, encounters)| {
                encounters
                    .iter()
                    .map(encounter_item)
                    .collect::<Vec<EncounterItem>>()
            })
            .unwrap_or_default();
        let open_encounter_count = records
            .iter()
            .flat_map(|(_, encounters)| encounters.iter())
            .filter(|encounter| is_open_encounter(&encounter.status))
            .count();

        Self {
            patients,
            encounters,
            tasks: vec![
                TaskItem::info("Local patients", records.len()),
                TaskItem::warning("Open encounters", open_encounter_count),
                TaskItem::error("Blocked AI calls", ai_blocked_count(ai_status)),
            ],
            problems: Vec::new(),
            medications: Vec::new(),
            allergies: Vec::new(),
            audit_flags: vec![
                AuditFlagItem::warning("Structured note audit pending"),
                AuditFlagItem::blocked("AI PHI request has no executed BAA"),
            ],
            billing_rows: Vec::new(),
            vitals_trend: vec![0],
            billing_ready_percent: if open_encounter_count > 0 { 25 } else { 0 },
            ai_status,
        }
    }

    #[cfg(test)]
    fn synthetic() -> Self {
        let patient_a = new_id();
        let patient_b = new_id();
        let patient_c = new_id();
        let encounter = new_id();

        Self {
            patients: vec![
                PatientQueueItem {
                    id: patient_a,
                    display_name: "Synthetic Patient A".to_owned(),
                    mrn: "MRN-0001".to_owned(),
                    age: Some(42),
                    status: "unsigned note".to_owned(),
                },
                PatientQueueItem {
                    id: patient_b,
                    display_name: "Synthetic Patient B".to_owned(),
                    mrn: "MRN-0002".to_owned(),
                    age: Some(58),
                    status: "billing flag".to_owned(),
                },
                PatientQueueItem {
                    id: patient_c,
                    display_name: "Synthetic Patient C".to_owned(),
                    mrn: "MRN-0003".to_owned(),
                    age: None,
                    status: "ready".to_owned(),
                },
            ],
            encounters: vec![EncounterItem {
                id: encounter,
                short_id: short_id(encounter),
                started_at: "2026-05-28".to_owned(),
                encounter_type: "Office visit".to_owned(),
                status: "In progress".to_owned(),
                reason: "-".to_owned(),
            }],
            tasks: vec![
                TaskItem::warning("Unsigned notes", 2),
                TaskItem::error("Billing flags", 3),
                TaskItem::info("AI blocked", 1),
            ],
            problems: vec![
                "Low back pain".to_owned(),
                "Hypertension".to_owned(),
                "Medication review due".to_owned(),
            ],
            medications: vec!["Lisinopril 10 mg".to_owned(), "Ibuprofen PRN".to_owned()],
            allergies: vec!["NKDA".to_owned()],
            audit_flags: vec![
                AuditFlagItem::warning("Assessment missing linked diagnosis"),
                AuditFlagItem::warning("Procedure code lacks supporting note section"),
                AuditFlagItem::info("Note is still unsigned"),
                AuditFlagItem::blocked("AI PHI request has no executed BAA"),
            ],
            billing_rows: vec![
                BillingRow {
                    code: "M54.50".to_owned(),
                    kind: "ICD-10-CM".to_owned(),
                    status: "linked".to_owned(),
                },
                BillingRow {
                    code: "97110".to_owned(),
                    kind: "CPT".to_owned(),
                    status: "needs note support".to_owned(),
                },
            ],
            vitals_trend: vec![98, 99, 97, 101, 100, 99, 98, 97],
            billing_ready_percent: 42,
            ai_status: ai_status_from_env(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatientQueueItem {
    pub id: PatientId,
    pub display_name: String,
    pub mrn: String,
    pub age: Option<u8>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct EncounterItem {
    pub id: EncounterId,
    pub short_id: String,
    pub started_at: String,
    pub encounter_type: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct TaskItem {
    pub label: String,
    pub count: usize,
    pub severity: Severity,
}

impl TaskItem {
    fn info(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Info,
        }
    }

    fn warning(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Warning,
        }
    }

    fn error(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditFlagItem {
    pub message: String,
    pub severity: Severity,
}

impl AuditFlagItem {
    fn info(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Info,
        }
    }

    fn warning(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Warning,
        }
    }

    fn blocked(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Blocked,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BillingRow {
    pub code: String,
    pub kind: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Blocked,
}

#[derive(Debug, Clone, Copy)]
pub enum AiStatus {
    Locked,
    Allowed,
}

fn selected_patient_index(
    records: &[(Patient, Vec<Encounter>)],
    preferred_patient_id: Option<PatientId>,
    fallback_index: usize,
) -> usize {
    if records.is_empty() {
        return 0;
    }

    preferred_patient_id
        .and_then(|patient_id| {
            records
                .iter()
                .position(|(patient, _)| patient.id == patient_id)
        })
        .unwrap_or_else(|| fallback_index.min(records.len() - 1))
}

fn patient_queue_item(
    patient: &Patient,
    encounters: &[Encounter],
    today: Date,
) -> PatientQueueItem {
    PatientQueueItem {
        id: patient.id,
        display_name: patient.display_name.clone(),
        mrn: patient
            .medical_record_number
            .clone()
            .unwrap_or_else(|| "-".to_owned()),
        age: patient
            .date_of_birth
            .and_then(|date_of_birth| age_on(date_of_birth, today)),
        status: patient_status(encounters),
    }
}

fn encounter_item(encounter: &Encounter) -> EncounterItem {
    EncounterItem {
        id: encounter.id,
        short_id: short_id(encounter.id),
        started_at: encounter.started_at.date().to_string(),
        encounter_type: encounter_type_label(&encounter.encounter_type),
        status: encounter_status_label(&encounter.status).to_owned(),
        reason: encounter.reason.clone().unwrap_or_else(|| "-".to_owned()),
    }
}

fn short_id(id: impl std::fmt::Display) -> String {
    id.to_string()[..8].to_owned()
}

fn patient_status(encounters: &[Encounter]) -> String {
    if encounters.is_empty() {
        return "no encounters".to_owned();
    }

    if encounters
        .iter()
        .any(|encounter| is_open_encounter(&encounter.status))
    {
        return "open encounter".to_owned();
    }

    "ready".to_owned()
}

fn is_open_encounter(status: &EncounterStatus) -> bool {
    matches!(
        status,
        EncounterStatus::Planned | EncounterStatus::InProgress
    )
}

fn encounter_type_label(encounter_type: &EncounterType) -> String {
    match encounter_type {
        EncounterType::OfficeVisit => "Office visit".to_owned(),
        EncounterType::Telehealth => "Telehealth".to_owned(),
        EncounterType::Procedure => "Procedure".to_owned(),
        EncounterType::Phone => "Phone".to_owned(),
        EncounterType::Administrative => "Administrative".to_owned(),
        EncounterType::Other(label) => label.clone(),
    }
}

fn encounter_status_label(status: &EncounterStatus) -> &'static str {
    match status {
        EncounterStatus::Planned => "Planned",
        EncounterStatus::InProgress => "In progress",
        EncounterStatus::Finished => "Finished",
        EncounterStatus::Cancelled => "Cancelled",
    }
}

fn age_on(date_of_birth: Date, today: Date) -> Option<u8> {
    let mut age = today.year() - date_of_birth.year();

    if today.ordinal() < date_of_birth.ordinal() {
        age -= 1;
    }

    u8::try_from(age).ok()
}

fn ai_status_from_env() -> AiStatus {
    if std::env::var_os("MEDCLI_TUI_DEMO_AI_ALLOWED").is_some() {
        AiStatus::Allowed
    } else {
        AiStatus::Locked
    }
}

fn ai_blocked_count(ai_status: AiStatus) -> usize {
    match ai_status {
        AiStatus::Locked => 1,
        AiStatus::Allowed => 0,
    }
}

fn default_note_editor() -> TextArea<'static> {
    let mut textarea = TextArea::from([
        "Subjective:",
        "",
        "Objective:",
        "",
        "Assessment:",
        "",
        "Plan:",
        "",
    ]);
    textarea.move_cursor(CursorMove::Jump(1, 0));
    textarea
}

fn is_note_save_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_note_editor_input_key(key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }

    !matches!(key.code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab)
}

fn note_editor_input(key: KeyEvent) -> Input {
    if key.kind == KeyEventKind::Release {
        return Input::default();
    }

    let textarea_key = match key.code {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Null,
        KeyCode::F(value) => Key::F(value),
        KeyCode::Char(value) => Key::Char(value),
        KeyCode::Esc
        | KeyCode::BackTab
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => Key::Null,
    };

    Input {
        key: textarea_key,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}

fn note_sections_from_lines(lines: &[String]) -> Vec<NoteSection> {
    const HEADINGS: [&str; 4] = ["Subjective", "Objective", "Assessment", "Plan"];

    let mut buckets: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    let mut current_section = 0;

    for line in lines {
        if let Some(index) = soap_heading_index(line) {
            current_section = index;
            continue;
        }

        buckets[current_section].push(line.clone());
    }

    HEADINGS
        .iter()
        .enumerate()
        .map(|(index, heading)| NoteSection {
            heading: (*heading).to_owned(),
            body: note_section_body(&buckets[index]),
            required: true,
        })
        .collect()
}

fn soap_heading_index(line: &str) -> Option<usize> {
    match line
        .trim()
        .trim_end_matches(':')
        .to_ascii_lowercase()
        .as_str()
    {
        "subjective" => Some(0),
        "objective" => Some(1),
        "assessment" => Some(2),
        "plan" => Some(3),
        _ => None,
    }
}

fn note_section_body(lines: &[String]) -> String {
    let mut start = 0;
    let mut end = lines.len();

    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }

    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    lines[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use med_store::LocalStore;
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn temp_store() -> (LocalStore, PathBuf) {
        let path = std::env::temp_dir().join(format!("flekks-med-tui-test-{}.db", new_id()));
        let store = LocalStore::open_encrypted(&path, "test-key").unwrap();
        (store, path)
    }

    fn cleanup(path: PathBuf) {
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn number_keys_select_workspace_tabs() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.selected_tab, WorkspaceTab::Note);

        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.selected_tab, WorkspaceTab::Audit);

        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.selected_tab, WorkspaceTab::Billing);
    }

    #[test]
    fn patient_selection_wraps() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_patient, app.data.patients.len() - 1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_patient, 0);
    }

    #[test]
    fn tab_moves_focus() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, FocusArea::Workspace);
    }

    #[test]
    fn note_editor_accepts_text_when_note_workspace_is_active() {
        let mut app = App::with_data(DashboardData::synthetic());
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;

        app.handle_key(key(KeyCode::Char('x')));

        assert_eq!(app.note_editor.lines()[1], "x");
        assert!(app.note_dirty);
    }

    #[test]
    fn loads_patients_from_store() {
        let (store, path) = temp_store();

        let now = OffsetDateTime::now_utc();
        store
            .insert_patient(&Patient {
                id: new_id(),
                medical_record_number: Some("MRN-SYNTH-001".to_owned()),
                display_name: "Synthetic Store Patient".to_owned(),
                date_of_birth: None,
                sex_at_birth: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap();

        let app = App::from_store(&store).unwrap();

        assert_eq!(app.data.patients.len(), 1);
        assert_eq!(app.data.patients[0].display_name, "Synthetic Store Patient");

        drop(store);
        cleanup(path);
    }

    #[test]
    fn create_local_patient_persists_and_selects_it() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();

        let patients = store.list_patients().unwrap();
        assert_eq!(patients.len(), 1);
        assert_eq!(app.data.patients.len(), 1);
        assert_eq!(app.active_patient().unwrap().id, patients[0].id);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn create_encounter_persists_for_selected_patient() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        let patient_id = app.active_patient().unwrap().id;

        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();

        let encounters = store.list_encounters_for_patient(patient_id).unwrap();
        assert_eq!(encounters.len(), 1);
        assert_eq!(app.data.encounters.len(), 1);
        assert_eq!(app.data.encounters[0].short_id, short_id(encounters[0].id));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn save_note_draft_persists_note_for_active_encounter() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        app.note_editor = TextArea::from([
            "Subjective:",
            "Synthetic subjective text",
            "Objective:",
            "Synthetic objective text",
            "Assessment:",
            "Synthetic assessment text",
            "Plan:",
            "Synthetic plan text",
        ]);
        app.note_dirty = true;

        let before = store.audit_event_count().unwrap();
        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();

        let note_id = app.note_draft_id.unwrap();
        let note = store.get_note(note_id).unwrap().unwrap();

        assert_eq!(note.patient_id, app.active_patient().unwrap().id);
        assert_eq!(note.encounter_id, app.active_encounter().unwrap().id);
        assert_eq!(note.sections.len(), 4);
        assert_eq!(note.sections[0].heading, "Subjective");
        assert_eq!(note.sections[0].body, "Synthetic subjective text");
        assert!(!app.note_dirty);
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }
}
