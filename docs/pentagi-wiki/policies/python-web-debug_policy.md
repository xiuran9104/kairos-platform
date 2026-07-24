---
name: python-web-debug_policy
description: PentAGI safety policy for Python Web Debug. Python Web 框架 Debug 模式利用。当目标是 Flask/Django/FastAPI/Tornado 应用且开启了 Debug 模式时使用。当看到 Werkzeug Debugger 页面（交互式 Python console）、Django 黄色调试页面（Settings/Traceback）、FastAPI 自动文档(/docs /redoc)、或错误页面泄露了 Python 
when-to-use: Use when Python Web Debug methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Python Web Debug — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/python-web-debug/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/python-web-debug/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/python-web-debug/SKILL.md`
