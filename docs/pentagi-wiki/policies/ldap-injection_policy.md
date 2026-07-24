---
name: ldap-injection_policy
description: PentAGI safety policy for Ldap Injection. LDAP 注入检测与利用。当目标有 LDAP 认证入口（企业登录/SSO/统一认证）、发现 389/636/3268/3269 端口、URL 参数中出现 uid/cn/ou/dc 等 LDAP 属性名、或后端使用 LDAP 进行用户查询时使用。LDAP 注入与 SQL 注入思路类似但语法不同——通过注入 LDAP 搜索过滤器操作符来绕过认证或提取数据。在域环境和企业内网中常见，与 AD 域攻击配合
when-to-use: Use when Ldap Injection methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# LDAP Injection — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/ldap-injection/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/ldap-injection/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/ldap-injection/SKILL.md`
