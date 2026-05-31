# Attribution Policy

Flekks EMR CLI is Apache-2.0.

## When Referencing Open Source Projects

It is acceptable to study open-source projects and apply general architectural ideas. Architectural concepts, APIs, module boundaries, and design lessons do not usually require copying source code.

When source code is copied, translated, or closely derived from another project, contributors must:

- preserve copyright and license notices
- include the upstream license when required
- preserve relevant upstream `NOTICE` entries when required
- document modifications clearly
- add attribution in this repository's `NOTICE`
- avoid implying endorsement by the upstream project

## Current Codex Usage

OpenAI Codex CLI is used as both an architecture reference and a small,
attributed source-code donor for the medical agent harness.

Current Codex-inspired ideas:

- separate protocol/data types from runtime execution
- keep tool definitions separate from tool handlers
- route tool calls through a registry/router
- pass a structured invocation context into tool handlers
- require explicit approval/policy checks for risky actions
- keep terminal UI state separate from model/tool runtime
- test terminal rendering and state transitions
- submission/event queue protocol shape
- thread handle methods for `submit`, `next_event`, status snapshots, and shutdown

Codex-derived files are tracked in `docs/CODEX_EXTRACTION_LOG.md`.
Each derived source file must include a file-level notice naming the upstream
repository, upstream commit, upstream file path, Apache-2.0 license, and Flekks
modification summary.

The planned extraction workflow is documented in `docs/CODEX_EXTRACTION_PLAN.md`.
Any future copied or closely derived files must also be recorded in
`docs/CODEX_EXTRACTION_LOG.md` before merging.

## Medical Safety Override

Even when an upstream license allows broad reuse, medical privacy and compliance requirements still apply.

No third-party agent, API, or tool may receive PHI unless the local compliance registry has an executed BAA record that covers the exact provider, account, service, and model.
