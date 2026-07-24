---
name: privilege-escalation-web_policy
description: PentAGI safety policy for Privilege Escalation Web. Web 应用层权限提升。当 Web 应用存在用户注册/登录、角色权限管理、API 参数传输用户身份信息时使用。覆盖 Mass Assignment（批量赋值）攻击、角色/权限字段注入、HTTP 方法/Header 绕过、Cookie/Session 篡改。注册后立即检查普通用户能否越权访问管理员功能——这是第一优先级
when-to-use: Use when Privilege Escalation Web methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Privilege Escalation Web — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/privilege-escalation-web/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/privilege-escalation-web/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/privilege-escalation-web/SKILL.md`
