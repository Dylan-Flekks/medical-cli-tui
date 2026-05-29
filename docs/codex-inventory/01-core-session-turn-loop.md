# Codex Lab Inventory 01 - Core Session And Turn Loop

This is the Flekks-side summary of the detailed Codex lab report:

```text
C:\Users\peter\flekks-codex-lab
docs/flekks-inventory/01-core-session-turn-loop.md
```

No OpenAI Codex source files have been copied into Flekks EMR CLI for this report. This is an architecture map for future extraction or rewrite work.

## Source

- Upstream repository: <https://github.com/openai/codex>
- Lab fork: <https://github.com/Dylan-Flekks/flekks-codex-lab>
- Snapshot commit: `bf72be59278e23002a352a53207182985cabb9d0`
- Lab branch: `flekks/inventory`
- Upstream license: Apache-2.0

## What Was Mapped

The detailed report maps the core Codex runtime path:

```text
ThreadManager
  -> Codex::spawn
  -> Session
  -> CodexThread
  -> submit(Op)
  -> session handlers
  -> Session::spawn_task
  -> run_turn
  -> ModelClientSession::stream
  -> stream event handling
  -> ToolRouter / ToolCallRuntime / ToolRegistry
  -> tool output recorded into history
  -> follow-up model sampling or turn completion
```

The most relevant pattern is the queue-pair agent runtime:

```text
submission queue: CLI/TUI operations into the session
event queue: structured runtime events back to the CLI/TUI
active turn state: cancellation, approvals, pending inputs, pending tool replies
task runtime: one background task owns the current turn
tool runtime: model tool calls become cancellable, audited local operations
```

## Flekks Implications

Flekks should not start by copying the Codex core wholesale. The better next step is to build a smaller medical agent runtime using the same architectural shape.

Recommended module direction:

```text
crates/med-agent/src/protocol.rs
crates/med-agent/src/thread.rs
crates/med-agent/src/session.rs
crates/med-agent/src/turn.rs
crates/med-agent/src/task.rs
crates/med-agent/src/tool_registry.rs
crates/med-agent/src/tool_runtime.rs
crates/med-agent/src/approval.rs
crates/med-agent/src/events.rs
crates/med-agent/src/cancellation.rs
```

Target runtime shape:

```text
MedicalAgentThread
  submit(MedicalOp)
  next_event() -> MedicalEvent
  status()
  shutdown()

MedicalSession
  active_turn
  local_store
  compliance_gate
  audit_writer
  provider_gateway
  tool_registry

MedicalTurnContext
  turn_id
  patient_id
  encounter_id
  note_id
  approval_policy
  phi_policy_snapshot
  baa_snapshot
  loop_limits
```

## MVP Extraction Order

1. Add `med-agent` protocol and event types.
2. Add `MedicalAgentThread` and `MedicalSession` as a local queue-pair runtime.
3. Add active turn state with cancellation and pending approvals.
4. Add a small medical tool registry for chart, encounter, note, audit, and billing-support actions.
5. Add BAA-gated `med-ai` provider calls behind one provider gateway.
6. Wire the Ratatui dashboard to `MedicalEvent` instead of calling internal agent state directly.
7. Record all medical tool activity to the local audit log.

## Safety Requirements

- No cloud medical record storage.
- No provider API call may receive PHI unless the BAA gate passes first.
- Public logs, docs, tests, and fixtures must not include PHI.
- Medical tool output must be written to local audit events.
- Human approval is required for signing, destructive changes, exports, outbound PHI, and irreversible local actions.
- Keep local desktop automation vendor-neutral and policy-gated.
- Do not position the project as autonomous diagnosis or autonomous clinical interpretation software.

## Next Report

Map `codex-rs/core/src/tools` in more detail:

- tool registry
- tool runtime
- approval orchestration
- cancellation behavior
- parallel tool execution
- which tool handlers should be deleted, rewritten, or studied only
