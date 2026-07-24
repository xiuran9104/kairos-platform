---
name: information-disclosure-methodology_policy
description: PentAGI safety policy for Information Disclosure Methodology. Web 应用敏感信息泄露检测与利用。当发现 .git/.svn 目录、备份文件路径(.bak/.zip/.tar.gz)、.env 配置文件、Swagger/OpenAPI 文档、debug 页面等信息泄露点时使用。也适用于发现源码泄露后的深入利用：git 历史审计（git log -p -S 搜索已删除的密码和 flag）、svn wc.db 文件列表提取、.DS_Store 目录枚举。优先于漏
when-to-use: Use when Information Disclosure Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Information Disclosure Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/information-disclosure-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/information-disclosure-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/information-disclosure-methodology/SKILL.md`
