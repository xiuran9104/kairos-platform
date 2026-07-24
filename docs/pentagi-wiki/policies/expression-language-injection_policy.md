---
name: expression-language-injection_policy
description: PentAGI safety policy for Expression Language Injection. 表达式语言(EL)注入方法论。当目标使用 Spring(SpEL)、Struts2(OGNL)、Confluence(OGNL)、JSP/JSF(Java EL) 且存在用户可控的表达式求值时使用。覆盖多语法探测与区分(${7*7}/#{7*7}/%{7*7})、SpEL RCE(Runtime.exec/ProcessBuilder/反射绕沙箱)、Spring Cloud Gateway CVE
when-to-use: Use when Expression Language Injection methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Expression Language Injection — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/expression-language-injection/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/expression-language-injection/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/expression-language-injection/SKILL.md`
