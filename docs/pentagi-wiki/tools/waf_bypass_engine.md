# waf_bypass_engine

Generates a ranked, bounded list of WAF bypass attempts using structured transformation rules from the AboutSecurity methodology.

Use this tool after `waf_observation_plan` has identified WAF/CDN blocking behavior and `confirm_authorized=true` has been given.

**This is a planning tool.** It emits `next_tool_calls` as `http_request` calls with transformed payloads and request modifications. It does not directly send traffic.

## Inputs

| Field | Required | Description |
|-------|----------|-------------|
| `target` | yes | In-scope URL where the payload was blocked |
| `original_payload` | yes | The payload that was blocked by the WAF |
| `suspected_vuln_type` | yes | Vulnerability class: sql_injection, xss, file_upload, ssrf, path_traversal, command_injection |
| `waf_fingerprint` | no | WAF product name from `waf_observation_plan` output (cloudflare, modsecurity, aws_waf, akamai, imperva, f5_asm) |
| `observations` | no | Context from previous blocked attempts |
| `confirm_authorized` | yes | Must be `true` |
| `max_per_category` | no | Max attempts per technique category (default 5, max 10) |

## Transformation Categories

1. **Encoding bypasses** — Double URL encoding, Unicode/UTF-8, HTML entities, mixed case, NULL bytes, comment injection
2. **HTTP protocol bypasses** — Chunked transfer, Content-Type switching, method switching, method override headers
3. **Parameter bypasses** — HPP (HTTP Parameter Pollution), array syntax, JSON nesting, multipart wrapping, URL-encoded parameter names
4. **Payload mutations** — Space substitution (tab/newline/comment/formfeed/CR), function name alternatives, string concatenation, SQL wildcard substitution
5. **Logic layer** — Multi-request probing, whitelist path abuse, slow-rate request timing

## WAF-Aware Prioritization

When a WAF fingerprint is provided, the engine prioritizes techniques known to be effective against that WAF:

- **Cloudflare**: Parameter bypass techniques prioritized highest
- **ModSecurity**: Parameter and payload mutation techniques prioritized
- **AWS WAF**: Encoding bypasses prioritized
- **Akamai**: HTTP protocol bypasses prioritized
- **Imperva**: Payload mutation techniques prioritized

## Output

- `status`: `ready` (attempts available) or `no_viable_bypasses`
- `next_tool_calls`: Array of `http_request` calls, each with a transformed payload and optional request modifications (method override, Content-Type switch, Transfer-Encoding, additional headers)
- `attempts`: Detailed records of each bypass attempt with rule name, category, priority, description, transformed payload, and expected behavior
- `guardrails`: Bounded limits per category and safety constraints
- `stop_conditions`: Conditions that must halt testing
- `techniques_used`: Count of attempts per technique category
- `evidence_refs`: Evidence references for the bypass plan

## Safety

- `confirm_authorized=true` required
- Max attempts per technique category (default 5, configurable 1-10)
- Destructive payloads rejected (DROP TABLE, xp_cmdshell, rm -rf, webshells, reverse shells, etc.)
- All attempts recorded as evidence with full audit trail
- Stop on repeated blocking, rate limits, or scope concerns
- HTTP protocol and logic layer categories capped lower (3 and 2 respectively)

## Source Knowledge

Bypass techniques derived from the AboutSecurity WAF bypass methodology:
- `wiki/skills/exploit/web-method/waf-bypass-methodology/SKILL.md`
- References: encoding-bypass-payloads, http-protocol-bypass, parameter-bypass, payload-mutation
