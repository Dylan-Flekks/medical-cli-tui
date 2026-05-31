# Flekks EMR CLI

Flekks EMR CLI is an experimental, local-first medical documentation project for terminal-based charting, record review, note editing, documentation auditing, and billing support.

The project goal is a CLI-first medical records tool with a Ratatui dashboard. Medical storage is local by design. Cloud medical storage is out of scope. Optional AI integrations must pass an explicit BAA and compliance gate before any PHI can be sent to a third-party API.

> This project is not a certified EHR, medical device, billing authority, or compliance guarantee. Do not use it for real patient care or real PHI until it has gone through professional security, legal, privacy, and clinical review.

## Codex CLI Team Request

This fork includes a focused request for the Codex CLI team:

[README-CODEX-TEAM.md](README-CODEX-TEAM.md)

The request asks for an agentic medical workflow harness with local agent loops, TUI audit dashboards, macOS authorization-aware automation, multimodal inputs, and plugin surfaces for chart editing, billing support, Medicare policy, ICD coding, and compliance review.

## Product Direction

- Local encrypted SQLite records.
- CLI workflows for automation and power users.
- Ratatui dashboard for chart review, note writing, auditing, and billing support.
- Structured medical records inspired by FHIR concepts.
- Structured notes for SOAP, progress notes, H&P, discharge summaries, procedures, addenda, and billing documentation.
- Documentation and billing audit checks.
- Strict AI provider controls: no BAA, no PHI.
- Vendor-neutral local desktop automation interfaces for future user-authorized workflows.
- No PHI in GitHub issues, pull requests, logs, test fixtures, or example data.

See [docs/TUI_DASHBOARD_PLAN.md](docs/TUI_DASHBOARD_PLAN.md) for the detailed dashboard build plan.
See [docs/AGENT_HARNESS.md](docs/AGENT_HARNESS.md) for the medical agent harness plan.
See [docs/DESKTOP_AUTOMATION.md](docs/DESKTOP_AUTOMATION.md) for the vendor-neutral desktop automation boundary.
See [NOTICE](NOTICE) and [docs/ATTRIBUTION_POLICY.md](docs/ATTRIBUTION_POLICY.md) for attribution rules.

## Workspace

```text
crates/
  med-agent/      # medical agent harness, local tool registry, OpenAI BAA gate
  med-core/        # domain models for charting, notes, billing, and audit events
  med-store/       # local encrypted SQLite storage boundary
  med-compliance/  # BAA registry and PHI policy checks
  med-ai/          # AI provider abstractions and preflight enforcement
  med-cli/         # CLI command surface
  med-tui/         # Ratatui dashboard
docs/
  ARCHITECTURE.md
  COMPLIANCE.md
  MVP_PLAN.md
  ROADMAP.md
compliance/
  vendors/example-ai.toml
```

## Current Status

This repository is in project bootstrap. The initial codebase defines the architecture, CLI/TUI entry points, core medical data models, and BAA enforcement primitives. It is not production-ready.

## Quick Start

Install Rust, then run:

```bash
cargo run -p med-cli -- --help
cargo run -p med-cli -- init
cargo run -p med-cli -- patient add "Synthetic Patient" --mrn MRN-SYNTH-001
cargo run -p med-cli -- patient list
cargo run -p med-cli -- encounter new <patient-id> --reason "Synthetic follow-up"
cargo run -p med-cli -- encounter list <patient-id>
cargo run -p med-cli -- tui
```

On this machine, Rust may be available at `C:\Users\peter\.cargo\bin\cargo.exe` even if it is not on `PATH`.

SQLCipher support is intended for PHI-capable builds, but it is not enabled by default because it requires additional platform crypto setup:

```bash
cargo build -p med-store --no-default-features --features sqlcipher
```

## Local Data Rule

Medical data belongs outside the repository:

```text
~/.medical-cli/          # default on Unix-like systems
C:\Users\<you>\.medical-cli\  # default on Windows
  records.db
  attachments/
  backups/
  exports/
```

Set `MEDCLI_DATA_DIR` to use another local directory.

Never commit real PHI, screenshots containing PHI, clinical exports, logs with patient identifiers, model prompts containing PHI, or vendor BAA documents.

## AI Provider Rule

Third-party AI providers are disabled for PHI by default.

Before an API can receive PHI, the local compliance registry must show:

- BAA status is `executed`.
- The exact provider service/model is covered.
- The vendor is approved for PHI.
- The approval has not expired or been revoked.
- The attempted request is logged.

If any of those checks fail, the app must block the AI call.

## Contributing

Contributions are welcome, especially around Rust architecture, terminal UX, clinical documentation templates, local encryption, auditability, billing workflows, accessibility, and deidentified test fixtures.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull request.

CI is planned but not committed yet because GitHub workflow publishing requires a token with `workflow` scope.

## License

Apache-2.0. See [LICENSE](LICENSE).
