---
name: cookie-analysis_policy
description: PentAGI safety policy for Cookie Analysis. Session cookie 分析与伪造方法论。当发现 Web 应用使用 cookie 进行认证、需要判断 cookie 类型并选择伪造方法时使用。覆盖 unsigned base64 cookie 直接伪造、Flask 签名 cookie（flask-unsign 爆破密钥）、加密/二进制 cookie 的识别。本技能负责 cookie 类型判断和分流：如果判断为 JWT（三段式 eyJ 开头）
when-to-use: Use when Cookie Analysis methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Cookie Analysis — PentAGI Safety Policy

> Policy compiled from `exploit/auth/cookie-analysis/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/auth/cookie-analysis/SKILL.md` for knowledge reference
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

Source: `exploit/auth/cookie-analysis/SKILL.md`
