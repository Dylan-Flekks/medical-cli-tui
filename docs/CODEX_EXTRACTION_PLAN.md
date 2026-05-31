# Codex Extraction Plan

This document describes how Flekks EMR CLI can use the open-source OpenAI Codex CLI codebase as a large implementation reference and possible source-code donor while keeping the medical product safe, attributed, and maintainable.

OpenAI Codex CLI repository: <https://github.com/openai/codex>

License: Apache-2.0.

Current Flekks status: small Codex-derived harness files have been copied and
adapted into `crates/med-agent` with Apache-2.0 attribution. See
`docs/CODEX_EXTRACTION_LOG.md`.

## Strategic Decision

The Codex CLI harness is too large to recreate quickly from scratch. A practical strategy is to import, map, and reduce an upstream-derived harness in a separate lab track, then move only vetted pieces into Flekks EMR CLI.

Do not turn the main Flekks EMR CLI repository into a direct reduced fork on day one.

Use two tracks:

1. Flekks EMR CLI product repo: small, medical-specific, local-first, PHI-safe.
2. Codex extraction lab: large upstream fork/import used for mapping, deletion, adaptation, and experiments.

The product repo should stay understandable to healthcare contributors. The lab can carry hundreds of thousands of upstream lines while the team learns the harness.

## Why Not Direct Fork-And-Delete In This Repo

A direct fork-and-delete sounds fast, but it imports a large amount of behavior that is not medically safe by default:

- coding-agent prompts and assumptions
- repo editing, patching, and Git workflows
- broad filesystem and shell tool surface
- cloud/account/session assumptions
- telemetry/logging assumptions that need PHI review
- release tooling unrelated to Flekks
- tests that verify coding-agent behavior instead of medical workflows
- large upstream merge burden

The useful part is the agent harness architecture. The unsafe part is assuming a coding agent can become a medical documentation agent by deleting files.

## Repository Layout Options

### Option A: Separate Fork Or Lab Repo

Recommended.

Create a separate public or private repository such as:

```text
flekks-codex-lab
```

Purpose:

- fork or mirror `openai/codex`
- preserve upstream history
- run inventory scripts
- map modules
- delete unrelated subsystems experimentally
- document changed files
- extract stable crates or modules into Flekks later

Benefits:

- keeps Flekks EMR CLI small
- preserves upstream attribution and history
- makes license compliance easier
- avoids burying the medical product under coding-agent files
- allows aggressive deletion experiments without destabilizing the EMR repo

### Option B: Git Submodule

Acceptable for read-only reference, less useful for modification.

```text
third_party/openai-codex
```

Benefits:

- clear upstream boundary
- small Flekks Git history

Drawbacks:

- awkward for large modifications
- does not support a clean reduced derivative unless the submodule points to a separate fork

### Option C: Git Subtree Or Vendor Import

Use only if the team intentionally wants the Codex source tree inside this repo.

```text
vendor/openai-codex/
```

Requirements:

- preserve Apache-2.0 license
- preserve upstream `NOTICE`
- add a Flekks modification notice
- avoid PHI test data
- keep upstream-derived code clearly separated

Drawbacks:

- repository becomes very large
- public contributors may struggle to find the medical product
- every scan, search, and review becomes noisier

## Recommended Path

Use Option A first.

```text
openai/codex
  -> fork or mirror into flekks-codex-lab
  -> inventory and map harness modules
  -> delete irrelevant subsystems in lab branches
  -> define stable extraction interfaces
  -> copy/adapt only selected files into Flekks EMR CLI with attribution
```

The lab is where the hundreds of thousands of lines can exist. Flekks EMR CLI receives only the medically appropriate parts after they are understood.

## Apache-2.0 Compliance Checklist

Before copying or deriving source code:

- keep the Apache-2.0 license text
- preserve copyright notices
- preserve relevant upstream attribution notices
- include upstream `NOTICE` content when applicable
- mark modified files prominently
- document the upstream commit SHA
- document why the file was copied
- document medical-specific changes
- avoid implying OpenAI endorsement
- avoid using upstream trademarks except for factual attribution

Every copied or closely derived file should start with a short notice:

```text
Derived from OpenAI Codex CLI.
Original source: https://github.com/openai/codex
Upstream license: Apache-2.0
Upstream commit: <sha>
Modified by Flekks EMR CLI contributors for local-first medical documentation workflows.
```

Update:

```text
NOTICE
docs/ATTRIBUTION_POLICY.md
docs/CODEX_EXTRACTION_LOG.md
```

## Medical Safety Gate

No upstream agent behavior is accepted into Flekks until it passes a medical safety review.

Required checks:

- no PHI in prompts, logs, tests, fixtures, screenshots, or telemetry
- no outbound PHI unless the local BAA registry allows the exact provider/account/service/model
- no autonomous signing, submission, deletion, export, or irreversible billing action
- no autonomous diagnosis
- no autonomous radiology or medical-image interpretation
- no vendor-specific local software claims without permission
- all long-running loops must be bounded, interruptible, and auditable
- all local desktop automation must be user-authorized and vendor-neutral

## Extraction Principles

Prefer structure over bulk.

Copying a whole subsystem is acceptable only when:

- it is understood
- it has a narrow runtime boundary
- it can be tested without PHI
- it does not require coding-agent-specific behavior
- attribution is complete
- medical safety review is documented

Otherwise, use Codex as a guide and implement an original Flekks module.

## Initial Inventory Targets

The extraction lab should inventory these areas first:

```text
codex-rs/
codex-cli/
sdk/
docs/
scripts/
third_party/
```

For each area, produce:

- file count
- physical line count
- nonblank line count
- language breakdown
- crate/package list
- dependency graph
- test count
- binary targets
- feature flags
- network-related code
- telemetry/logging code
- filesystem/shell execution code
- TUI code
- model provider code
- MCP/plugin code
- session/history code
- approval/sandbox code

Output documents:

```text
docs/codex-inventory/00-summary.md
docs/codex-inventory/01-crates.md
docs/codex-inventory/02-agent-loop.md
docs/codex-inventory/03-tools.md
docs/codex-inventory/04-approval-sandbox.md
docs/codex-inventory/05-tui.md
docs/codex-inventory/06-model-providers.md
docs/codex-inventory/07-mcp-plugins.md
docs/codex-inventory/08-delete-map.md
```

## Relevance Map

### High Relevance

These should be mapped deeply and may become direct extraction candidates:

- session state
- turn loop
- streaming event model
- model request/response abstraction
- tool registry
- tool invocation router
- tool-call approval model
- interrupt/cancel handling
- long-running task state
- TUI event loop patterns
- snapshot/state-transition tests
- config/profile loading

### Medium Relevance

These should be studied and adapted cautiously:

- MCP client/server support
- plugin discovery and capability negotiation
- conversation history
- provider/model catalog
- resumable sessions
- terminal transcript rendering
- error classification
- update/release checks

### Low Relevance Or Delete First

These should not drive the Flekks product architecture:

- coding-agent prompts
- Git commit/PR workflows
- patch application as a primary feature
- repository search/edit tooling
- cloud coding-agent assumptions
- IDE-specific integration paths
- npm package wrapper details unless needed for installation
- OpenAI-specific product UX that does not apply to medical records

### Safety-Critical Rewrite

These may be useful but must be rewritten or heavily wrapped:

- shell command execution
- filesystem read/write tools
- screenshot/screen observation
- telemetry/logging
- crash reports
- external network calls
- browser or desktop automation
- memory systems

## Target Flekks Harness Architecture

The extracted or inspired code should land behind medical-specific crates:

```text
crates/med-agent/
  src/session.rs
  src/turn.rs
  src/loop_control.rs
  src/events.rs
  src/tool_runtime.rs
  src/approval.rs
  src/policy.rs
  src/memory.rs

crates/med-ai/
  src/provider.rs
  src/openai.rs
  src/request.rs
  src/response.rs
  src/preflight.rs

crates/med-tui/
  src/agent_panel.rs
  src/agent_events.rs

crates/med-store/
  src/audit.rs
  src/sessions.rs
```

Medical tools should remain domain-specific:

```text
chart.list_patients
chart.get_patient
chart.list_encounters
chart.create_encounter
note.get_or_create_draft
note.update_section
note.run_local_audit
billing.prepare_support_draft
compliance.check_baa
desktop.observe_authorized_app
desktop.propose_action
desktop.verify_state
```

## Extraction Workstreams

### Workstream 1: Upstream Lab Setup

Deliverables:

- `flekks-codex-lab` fork or mirror
- upstream remote preserved
- baseline branch pinned to upstream commit
- local build documented
- upstream LICENSE and NOTICE preserved

Acceptance criteria:

- lab builds or has documented build blockers
- upstream commit SHA is recorded
- no medical code added yet

### Workstream 2: Static Inventory

Deliverables:

- line-count report
- crate/package map
- dependency map
- binary target map
- delete candidates
- extraction candidates

Acceptance criteria:

- every top-level directory has an owner decision: keep, study, delete, or ignore
- high-risk code paths are flagged

### Workstream 3: Harness Map

Deliverables:

- agent loop diagram
- session lifecycle diagram
- model call lifecycle
- tool call lifecycle
- approval lifecycle
- cancellation lifecycle
- TUI event lifecycle

Acceptance criteria:

- Flekks contributors can explain how a user request becomes model/tool events
- every outbound call location is identified
- every local execution location is identified

### Workstream 4: First Reduction Branch

Deliverables:

- delete coding-agent prompts
- delete Git/PR-specific workflow code
- delete IDE-specific paths
- delete release/package code not needed for Flekks
- keep tests that prove core harness behavior

Acceptance criteria:

- reduced lab still builds or has a clear compile-fix list
- deleted areas are documented
- no Flekks medical logic has been mixed in yet

### Workstream 5: Medical Policy Layer

Deliverables:

- PHI boundary policy
- BAA preflight hook
- local-only storage policy
- human approval policy
- audit event policy
- long-running loop limits

Acceptance criteria:

- every model call passes through BAA/PHI preflight
- every tool action has an approval class
- every irreversible action requires confirmation

### Workstream 6: Flekks Interface Extraction

Deliverables:

- small Rust interfaces copied or rewritten into Flekks
- attribution notices for copied/derived files
- tests in Flekks against medical workflows

Acceptance criteria:

- Flekks compiles without the whole Codex tree
- Flekks tests do not require upstream lab checkout
- copied code is traceable to upstream commit/file

### Workstream 7: Agent TUI Integration

Deliverables:

- Ratatui agent activity panel
- turn status view
- approval queue
- cancellation controls
- audit event stream

Acceptance criteria:

- TUI shows what the agent is doing
- user can approve/deny actions
- user can stop a loop
- audit trail is local and PHI-safe

## Delete Map Draft

Initial likely delete categories in the lab:

```text
coding-only prompts
repo patching UX
GitHub PR workflow
cloud coding task UX
IDE-specific install/docs
release/package automation
non-Rust wrappers not needed for Flekks distribution
tests that only validate coding-agent behavior
```

Initial likely keep/study categories:

```text
session runtime
turn loop
events
model provider abstraction
tool registry
approval policy
sandbox policy concepts
MCP/plugin architecture
TUI state/event patterns
snapshot tests
config profiles
```

Initial likely rewrite categories:

```text
shell/filesystem tools
screen observation
desktop automation
telemetry/logging
memory
external network tools
provider authentication
```

## Flekks-Specific Acceptance Tests

The extracted harness should eventually pass these tests:

- PHI model request is blocked without executed BAA record.
- Non-PHI model request can be allowed without BAA.
- Note drafting can run only on local chart data or BAA-approved provider calls.
- Agent loop stops at max step count.
- Agent loop can be cancelled from the TUI.
- Desktop automation cannot run without an allowlisted target.
- Desktop action proposal does not execute before approval.
- Signing/export/submission/deletion requires explicit confirmation.
- Logs contain no raw note body by default.
- Audit events are appended for every tool action.

## Branching Plan

Suggested branches in the lab:

```text
upstream/main
flekks/inventory
flekks/delete-pass-1
flekks/harness-map
flekks/policy-layer
flekks/extraction-candidates
```

Suggested branches in Flekks EMR CLI:

```text
main
codex-extraction-plan
agent-harness-events
agent-harness-tool-runtime
agent-harness-approval
agent-tui-panel
```

## Documentation Log

Create this file before copying code:

```text
docs/CODEX_EXTRACTION_LOG.md
```

Each copied or closely derived file must have an entry:

```text
## <Flekks file path>

- Upstream project: OpenAI Codex CLI
- Upstream repository: https://github.com/openai/codex
- Upstream commit: <sha>
- Upstream file(s): <path>
- License: Apache-2.0
- Modification summary:
- Medical safety changes:
- Tests added:
```

## First Three Concrete Tasks

1. Create `flekks-codex-lab` as a fork or mirror of `openai/codex`.
2. Generate `docs/codex-inventory/00-summary.md` from the lab checkout.
3. Create `docs/CODEX_EXTRACTION_LOG.md` in Flekks EMR CLI before copying any code.

Only after those tasks should code move from Codex into Flekks.

## Current Decision

Flekks EMR CLI will continue building the medical TUI and local record system while the Codex lab maps the larger harness. The two efforts meet at stable interfaces:

- medical chart tools
- BAA-gated provider calls
- local audit events
- TUI approval queue
- bounded loop controller

This keeps the MVP moving while still using the large existing harness to accelerate the agent architecture.
