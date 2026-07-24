---
name: webshell-deploy_policy
description: PentAGI safety policy for Webshell Deploy. Webshell 部署与利用方法论。当漏洞利用需要上传/写入 webshell 到目标服务器时使用。覆盖 JSP/PHP/ASPX webshell 生成、HTTP PUT 上传（含绕过技巧）、文件上传表单、日志注入+LFI 写入，以及 webshell 验证和利用。当发现文件上传漏洞、PUT 方法可用、LFI 可利用、或需要在目标部署持久化命令执行入口时，都应使用此 skill
when-to-use: Use when Webshell Deploy methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Webshell Deploy — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/webshell-deploy/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/webshell-deploy/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/webshell-deploy/SKILL.md`
