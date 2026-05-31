# Codex Extraction Log

This log tracks any OpenAI Codex CLI source files copied, translated, or closely derived for Flekks EMR CLI.

OpenAI Codex CLI repository: <https://github.com/openai/codex>

License: Apache-2.0.

Current status: Flekks EMR CLI includes small, attributed Codex-derived harness
files for the medical agent submission/event protocol and thread handle.

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
- Tests added: `emits_configured_event_on_spawn`, `accepts_non_phi_turn_and_emits_turn_lifecycle`, `blocks_phi_turn_without_baa`, `permits_phi_turn_with_executed_baa_provider`, and `rejects_zero_loop_limits`.
