---
name: web_tls_scan
description: Guidance for bounded TLS and certificate configuration checks with testssl.sh, sslscan, or quick tls_inspect when HTTPS services need protocol, cipher, certificate, hostname, or expiry triage.
when-to-use: Use when HTTPS services need certificate, protocol, or cipher triage.
allowed-tools:
  - command_check
  - web_tls_scan
  - tls_inspect
  - vuln_engine
---

# web_tls_scan

Use `web_tls_scan` for bounded TLS checks. Prefer `tls_inspect` for a quick certificate summary and `web_tls_scan` when configuration findings are needed.

Use for:

- certificate subject/SAN/issuer/expiry evidence
- weak protocol or cipher triage
- HTTPS service inventory
- reportable TLS misconfiguration validation

Planning notes from AboutSecurity raw material:

- Capture certificate CN/SAN, issuer, validity window, hostname match, protocol versions, and weak cipher indicators.
- JARM/JA3-style fingerprints can help correlate services, but should not be reported as vulnerabilities.
- Re-check hostname mismatch and expiry against the user-facing hostname, not only an IP or internal redirect target.
- Prefer quick `tls_inspect` first when only certificate identity is needed; use scanner output when configuration findings matter.

Recommended flow:

1. Confirm host/port is in scope.
2. Run `web_tls_scan` with `testssl.sh` or `sslscan`.
3. Review returned `findings` and `evidence_refs`; the tool automatically writes a `finding_set` evidence record when findings are present.
4. Use `vuln_engine` only if additional normalization from saved stdout is needed.
5. Report protocol/cipher/certificate evidence with target and timestamp.

Source reference: compiled from `wiki/skills/aboutsecurity/tool/tlsx-probe/SKILL.md` into bounded TLS triage guidance.

Do not treat an old certificate date, hostname mismatch, or scanner warning as final impact without checking the exact service and user-facing hostname.
