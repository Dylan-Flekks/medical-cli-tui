# Codex Lab Inventory 00 - Summary

This is the first inventory summary for the separate Flekks Codex lab fork.

## Source

- Upstream repository: <https://github.com/openai/codex>
- Lab fork: <https://github.com/Dylan-Flekks/flekks-codex-lab>
- Local lab checkout: `C:\Users\peter\flekks-codex-lab`
- Snapshot commit: `bf72be59278e23002a352a53207182985cabb9d0`
- Upstream license: Apache-2.0

The detailed lab report lives in the lab branch:

```text
flekks/inventory
docs/flekks-inventory/00-summary.md
```

## First Count

Counts were generated from `git ls-files` in the lab checkout and streamed through tracked text/config-ish files.

| Metric | Count |
| --- | ---: |
| Tracked files | 4,711 |
| Counted text/config files | 4,080 |
| Physical lines | 1,157,255 |
| Nonblank lines | 1,060,244 |

## Rust Size

| Bucket | Files | Physical lines | Nonblank lines |
| --- | ---: | ---: | ---: |
| All Rust | 2,044 | 931,266 | 851,060 |
| `codex-rs` Rust | 2,033 | 930,503 | 850,381 |
| `codex-rs/*/src` implementation heuristic | 1,500 | 634,311 | 581,363 |
| Rust tests heuristic | 523 | 295,583 | 268,482 |

## Largest Areas

| Area | Physical lines | Initial Flekks relevance |
| --- | ---: | --- |
| `codex-rs/core` | 242,525 | highest: session, turn loop, tools, provider calls |
| `codex-rs/tui` | 207,031 | high: terminal state, event loop, approvals, tests |
| `codex-rs/app-server-protocol` | 130,553 | medium: protocol/event shapes |
| `codex-rs/app-server` | 97,164 | safety-critical: cloud/app assumptions need review |
| `codex-rs/core-plugins` | 21,262 | medium/high: plugin concepts |
| `codex-rs/cli` | 20,403 | medium: command structure |
| `codex-rs/exec-server` | 18,883 | rewrite/wrap: local execution boundary |
| `codex-rs/protocol` | 18,622 | high: protocol and event types |
| `codex-rs/windows-sandbox-rs` | 16,811 | study only: platform sandbox concepts |
| `codex-rs/state` | 16,573 | high: local state/session patterns |
| `codex-rs/config` | 15,873 | high: profile/config architecture |

## Immediate Mapping Targets

1. `codex-rs/core`
2. `codex-rs/tui`
3. `codex-rs/protocol`
4. `codex-rs/config`
5. `codex-rs/state`
6. `codex-rs/thread-store`
7. `codex-rs/tools`
8. `codex-rs/codex-mcp`
9. `codex-rs/rmcp-client`
10. `codex-rs/core-plugins`
11. `codex-rs/skills`

## Rewrite Or Wrap First

These are useful but high-risk for healthcare and must not be imported directly:

- shell execution
- filesystem mutation
- desktop/screen observation
- sandbox execution
- telemetry and OpenTelemetry
- login/auth
- network proxying
- app-server/cloud task assumptions

## Report 01

Completed:

```text
docs/codex-inventory/01-core-session-turn-loop.md
```

This report maps `codex-rs/core`:

- session lifecycle
- turn loop
- tool dispatch
- model call boundary
- approval gate
- cancellation path
- event stream

No Codex source files have been copied into Flekks EMR CLI.

## Report 02

Completed:

```text
docs/codex-inventory/02-tool-runtime-and-approval.md
```

This report maps `codex-rs/core/src/tools`:

- tool registry
- tool runtime
- approval orchestration
- cancellation behavior
- parallel tool execution
- which handlers to delete, rewrite, or study only

## Next Report

The next report should map `codex-rs/tui`:

- event loop
- dashboard layout/state model
- streaming turn updates
- approval UI
- status rendering
- snapshot test patterns
