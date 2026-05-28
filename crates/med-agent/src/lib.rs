use std::collections::HashMap;

use med_ai::{preflight_ai_request, AiDraftRequest};
use med_compliance::{ComplianceBlock, VendorComplianceRecord};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Date, OffsetDateTime};

#[derive(Debug, Clone)]
pub struct MedicalAgentHarness {
    tools: MedicalToolRegistry,
    vendors: HashMap<String, VendorComplianceRecord>,
}

impl Default for MedicalAgentHarness {
    fn default() -> Self {
        Self {
            tools: MedicalToolRegistry::with_default_tools(),
            vendors: HashMap::new(),
        }
    }
}

impl MedicalAgentHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_vendor(&mut self, vendor: VendorComplianceRecord) {
        self.vendors.insert(vendor.provider.clone(), vendor);
    }

    pub fn tools(&self) -> &MedicalToolRegistry {
        &self.tools
    }

    pub fn start_turn(
        &self,
        request: AgentTurnRequest,
        today: Date,
    ) -> Result<AgentTurnOutcome, AgentHarnessError> {
        if let Some(provider) = &request.outbound_provider {
            let vendor = self.vendors.get(&provider.provider_id);
            let ai_request = AiDraftRequest {
                contains_phi: request.contains_phi,
                service_name: provider.service_name.clone(),
                instruction: request.instruction.clone(),
                clinical_context: request.context_summary.clone().unwrap_or_default(),
            };

            preflight_ai_request(vendor, &ai_request, today)?;
        }

        let planned_tools = self.plan_tools(&request)?;
        Ok(AgentTurnOutcome {
            state: AgentState::ReadyForLocalTools,
            events: vec![AgentEvent {
                occurred_at: OffsetDateTime::now_utc(),
                kind: AgentEventKind::TurnAccepted,
                message: "agent turn accepted after policy preflight".to_owned(),
            }],
            planned_tools,
            requires_human_review: true,
        })
    }

    fn plan_tools(
        &self,
        request: &AgentTurnRequest,
    ) -> Result<Vec<ToolCallPlan>, AgentHarnessError> {
        let requested_tools = if request.requested_tools.is_empty() {
            MedicalToolName::default_turn_tools()
        } else {
            request.requested_tools.clone()
        };

        requested_tools
            .into_iter()
            .map(|name| {
                let spec = self
                    .tools
                    .get(&name)
                    .ok_or_else(|| AgentHarnessError::UnknownTool(name.clone()))?;

                if spec.outbound && request.outbound_provider.is_none() {
                    return Err(AgentHarnessError::OutboundProviderRequired(name));
                }

                Ok(ToolCallPlan {
                    name,
                    requires_human_review: spec.requires_human_review,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurnRequest {
    pub instruction: String,
    pub contains_phi: bool,
    pub context_summary: Option<String>,
    pub outbound_provider: Option<OutboundProvider>,
    pub requested_tools: Vec<MedicalToolName>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundProvider {
    pub provider_id: String,
    pub service_name: String,
}

impl OutboundProvider {
    pub fn openai(service_name: impl Into<String>) -> Self {
        Self {
            provider_id: "openai".to_owned(),
            service_name: service_name.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurnOutcome {
    pub state: AgentState,
    pub events: Vec<AgentEvent>,
    pub planned_tools: Vec<ToolCallPlan>,
    pub requires_human_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallPlan {
    pub name: MedicalToolName,
    pub requires_human_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub occurred_at: OffsetDateTime,
    pub kind: AgentEventKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    CheckingPolicy,
    ReadyForLocalTools,
    WaitingForHumanApproval,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEventKind {
    TurnAccepted,
    ToolPlanned,
    PolicyBlocked,
    HumanReviewRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MedicalToolName {
    SearchPatients,
    ReadPatientSummary,
    ListEncounters,
    CreateNoteDraft,
    UpdateNoteDraft,
    RunDocumentationAudit,
    PrepareSuperbillDraft,
    CheckVendorBaa,
    DraftNoteWithOpenAi,
}

impl MedicalToolName {
    fn default_turn_tools() -> Vec<Self> {
        vec![
            Self::ReadPatientSummary,
            Self::ListEncounters,
            Self::RunDocumentationAudit,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SearchPatients => "chart.search_patients",
            Self::ReadPatientSummary => "chart.read_patient_summary",
            Self::ListEncounters => "chart.list_encounters",
            Self::CreateNoteDraft => "note.create_draft",
            Self::UpdateNoteDraft => "note.update_draft",
            Self::RunDocumentationAudit => "note.run_documentation_audit",
            Self::PrepareSuperbillDraft => "billing.prepare_superbill_draft",
            Self::CheckVendorBaa => "compliance.check_vendor_baa",
            Self::DraftNoteWithOpenAi => "ai.draft_note_with_openai",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MedicalToolRegistry {
    tools: HashMap<MedicalToolName, MedicalToolSpec>,
}

impl MedicalToolRegistry {
    pub fn with_default_tools() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        registry.register(MedicalToolSpec::local(
            MedicalToolName::SearchPatients,
            "Search local patient records",
        ));
        registry.register(MedicalToolSpec::local(
            MedicalToolName::ReadPatientSummary,
            "Read a local chart summary",
        ));
        registry.register(MedicalToolSpec::local(
            MedicalToolName::ListEncounters,
            "List local encounters for a patient",
        ));
        registry.register(MedicalToolSpec::reviewed_local(
            MedicalToolName::CreateNoteDraft,
            "Create a local structured note draft",
        ));
        registry.register(MedicalToolSpec::reviewed_local(
            MedicalToolName::UpdateNoteDraft,
            "Update a local structured note draft",
        ));
        registry.register(MedicalToolSpec::local(
            MedicalToolName::RunDocumentationAudit,
            "Run local documentation audit rules",
        ));
        registry.register(MedicalToolSpec::reviewed_local(
            MedicalToolName::PrepareSuperbillDraft,
            "Prepare a local billing-support draft",
        ));
        registry.register(MedicalToolSpec::local(
            MedicalToolName::CheckVendorBaa,
            "Check local vendor BAA and approval status",
        ));
        registry.register(MedicalToolSpec::outbound(
            MedicalToolName::DraftNoteWithOpenAi,
            "Draft documentation with OpenAI after BAA preflight",
        ));

        registry
    }

    pub fn register(&mut self, spec: MedicalToolSpec) {
        self.tools.insert(spec.name.clone(), spec);
    }

    pub fn get(&self, name: &MedicalToolName) -> Option<&MedicalToolSpec> {
        self.tools.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &MedicalToolSpec> {
        self.tools.values()
    }
}

#[derive(Debug, Clone)]
pub struct MedicalToolSpec {
    pub name: MedicalToolName,
    pub description: String,
    pub outbound: bool,
    pub requires_human_review: bool,
}

impl MedicalToolSpec {
    fn local(name: MedicalToolName, description: &str) -> Self {
        Self {
            name,
            description: description.to_owned(),
            outbound: false,
            requires_human_review: false,
        }
    }

    fn reviewed_local(name: MedicalToolName, description: &str) -> Self {
        Self {
            name,
            description: description.to_owned(),
            outbound: false,
            requires_human_review: true,
        }
    }

    fn outbound(name: MedicalToolName, description: &str) -> Self {
        Self {
            name,
            description: description.to_owned(),
            outbound: true,
            requires_human_review: true,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentHarnessError {
    #[error("agent turn blocked by compliance policy: {0}")]
    Compliance(#[from] ComplianceBlock),

    #[error("unknown medical agent tool: {0:?}")]
    UnknownTool(MedicalToolName),

    #[error("tool {0:?} requires an outbound provider")]
    OutboundProviderRequired(MedicalToolName),
}

#[cfg(test)]
mod tests {
    use super::*;
    use med_compliance::{BaaRecord, BaaStatus, ComplianceApproval};

    fn today() -> Date {
        Date::from_calendar_date(2026, time::Month::May, 28).unwrap()
    }

    #[test]
    fn blocks_phi_openai_turn_without_vendor_record() {
        let harness = MedicalAgentHarness::new();
        let request = AgentTurnRequest {
            instruction: "Draft a SOAP note".to_owned(),
            contains_phi: true,
            context_summary: Some("synthetic context".to_owned()),
            outbound_provider: Some(OutboundProvider::openai("responses")),
            requested_tools: vec![MedicalToolName::DraftNoteWithOpenAi],
        };

        let result = harness.start_turn(request, today());

        assert!(matches!(
            result,
            Err(AgentHarnessError::Compliance(
                ComplianceBlock::MissingVendorRecord
            ))
        ));
    }

    #[test]
    fn permits_non_phi_openai_turn_without_baa() {
        let harness = MedicalAgentHarness::new();
        let request = AgentTurnRequest {
            instruction: "Explain SOAP note structure".to_owned(),
            contains_phi: false,
            context_summary: None,
            outbound_provider: Some(OutboundProvider::openai("responses")),
            requested_tools: vec![MedicalToolName::DraftNoteWithOpenAi],
        };

        let outcome = harness.start_turn(request, today()).unwrap();

        assert_eq!(outcome.state, AgentState::ReadyForLocalTools);
        assert_eq!(outcome.planned_tools.len(), 1);
        assert!(outcome.requires_human_review);
    }

    #[test]
    fn permits_phi_openai_turn_with_executed_baa_record() {
        let mut harness = MedicalAgentHarness::new();
        harness.register_vendor(VendorComplianceRecord {
            provider: "openai".to_owned(),
            phi_allowed: true,
            baa: BaaRecord {
                status: BaaStatus::Executed,
                signed_at: Some(today()),
                expires_at: None,
                covered_services: vec!["responses".to_owned()],
                document_sha256: Some("synthetic-document-hash".to_owned()),
                secure_document_uri: None,
            },
            approval: ComplianceApproval {
                approved: true,
                approved_by: Some("synthetic-compliance-reviewer".to_owned()),
                approved_at: Some(OffsetDateTime::now_utc()),
                notes: None,
            },
        });

        let request = AgentTurnRequest {
            instruction: "Draft a SOAP note".to_owned(),
            contains_phi: true,
            context_summary: Some("synthetic context".to_owned()),
            outbound_provider: Some(OutboundProvider::openai("responses")),
            requested_tools: vec![MedicalToolName::DraftNoteWithOpenAi],
        };

        let outcome = harness.start_turn(request, today()).unwrap();

        assert_eq!(outcome.state, AgentState::ReadyForLocalTools);
        assert_eq!(
            outcome.planned_tools[0].name,
            MedicalToolName::DraftNoteWithOpenAi
        );
    }
}
