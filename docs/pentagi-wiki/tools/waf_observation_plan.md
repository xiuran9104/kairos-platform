# waf_observation_plan

Builds a non-executing WAF observation and safe validation plan from existing evidence.

Use this tool after a baseline request, scanner result, or manual reproduction shows possible WAF/CDN/security-control behavior such as `403`, `406`, `429`, block-page text, or recognizable response headers.

This tool does not send traffic and does not generate bypass payloads. It classifies observations and returns:

- likely WAF/CDN fingerprints
- block signals
- evidence requirements
- allowed next tools
- blocked automation categories
- stop conditions

Important semantics:

- Without `confirm_authorized=true`: `agent_execution_allowed=false`, `payload_generation_allowed=false`, `mutation_table_allowed=false`, `manual_review_required=true`.
- With `confirm_authorized=true`: `agent_execution_allowed=true`, `payload_generation_allowed=true`, `mutation_table_allowed=true`. Bounded bypass planning via `waf_bypass_engine` is permitted. Destructive payloads and unbounded loops remain blocked.

Allowed next tools are limited to evidence and bounded baseline capture. When authorized, `waf_bypass_engine` is also available for bounded, recorded bypass attempt generation.

AboutSecurity raw WAF material is retained only as raw reference context:

- `wiki/skills/exploit/web-method/waf-bypass-methodology/SKILL.md`
- `references/encoding-bypass-payloads.md`
- `references/http-protocol-bypass.md`
- `references/parameter-bypass.md`
- `references/payload-mutation.md`

Do not convert raw headings, payloads, or bypass notes into automated payload mutation, encoding loops, protocol bypass loops, or rate escalation.
