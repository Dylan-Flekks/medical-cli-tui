# Codex Lab Inventory 02 - Tool Runtime And Approval

This is the Flekks-side summary of the detailed Codex lab report:

```text
C:\Users\peter\flekks-codex-lab
docs/flekks-inventory/02-tool-runtime-and-approval.md
```

No OpenAI Codex source files have been copied into Flekks EMR CLI for this report.

## What Was Mapped

The detailed report maps `codex-rs/core/src/tools`:

- tool spec planning
- model-visible tool registration
- tool router
- tool registry
- tool invocation context
- tool output conversion
- parallel execution
- cancellation
- approval orchestration
- network approval
- lifecycle events
- handler classification

The key runtime path is:

```text
model response item
  -> ToolRouter::build_tool_call
  -> ToolCallRuntime::handle_tool_call
  -> ToolRegistry::dispatch
  -> tool policy/hooks
  -> handler execution
  -> ToolOutput
  -> model-safe response item
  -> local event/audit path
```

## Main Decision

Do not copy the Codex tools module into Flekks yet.

Use it as a guide for a smaller original medical tool runtime:

```text
MedicalToolRuntime
MedicalToolRegistry
MedicalToolRouter
MedicalToolInvocation
MedicalToolOutput
MedicalApprovalPolicy
MedicalToolAudit
```

The Codex shape is strong. The default Codex handlers are coding-agent tools and several are unsafe defaults for a healthcare charting MVP.

## What To Adapt First

Build these concepts in Flekks:

- model response to typed tool-call boundary
- registry that owns all tool dispatch
- per-call invocation context with patient, encounter, note, and policy snapshot
- serialized-by-default execution
- cancellation with audit event
- human input tool for missing information
- medical approval gate before risky actions
- model-safe tool output separate from local audit detail

Recommended Flekks modules:

```text
crates/med-agent/src/tools/spec.rs
crates/med-agent/src/tools/router.rs
crates/med-agent/src/tools/registry.rs
crates/med-agent/src/tools/runtime.rs
crates/med-agent/src/tools/context.rs
crates/med-agent/src/tools/output.rs
crates/med-agent/src/tools/policy.rs
crates/med-agent/src/tools/approval.rs
crates/med-agent/src/tools/lifecycle.rs
crates/med-agent/src/tools/audit.rs
```

## MVP Tool Groups

Start with local charting tools only:

```text
chart.*
encounter.*
note.*
audit.*
billing_support.*
human.*
provider.*
```

Provider tools must stay behind the BAA gate. PHI cannot be sent to any model API unless the local repo has an active BAA record for that provider and the provider adapter marks the request path as BAA-eligible.

## What To Defer

Do not include these in the MVP runtime:

- arbitrary shell execution
- unified exec sessions
- stdin writing to local processes
- free-form patch application
- code-mode nested execution
- plugin install flow
- MCP/plugin runtime
- multi-agent spawning
- local desktop automation
- image/file preview as a model tool

These can be studied later, but each needs medical-specific policy, audit, approval, and PHI controls before becoming part of Flekks.

## Approval Policy Direction

Flekks approval should be medical, not shell/sandbox oriented:

```text
LocalRead
LocalDraftWrite
LocalAuditOnly
OutboundNonPhi
OutboundPhi
SignedClinicalChange
BillingSupportExport
DestructiveLocalWrite
DesktopAutomation
PluginInstall
BulkImport
BulkExport
```

Defaults:

- local reads can be allowed with local access
- local draft saves require audit events
- outbound PHI requires BAA plus human approval
- note signing/finalization requires human signature
- billing-support export requires human review
- destructive writes and bulk operations require approval
- desktop automation is supervised only

## Next Implementation Slice

Create the first `med-agent` tool runtime skeleton:

1. `MedicalToolInvocation`
2. `MedicalToolOutput`
3. `MedicalToolRuntime` trait
4. `MedicalToolRegistry`
5. `MedicalApprovalPolicy`
6. one read-only chart tool
7. one local note draft save tool
8. audit event emission for both tools
