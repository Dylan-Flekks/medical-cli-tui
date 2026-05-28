# Compliance Guardrails

This project is not a compliance guarantee. It is an open-source software project that should make compliant behavior easier to implement and unsafe behavior harder to trigger.

## Non-Negotiable Rules

- Do not commit PHI.
- Do not put PHI in GitHub issues, pull requests, screenshots, logs, prompts, examples, fixtures, or documentation.
- Do not send PHI to third-party APIs unless the provider, account, product, and service are covered by an executed BAA and locally approved.
- Do not treat generated billing codes or documentation as final without qualified human review.
- Do not use this for production patient care until it has undergone legal, privacy, security, and clinical review.

## BAA Gate

Before any AI provider can receive PHI, the app must verify:

```text
BAA status = executed
provider is approved for PHI
requested service/model is covered
approval is current
request metadata is logged
```

If any check fails, the app must block the request.

## Local Medical Storage

The repository may contain:

- schemas
- templates
- migrations
- synthetic fixtures
- deidentified fixtures
- code
- documentation

The repository must not contain:

- real patient charts
- identifiers
- clinical exports
- screenshots with PHI
- logs with PHI
- BAA contracts
- API keys

## Audit Scope

Audit events should cover:

- chart opened
- patient searched
- note created
- note edited
- note signed
- note amended
- billing code added or changed
- export created
- backup created
- AI call allowed
- AI call blocked
- compliance vendor changed
- failed unlock/login

## Vendor Registry

Vendor records live in the local compliance registry. Public repository examples must be placeholders only.
