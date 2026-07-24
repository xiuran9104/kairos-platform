---
name: jwt-attack-methodology_policy
description: PentAGI safety policy for Jwt Attack Methodology. JWT Token 攻击方法论。当响应头/Cookie 中出现 eyJ 开头的字符串（三段式 base64，Header.Payload.Signature）、Authorization: Bearer token、API 返回 token/access_token 字段时使用。包含 alg:none 绕过、弱密钥爆破（hashcat/john/c-jwt-cracker/jwt_tool 完整工
when-to-use: Use when Jwt Attack Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# JWT Attack Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/auth/jwt-attack-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/auth/jwt-attack-methodology/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

## Escalation Path

If the methodology requires blocked operations:
1. Stop automated execution
2. Document the specific need and target
3. Request explicit human authorization with scope boundaries
4. Wait for approval before proceeding

## Evidence Requirements

- Record every probe attempt with request/response evidence
- Link findings to the source methodology path
- Mark unverified observations as `needs_review`

---

Source: `exploit/auth/jwt-attack-methodology/SKILL.md`
