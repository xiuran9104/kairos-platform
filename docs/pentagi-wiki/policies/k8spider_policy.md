---
name: k8spider_policy
description: PentAGI safety policy for K8Spider. 使用 k8spider 进行 Kubernetes 集群 DNS 服务发现与侦察。当需要低权限枚举 K8s 集群中的 Service、探测 DNS 服务、扫描网段 PTR/SRV 记录、尝试 AXFR 区域传输时使用。k8spider 仅需 DNS 访问权限即可发现集群内所有服务，无需 API Server 权限。涉及 K8s 服务发现、DNS 侦察、集群信息收集、PTR 扫描、SRV 枚举的场景
when-to-use: Use when K8Spider methodology is triggered and active testing may occur.
allowed-tools:
  - command_check
  - evidence_search
  - evidence_read
  - todo_write
---

# K8Spider — PentAGI Safety Policy

> Policy compiled from `tool/k8spider/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `tool/k8spider/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 未经 command_check 验证直接执行工具
- 对非授权目标使用工具
- 高危参数（--os-shell/--os-cmd/破坏性操作）

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

Source: `tool/k8spider/SKILL.md`
