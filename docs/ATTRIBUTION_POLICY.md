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

OpenAI Codex CLI is used as an architecture reference only.

Current Codex-inspired ideas:

- separate protocol/data types from runtime execution
- keep tool definitions separate from tool handlers
- route tool calls through a registry/router
- pass a structured invocation context into tool handlers
- require explicit approval/policy checks for risky actions
- keep terminal UI state separate from model/tool runtime
- test terminal rendering and state transitions

No Codex source files are currently copied into this repository.

## Medical Safety Override

Even when an upstream license allows broad reuse, medical privacy and compliance requirements still apply.

No third-party agent, API, or tool may receive PHI unless the local compliance registry has an executed BAA record that covers the exact provider, account, service, and model.
