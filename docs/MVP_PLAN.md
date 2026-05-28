# MVP Plan

## MVP Definition

The first usable MVP should support local workflows for:

- patient creation and search
- encounter creation
- structured note editing
- signed note immutability
- chart dashboard review
- basic documentation audit flags
- basic billing workbench
- append-only audit events
- encrypted backup/export boundary
- AI provider preflight with BAA enforcement

## Phase 1: Foundation

- Rust workspace.
- CLI command shell.
- Ratatui app shell.
- Encrypted SQLite connection boundary.
- Initial schema migrations.
- Audit event model.

## Phase 2: Core Chart

- Patient records.
- Encounters.
- Problems.
- Medications.
- Allergies.
- Observations.
- Encounter timeline.

## Phase 3: Notes

- SOAP note template.
- Progress note template.
- Multiline terminal editor with `tui-textarea`.
- Draft, reviewed, signed, amended statuses.
- Version history.

## Phase 4: Auditing

- Missing sections.
- Unsigned notes.
- Billing code without linked documentation.
- Diagnosis without supporting assessment.
- Stale drafts.

## Phase 5: Billing Workbench

- ICD-10-CM diagnosis references.
- CPT/HCPCS procedure-code placeholders.
- Modifiers and units.
- Superbill draft.
- Claim-readiness checklist.

## Phase 6: AI Boundary

- Provider trait.
- Request classification.
- BAA gate.
- Blocked-call audit events.
- Non-PHI demo provider.
- Human review workflow.

## Phase 7: Open Source Hardening

- CI.
- Deidentified test fixtures.
- Contribution guide.
- Security policy.
- Documentation.
- Issues for contributor-friendly tasks.
