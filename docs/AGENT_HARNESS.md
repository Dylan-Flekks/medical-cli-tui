# Medical Agent Harness

The medical agent harness is the structured bridge between the local chart repository, the Ratatui dashboard, and optional OpenAI API calls.

The harness is not a generic coding agent. It is a medical documentation assistant that must obey local-only storage, auditability, and BAA-gated PHI boundaries.

## Goals

- Help draft and edit structured clinical documentation.
- Help audit notes before signing.
- Help prepare billing-support drafts.
- Keep the TUI informed about agent state.
- Prevent outbound PHI unless compliance checks pass.
- Keep all chart source-of-truth data local in SQLite.

## Package Boundary

```text
med-agent
  agent turn state
  tool registry
  medical tool names
  BAA preflight before outbound OpenAI calls
  TUI-facing agent events

med-ai
  provider abstraction
  OpenAI/OpenAI-compatible API boundary
  request/response structs

med-store
  local SQLite repository

med-compliance
  BAA and vendor approval records

med-tui
  dashboard, status, review, and human confirmation UI
```

## Harness Flow

```text
TUI/CLI request
  -> MedicalAgentHarness::start_turn
  -> classify request: PHI | deidentified | non-PHI
  -> if outbound provider is requested:
       run BAA/vendor preflight
       block if missing, expired, revoked, not approved, or service not covered
  -> select allowed medical tools
  -> execute local tools against SQLite/service layer
  -> stream state/events back to TUI
  -> require human review before signing/exporting/billing finalization
  -> append audit events
```

## Initial Tool Set

```text
chart.search_patients
chart.read_patient_summary
chart.list_encounters
note.create_draft
note.update_draft
note.run_documentation_audit
billing.prepare_superbill_draft
compliance.check_vendor_baa
ai.draft_note_with_openai
```

All chart, note, audit, and billing tools are local. The only outbound tool is the OpenAI draft call, and it is disabled for PHI until the BAA gate passes.

## OpenAI API Boundary

The OpenAI API adapter must not be called directly from UI code.

Required preflight:

```text
if request.contains_phi:
  require provider == openai or configured OpenAI-compatible provider
  require local vendor compliance record exists
  require BAA status == executed
  require requested service/model is covered
  require approval.approved == true
  append attempted AI audit event
  block if any check fails
```

## Dashboard Integration

The TUI should show:

- agent state: idle, thinking, running local tool, waiting for approval, blocked, done
- current tool name
- AI BAA lock status
- blocked-request reason
- human-review warnings
- local-only storage indicator

## Dependencies

Baseline:

- Rust
- Ratatui
- Crossterm
- SQLite via `rusqlite`
- SQLCipher feature for PHI-capable builds
- Serde
- UUID

Future OpenAI adapter:

- `reqwest`
- `tokio`
- `eventsource-stream` or streaming-compatible parser
- `schemars` if JSON schema tool definitions are generated

The default build should not require an OpenAI API key.
