---
name: python-prototype-pollution_policy
description: PentAGI safety policy for Python Prototype Pollution. Python 原型链污染（属性注入/Class Pollution）检测与利用。当目标为 Python Web 应用（Flask/Sanic/Django/FastAPI）且存在递归合并(merge)、深度属性设置(pydash.set_)、JSON 配置更新接口时使用。覆盖污染入口识别、__globals__链构造、Flask SECRET_KEY/Jinja2定界符/searchpath污染、
when-to-use: Use when Python Prototype Pollution methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Python Prototype Pollution — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/python-prototype-pollution/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/python-prototype-pollution/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/python-prototype-pollution/SKILL.md`
