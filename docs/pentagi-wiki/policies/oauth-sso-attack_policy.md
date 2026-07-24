---
name: oauth-sso-attack_policy
description: PentAGI safety policy for Oauth Sso Attack. OAuth 2.0 / SSO / OpenID Connect 认证流程攻击。当目标有「使用 Google/GitHub/微信 登录」按钮、redirect_uri 参数、authorization_code 流程、或 /.well-known/openid-configuration 端点时使用。覆盖 redirect_uri 劫持、state 缺失 CSRF、token 泄露、scope 提
when-to-use: Use when Oauth Sso Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# OAuth Sso Attack — PentAGI Safety Policy

> Policy compiled from `exploit/auth/oauth-sso-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/auth/oauth-sso-attack/SKILL.md` for knowledge reference
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

Source: `exploit/auth/oauth-sso-attack/SKILL.md`
