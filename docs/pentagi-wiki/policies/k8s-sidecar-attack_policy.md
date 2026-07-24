---
name: k8s-sidecar-attack_policy
description: PentAGI safety policy for K8S Sidecar Attack. Kubernetes Sidecar 容器流量劫持与敏感信息窃取。当目标 Pod 存在 Istio/Envoy/Linkerd sidecar、题目提到'隐形旁观者'或'共享网络'、或需要从 Pod 内部嗅探流量时使用。覆盖 tcpdump 抓包、sidecar 明文流量捕获、共享网络命名空间利用。只要在 K8s Pod 中发现有 sidecar 或多容器共存的迹象，就应使用此技能
when-to-use: Use when K8S Sidecar Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# K8S Sidecar Attack — PentAGI Safety Policy

> Policy compiled from `cloud/k8s-sidecar-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/k8s-sidecar-attack/SKILL.md` for knowledge reference
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

Source: `cloud/k8s-sidecar-attack/SKILL.md`
