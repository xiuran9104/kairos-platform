---
name: web_wordpress_scan
description: Guidance for authorization-gated WordPress scanning with wpscan after WordPress is confirmed and plugin, theme, version, or configuration evidence is needed.
when-to-use: Use only after WordPress is identified on an in-scope URL.
allowed-tools:
  - command_check
  - web_wordpress_scan
  - web_fingerprint
  - vuln_engine
---

# web_wordpress_scan

Use `web_wordpress_scan` after WordPress is confirmed. It requires `confirm_authorized: true`.

Use for:

- WordPress core, plugin, and theme inventory
- known vulnerable component triage
- configuration and exposure evidence
- normalized finding generation for reporting

Recommended flow:

1. Confirm WordPress via `web_fingerprint` or HTTP evidence.
2. Run `web_wordpress_scan` without enumeration first.
3. Use `enumerate: true` only for bounded plugin/theme/config checks.
4. Review returned `findings` and `evidence_refs`; the tool automatically writes a `finding_set` evidence record when findings are present.
5. Use `vuln_engine` only if additional normalization from saved stdout is needed.

Do not perform password attacks or user-focused brute force.
