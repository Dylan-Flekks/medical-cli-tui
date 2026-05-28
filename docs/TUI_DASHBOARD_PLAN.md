# TUI Dashboard Plan

This document is the working build plan for the Ratatui dashboard.

The dashboard is the primary day-to-day interface for local medical documentation. The CLI remains the automation layer, but the TUI should feel like a focused terminal workstation for chart review, note writing, documentation audit, and billing support.

## Product Goals

- Review a patient chart without leaving the terminal.
- Write structured notes quickly with keyboard-first navigation.
- Keep relevant clinical context visible while documenting.
- Surface documentation and billing issues before signing.
- Make PHI boundaries obvious.
- Make AI access visibly gated by BAA/compliance status.
- Keep all medical storage local.

## Non-Goals for the First Dashboard

- Full EHR replacement.
- Claim submission.
- Cloud medical record storage.
- Automatic medical decision-making.
- Automatic billing-code finalization.
- Production PHI use before encryption, key handling, and compliance review are complete.

## Dashboard Layout

```text
+-------------------------------+--------------------------------------------------+----------------------------------+
| Patient Queue                 | Workspace                                        | Context                          |
|                               |                                                  |                                  |
| Search / filter               | Tabs: Chart | Note | Audit | Billing          | Problems                         |
| Active patients               |                                                  | Medications                      |
| Unsigned notes                | Chart summary                                    | Allergies                        |
| Billing flags                 | Encounter timeline                               | AI/BAA status                    |
| Audit tasks                   | Structured editor / tables / review lists       | Billing readiness                |
+-------------------------------+--------------------------------------------------+----------------------------------+
| Mode | focus | key hints | save/sign state | local-only PHI | AI BAA gate                          |
+--------------------------------------------------------------------------------------------------+
```

## Primary Screens

### Dashboard

The default screen. Shows the patient queue, active chart workspace, right-side context, and status bar.

Expected widgets:

- patient search box
- patient queue
- open task queue
- chart summary
- workspace tabs
- context panels
- status bar

### Patient Chart

Goal: fast chart review.

Expected widgets:

- demographics summary
- active encounter summary
- encounter timeline
- problems table
- meds table
- allergies panel
- vitals/labs trends
- recent notes
- attachments list

### Note Editor

Goal: structured documentation.

Expected widgets:

- template selector
- `tui-textarea` editor
- section navigation
- note status indicator
- save/review/sign actions
- version/amendment indicator
- documentation audit sidebar

Initial templates:

- SOAP
- Progress note
- H&P
- Procedure note
- Discharge summary
- Telephone encounter
- Billing addendum

### Audit Review

Goal: catch documentation and compliance issues before signing or billing.

Audit categories:

- missing required note section
- unsigned note
- stale draft
- assessment missing linked diagnosis
- procedure code lacks supporting documentation
- billing code without encounter link
- AI request blocked because BAA is missing
- export or backup warnings

### Billing Workbench

Goal: prepare a supervised superbill/claim draft.

Expected widgets:

- diagnosis-code table
- procedure-code table
- modifier and unit fields
- place-of-service field
- provider NPI field
- payer field
- claim-readiness checklist
- documentation support links

Billing output is a draft only. A qualified human must review final billing.

### Compliance

Goal: make outbound API state obvious.

Expected widgets:

- vendor list
- BAA status
- covered services
- approval status
- blocked request history
- PHI allowed/blocked indicator

## Keyboard Model

Global keys:

```text
q / Esc     quit
Tab         move focus forward
Shift+Tab   move focus backward
1           chart tab
2           note tab
3           audit tab
4           billing tab
j / Down    move selection down
k / Up      move selection up
Enter       open/activate selected item
/           search
s           save draft
S           sign or request sign confirmation
a           run audit
b           open billing workbench
?           keymap help
```

Dangerous or irreversible actions should require confirmation. Signing a note should never be a single accidental keystroke.

## TUI Code Architecture

```text
crates/med-tui/src/
  lib.rs
  app.rs             # app state, focus, tabs, demo data
  terminal.rs        # crossterm setup, event loop, cleanup
  ui.rs              # top-level layout
  theme.rs           # colors and styles
  widgets/
    mod.rs
    patient_queue.rs
    workspace.rs
    context_panel.rs
    status_bar.rs
```

Future expansion:

```text
screens/
  dashboard.rs
  patient_chart.rs
  note_editor.rs
  audit.rs
  billing.rs
  compliance.rs
```

The first refactor should keep behavior simple while making the boundaries clear.

## Data Flow

```text
keyboard event
  -> App::handle_key
  -> Action / state change
  -> service/repository call when needed
  -> App state update
  -> Ratatui redraw
```

The UI should never directly own database logic. The dashboard will eventually receive a repository/service handle from `med-cli` after local storage is initialized.

## Local Storage Integration Plan

Phase 1:

- TUI uses demo state.
- CLI owns local database commands.

Phase 2:

- TUI loads patient list from `med-store`.
- Selecting a patient loads encounters.
- Empty DB shows clear local-first onboarding.

Phase 3:

- Note editor saves drafts to local DB.
- Audit screen reads local note/billing state.
- Billing screen saves claim drafts.

Phase 4:

- Audit events are appended for chart open, note edit, note sign, export, and AI preflight.

## Dashboard Milestones

### Milestone 1: Component Refactor

Acceptance criteria:

- TUI code is split into app, terminal, UI, theme, and widgets.
- Existing dashboard still runs.
- Workspace tabs can be changed with `Tab` and number keys.
- Patient selection can move with `j/k` and arrows.
- Tests/checks pass.

### Milestone 2: Local Data Read

Acceptance criteria:

- `med tui` opens with real local patient list when records exist.
- Empty state explains `med patient add`.
- Selecting a patient shows stored encounters.
- No PHI is logged.

### Milestone 3: Structured Note Editor

Acceptance criteria:

- Note tab uses `tui-textarea`.
- SOAP template can be edited.
- Draft save works.
- Signing is blocked behind confirmation.
- Signed-note immutability is represented in the model.

### Milestone 4: Audit and Billing

Acceptance criteria:

- Audit tab displays structured audit flags.
- Billing tab displays diagnosis/procedure draft rows.
- Billing readiness gauge is calculated from data.
- Human review warnings are visible.

### Milestone 5: Compliance-Aware AI UI

Acceptance criteria:

- AI status indicator shows locked/allowed state.
- PHI AI requests are blocked without an executed BAA.
- Blocked calls appear in audit/compliance context.
- Provider setup is documented but disabled by default.

## Contributor-Friendly Issues

Good first contribution areas:

- improve table rendering and truncation
- add keymap help modal
- add empty-state UI
- add dashboard tests with Ratatui `TestBackend`
- implement note editor section navigation
- add audit flag types and display widgets
- add SQLCipher setup documentation

## Visual Design Rules

- Prefer dense, readable clinical information.
- Avoid decorative layouts.
- Use consistent borders and restrained color.
- Use color to signal status, not decoration.
- Keep PHI/local/AI state visible in the status bar.
- Never let dynamic text resize the layout.
- Keep important actions reachable by keyboard.

## Safety Rules

- No PHI in screenshots, tests, logs, fixtures, or GitHub issues.
- AI actions must call the BAA preflight before any outbound request.
- Signing, exporting, and deleting require confirmation.
- All chart access and edits should eventually write audit events.
- Billing output is draft-only until reviewed by a qualified human.
