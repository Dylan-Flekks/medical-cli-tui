# Architecture

Flekks EMR CLI is designed as a CLI-first local medical records application with a Ratatui dashboard.

## Principles

- Local-first: medical records are stored locally.
- CLI-first: every important workflow should be possible without the dashboard.
- TUI-first for daily use: Ratatui provides the main charting workspace.
- Storage is abstracted: the UI and CLI call services, not raw database code.
- AI is optional and policy-gated.
- No PHI is allowed in the repository.

## Layers

```text
med-cli
  command parsing and scripting workflows

med-tui
  Ratatui dashboard, key handling, screen state, widgets

med-agent
  medical agent harness, local tool registry, OpenAI BAA gate, TUI-facing agent events

service layer
  patient, encounter, note, billing, audit, AI orchestration

med-core
  medical domain types and value objects

med-store
  encrypted SQLite storage and migrations

med-compliance
  BAA registry, PHI policy checks, vendor authorization

med-ai
  AI provider traits and request preflight
```

## TUI Layout

```text
+-------------------+--------------------------------------+--------------------+
| Patient Queue     | Chart / Editor / Billing             | Context            |
| Search            | Encounter timeline                   | Problems           |
| Open tasks        | Structured note editor               | Medications        |
| Unsigned notes    | Vitals and labs charts               | Allergies          |
| Billing flags     | Audit and billing review             | AI/BAA status      |
+-------------------+--------------------------------------+--------------------+
| Mode | key hints | save status | PHI lock | AI provider status               |
+----------------------------------------------------------------------------+
```

## Data Storage

The intended local data directory is outside the repository:

```text
~/.medical-cli/
  records.db
  attachments/
  backups/
  exports/
```

SQLite should be encrypted with SQLCipher or another reviewed encryption approach before any real PHI is stored.

## Record Immutability

Signed notes should be immutable. Amendments create new versions and preserve the original signed content.

## Audit Events

Audit events should be append-only and eventually hash-chained. Events should capture access, edits, signing, exports, backups, failed access, AI preflight outcomes, and compliance registry changes.
