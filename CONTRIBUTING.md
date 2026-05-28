# Contributing

Thank you for considering a contribution.

## No PHI

Do not include protected health information in:

- GitHub issues or discussions.
- Pull requests.
- Commit messages.
- Screenshots.
- Test fixtures.
- Logs.
- Prompt examples.
- Export samples.

Use synthetic or deidentified data only.

## Useful Contribution Areas

- Ratatui dashboard design.
- CLI command design.
- Local encrypted SQLite storage.
- Structured note templates.
- Documentation audit rules.
- Billing-support workflows.
- FHIR-inspired data modeling.
- Accessibility in terminal interfaces.
- Tests with synthetic data.
- Security and compliance documentation.

## Development

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Pull Requests

Please include:

- What changed.
- Why it changed.
- How it was tested.
- Whether any medical, security, privacy, or billing behavior changed.

## Medical and Compliance Boundaries

This project is not legal, clinical, billing, or compliance advice. Contributions that affect PHI handling, AI provider access, audit logs, billing logic, or record immutability should be conservative and clearly documented.
