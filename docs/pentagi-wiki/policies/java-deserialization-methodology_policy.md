---
name: java-deserialization-methodology_policy
description: PentAGI safety policy for Java Deserialization Methodology. Java 反序列化漏洞检测与利用。当目标是 Java 应用（Tomcat/Spring/WebLogic/JBoss/Jenkins）、发现 rO0AB/aced0005 开头的 Base64 数据、HTTP 请求中有 application/x-java-serialized-object Content-Type、或存在 T3/IIOP/RMI/JNDI 协议端口时使用。覆盖 ysoseria
when-to-use: Use when Java Deserialization Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Java Deserialization Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/java-deserialization-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/java-deserialization-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/java-deserialization-methodology/SKILL.md`
