use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone)]
pub struct App {
    pub focus: FocusArea,
    pub selected_patient: usize,
    pub selected_tab: WorkspaceTab,
    pub data: DashboardData,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            focus: FocusArea::PatientQueue,
            selected_patient: 0,
            selected_tab: WorkspaceTab::Chart,
            data: DashboardData::demo(),
            should_quit: false,
        }
    }
}

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
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
    fn demo() -> Self {
        Self {
            patients: vec![
                PatientQueueItem {
                    display_name: "Jane Example".to_owned(),
                    mrn: "MRN-0001".to_owned(),
                    age: Some(42),
                    status: "unsigned note".to_owned(),
                },
                PatientQueueItem {
                    display_name: "Sam Sample".to_owned(),
                    mrn: "MRN-0002".to_owned(),
                    age: Some(58),
                    status: "billing flag".to_owned(),
                },
                PatientQueueItem {
                    display_name: "Avery Demo".to_owned(),
                    mrn: "MRN-0003".to_owned(),
                    age: None,
                    status: "ready".to_owned(),
                },
            ],
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
            ai_status: if std::env::var_os("MEDCLI_TUI_DEMO_AI_ALLOWED").is_some() {
                AiStatus::Allowed
            } else {
                AiStatus::Locked
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatientQueueItem {
    pub display_name: String,
    pub mrn: String,
    pub age: Option<u8>,
    pub status: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn number_keys_select_workspace_tabs() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.selected_tab, WorkspaceTab::Note);

        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.selected_tab, WorkspaceTab::Audit);

        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.selected_tab, WorkspaceTab::Billing);
    }

    #[test]
    fn patient_selection_wraps() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_patient, app.data.patients.len() - 1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_patient, 0);
    }

    #[test]
    fn tab_moves_focus() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, FocusArea::Workspace);
    }
}
