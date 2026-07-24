---
name: k8s-webhook-abuse_policy
description: PentAGI safety policy for K8S Webhook Abuse. Kubernetes Admission Webhook 滥用与策略引擎利用。当集群存在 Kyverno/OPA Gatekeeper/自定义 Webhook、DNS 扫描发现 kyverno-svc 或 gatekeeper 服务、或需要从 Mutating Webhook 提取注入的 Secret 时使用。核心手法：伪造 AdmissionReview 请求。任何在 K8s 中发现 Webho
when-to-use: Use when K8S Webhook Abuse methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# K8S Webhook Abuse — PentAGI Safety Policy

> Policy compiled from `cloud/k8s-webhook-abuse/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/k8s-webhook-abuse/SKILL.md` for knowledge reference
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

Source: `cloud/k8s-webhook-abuse/SKILL.md`
