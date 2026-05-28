# BAA Vendor Registry

The vendor registry is the enforcement source for outbound services.

## Rule

No executed BAA record means no PHI can be sent to that API.

## Example

```toml
provider = "example-ai"
phi_allowed = false

[baa]
status = "missing"
signed_at = ""
expires_at = ""
covered_services = []
document_sha256 = ""

[approval]
approved = false
approved_by = ""
approved_at = ""
```

## Status Values

```text
missing
pending
executed
expired
revoked
```

## Runtime Decision

```text
if request.contains_phi:
  require vendor record exists
  require phi_allowed == true
  require baa.status == executed
  require requested service in covered_services
  require approval.approved == true
  otherwise block
```

## Repository Policy

Do not commit actual BAA contracts. Store only non-sensitive metadata, hashes, or references to a secure local document store.
