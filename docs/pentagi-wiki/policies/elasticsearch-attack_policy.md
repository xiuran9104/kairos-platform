---
name: elasticsearch-attack_policy
description: PentAGI safety policy for Elasticsearch Attack. Elasticsearch 未授权访问与利用。当发现目标开放 9200/9300 端口、Elasticsearch 服务无认证、需要从 ES 获取索引数据或实现远程命令执行时使用。覆盖未授权访问、认证绕过、索引数据窃取、MVEL/Groovy 脚本 RCE（旧版本）、快照仓库滥用、集群信息泄露、动态脚本注入、Ingest Pipeline 滥用、路径穿越
when-to-use: Use when Elasticsearch Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Elasticsearch Attack — PentAGI Safety Policy

> Policy compiled from `exploit/network-service/elasticsearch-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/network-service/elasticsearch-attack/SKILL.md` for knowledge reference
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

Source: `exploit/network-service/elasticsearch-attack/SKILL.md`
