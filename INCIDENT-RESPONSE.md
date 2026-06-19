# Incident Response Plan — rust-RCP

## Scope

This plan covers security incidents affecting the rust-RCP library or any deployed system using it.

## Severity Classification

| Severity | Definition | Response SLA |
|---|---|---|
| Critical | Remote code execution, safety goal violation | 4 hours |
| High | Authentication bypass, replay attack confirmed | 24 hours |
| Medium | Integrity check failure, DoS via flooding | 72 hours |
| Low | Information disclosure, minor logic error | 5 business days |

## Contacts

| Role | Contact | Backup |
|---|---|---|
| Security Lead | security@example.com | +1-555-0100 |
| Safety Manager | safety@example.com | +1-555-0101 |
| Engineering Lead | engineering@example.com | +1-555-0102 |

## Response Steps

### 1. Detection and Triage (0–4 hours for Critical)

1. Receive report via `security@example.com` or internal monitoring alert.
2. Assign severity using the table above.
3. Create a private security advisory on GitHub (do not disclose publicly).
4. Notify Safety Manager if any safety goal (SG-001..SG-010) may be affected.

### 2. Containment

1. Identify affected versions and deployment scope.
2. For confirmed RCE or safety impact: recommend immediate update or isolation.
3. Disable affected features if possible via `AuthzController` policy hot-swap.

### 3. Analysis and Fix

1. Root cause analysis — trace to specific `REQ-*` requirement.
2. Write a failing test reproducing the vulnerability (`// fusa:test REQ-XXX`).
3. Implement the fix; add the requirement annotation.
4. Update `.fusa-problems.json` with problem record.
5. Re-run `rsfusa check` to verify coverage.

### 4. Recovery and Disclosure

1. Release a patch version.
2. Publish a security advisory (CVE if applicable).
3. Notify affected users.
4. Update `SECURITY.md` with lessons learned.

### 5. Post-Incident Review

Within 5 business days: review timeline, root cause, and process improvements. Update HARA if a new hazard is identified.
