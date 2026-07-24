---
name: k8s-container-escape_policy
description: PentAGI safety policy for K8S Container Escape. Kubernetes 容器逃逸与集群攻击。当目标运行在 K8s 环境中、发现 6443/10250/2379 端口、获取到 ServiceAccount Token、或已在容器内时使用。覆盖容器逃逸、Pod 提权、API Server 未授权、etcd 泄露、RBAC 滥用、节点接管。任何涉及 Kubernetes、容器编排、云原生安全的场景都应使用此技能
when-to-use: Use when K8S Container Escape methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# K8S Container Escape — PentAGI Safety Policy

> Policy compiled from `cloud/k8s-container-escape/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/k8s-container-escape/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 创建后门/持久化资源
- 跨账户横向移动
- 修改 IAM/CAM 策略
- 删除 CloudTrail/审计日志

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

Source: `cloud/k8s-container-escape/SKILL.md`
