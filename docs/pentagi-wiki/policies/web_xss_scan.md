---
name: web_xss_scan
description: Guidance for authorization-gated XSS reflection and bounded validation with dalfox or kxss when a concrete in-scope URL, parameter, or scanner finding needs non-destructive evidence.
when-to-use: Use when a concrete in-scope URL or parameter needs XSS validation.
allowed-tools:
  - command_check
  - web_xss_scan
  - vuln_engine
  - http_request
---

# web_xss_scan

Use `web_xss_scan` only for explicit, in-scope XSS validation. It requires `confirm_authorized: true`.

Use for:

- confirming reflected XSS signals from crawlers or scanners
- triaging parameter reflection contexts
- producing evidence refs for validated or false-positive findings
- supporting report impact with exact request/response proof

Planning notes from AboutSecurity raw material:

- Start with parameter reflection and output-context triage before full scanner validation.
- DOM analysis is useful when JavaScript builds sinks or routes client-side, but it still needs request/response or browser-context evidence.
- Keep payloads inert and clearly marked. Evidence should prove reflection/context, not token theft or impact on other users.
- Blind callback and WAF-bypass modes are not default PentAGI workflow steps; require separate authorization and review.

Recommended flow:

1. Confirm the exact URL and parameter are authorized.
2. Use `kxss` for reflection triage or `dalfox` for bounded validation.
3. Review returned `findings` and `evidence_refs`; the tool automatically writes a `finding_set` evidence record when findings are present.
4. Use `vuln_engine` only if additional normalization from saved stdout is needed.
5. Keep proof non-destructive and avoid browser exploitation chains.

Source reference: compiled from `wiki/skills/aboutsecurity/tool/dalfox-xss/SKILL.md` with blind callback, WAF bypass loops, and exploit-chain behavior excluded.

Do not target other users, steal tokens, perform account actions, or iterate WAF bypass payloads.
