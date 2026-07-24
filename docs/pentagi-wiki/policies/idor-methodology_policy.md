---
name: idor-methodology_policy
description: PentAGI safety policy for Idor Methodology. IDOR 不安全直接对象引用与未授权访问检测与利用。当 API/URL 中出现 user_id、account、订单号、文件名等可预测标识符，或需要测试水平越权（访问他人数据）、垂直越权（普通用户获取 admin 管理员权限）、接口越权时使用。覆盖 ID 遍历、绕过技巧（参数污染/编码/方法切换）、多步 IDOR 链、文件资源 IDOR、批量操作越权、间接引用枚举、框架特征利用、UUID 猜测、响
when-to-use: Use when Idor Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# IDOR Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/auth/idor-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/auth/idor-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/auth/idor-methodology/SKILL.md`
