# Codex Extraction Log

This log tracks any OpenAI Codex CLI source files copied, translated, or closely derived for Flekks EMR TUI.

OpenAI Codex CLI repository: <https://github.com/openai/codex>

License: Apache-2.0.

Current status: Flekks EMR TUI includes small, attributed Codex-derived harness
files for the medical agent submission/event protocol, thread handle, and active
turn state.

## Required Entry Format

Use this format before merging any copied or closely derived code:

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

## Entries

## `crates/med-agent/src/protocol.rs`

- Upstream project: OpenAI Codex CLI
- Upstream repository: <https://github.com/openai/codex>
- Upstream commit: `bf72be59278e23002a352a53207182985cabb9d0`
- Lab commit used for review: `eba381c1a00afd21ddd4de5cdbf160f09ef67bbd`
- Upstream file(s): `codex-rs/protocol/src/protocol.rs`
- License: Apache-2.0
- Modification summary: copied/adapted the Codex submission queue/event queue protocol shape into a smaller medical protocol with `MedicalSubmission`, `MedicalOp`, `MedicalEvent`, and `MedicalEventMsg`.
- Medical safety changes: removed coding-agent, shell, patch, MCP, realtime, telemetry, and workspace payloads; added patient/encounter/note identifiers, PHI markers, BAA status, medical approval classes, and bounded loop limits.
- Tests added: covered through `crates/med-agent/src/thread.rs` lifecycle tests.

## `crates/med-agent/src/thread.rs`

- Upstream project: OpenAI Codex CLI
- Upstream repository: <https://github.com/openai/codex>
- Upstream commit: `bf72be59278e23002a352a53207182985cabb9d0`
- Lab commit used for review: `eba381c1a00afd21ddd4de5cdbf160f09ef67bbd`
- Upstream file(s): `codex-rs/core/src/codex_thread.rs`, `codex-rs/core/src/session/mod.rs`, `codex-rs/core/src/session/handlers.rs`
- License: Apache-2.0
- Modification summary: copied/adapted the Codex thread conduit pattern into `MedicalAgentThread` with `submit`, `next_event`, `status`, `config_snapshot`, and `shutdown_and_wait`.
- Medical safety changes: removed coding-agent config, shell execution, patching, MCP, telemetry, and cloud task behavior; added local-only session configuration, PHI/BAA turn blocking, loop-limit validation, cancellation, and shutdown events.
- Tests added: `emits_configured_event_on_spawn`, `accepts_non_phi_turn_and_emits_turn_lifecycle`, `blocks_phi_provider_turn_without_baa`, `permits_phi_turn_with_executed_baa_provider`, `rejects_zero_loop_limits`, `desktop_turn_waits_for_approval_then_completes`, and `desktop_turn_denial_aborts_turn`.

## `crates/med-agent/src/turn.rs`

- Upstream project: OpenAI Codex CLI
- Upstream repository: <https://github.com/openai/codex>
- Upstream commit: `bf72be59278e23002a352a53207182985cabb9d0`
- Lab commit used for review: `eba381c1a00afd21ddd4de5cdbf160f09ef67bbd`
- Upstream file(s): `codex-rs/core/src/state/turn.rs`, `codex-rs/core/src/tasks/mod.rs`
- License: Apache-2.0
- Modification summary: copied/adapted the Codex active-turn state pattern into `ActiveMedicalTurn`, tracking one running medical turn with pending approvals, pending tool calls, cancellation state, loop counters, and completion/abort lifecycle events.
- Medical safety changes: replaced shell/sandbox approval waiters with medical approval classes; added patient/encounter/note context, bounded loop limits, local cancellation cleanup, and PHI-safe redacted summaries.
- Tests added: `records_steps_until_limit`, `tracks_pending_approval_resolution`, and `clears_pending_state_on_cancel`.
