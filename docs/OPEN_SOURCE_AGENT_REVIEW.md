# Open-Source Agent Harness Review

This project can learn from open-source agent harnesses, but medical constraints require a narrower design.

## OpenAI Codex CLI

Repository: <https://github.com/openai/codex>

License: Apache-2.0.

Status for Flekks EMR CLI: strong architecture reference.

Useful patterns observed in the Rust implementation:

- protocol crate separated from runtime crates
- model client separated from tool execution
- session and turn-scoped model client state
- explicit tool registry/router modules
- typed tool invocation context
- structured tool output conversion back to model messages
- approval and sandbox policy modules
- TUI state separated from core execution
- extensive snapshot and state-transition tests

Current usage: reference only. No Codex source files are copied.

## Google Gemini CLI

Repository: <https://github.com/google-gemini/gemini-cli>

License: Apache-2.0 according to the project README.

Status for Flekks EMR CLI: useful reference for terminal UX, config, MCP-style extensibility, and project context conventions. It is TypeScript/Node, so less directly reusable for the Rust TUI.

## Cline

Repository: <https://github.com/cline/cline>

License: the project is publicly available and commonly listed as Apache-2.0. Verify the repository license before copying code.

Status for Flekks EMR CLI: useful reference for human-in-the-loop approvals, multi-provider design, and tool/action review flows.

## Goose

Repository: <https://github.com/block/goose>

License: Apache-2.0 according to Block's launch materials and repository metadata.

Status for Flekks EMR CLI: useful reference for local agent extensibility and provider-agnostic tool execution.

## Cursor

Cursor provides public repositories and open-source notices, but its main agent/editor harness should be treated as proprietary unless a specific source file/repository has a clear open-source license.

Status for Flekks EMR CLI: do not copy agent/editor code. It can be studied only at the product-behavior level through public documentation.

## Windsurf

Windsurf provides public documentation and plugin materials, but its core editor/agent implementation should be treated as proprietary unless a specific repository has a clear open-source license.

Status for Flekks EMR CLI: do not copy agent/editor code. It can be studied only at the product-behavior level through public documentation.

## Medical Harness Direction

Flekks EMR CLI should implement an original harness with these boundaries:

- local chart repository as the source of truth
- structured tools for patient, encounter, note, audit, billing, and compliance operations
- OpenAI API adapter behind a BAA/PHI gate
- no cloud medical storage
- no PHI in logs, GitHub, screenshots, fixtures, or telemetry
- human confirmation for signing, exporting, deleting, and outbound PHI requests
