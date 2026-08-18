#!/bin/bash
# M1 内核验证脚本：任务创建 → Agent 自主执行 → SSE 实时流 → 报告生成
set -e

BASE="http://127.0.0.1:8080"
TOKEN=$(curl -s -X POST "$BASE/system/auth/login" -H 'Content-Type: application/json' -d '{"username":"admin","password":"Admin#123456"}' | jq -r '.data.access_token')
AUTH="Authorization: Bearer $TOKEN"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║          M1 内核验收 — 端到端渗透测试验证                  ║"
echo "╚══════════════════════════════════════════════════════════════╝"

# ── 1. Infrastructure Health ──────────────────────────────────────
echo ""
echo "▶ 1. 基础设施健康检查"
HEALTH=$(curl -s "$BASE/health")
echo "   网关: $(echo $HEALTH | jq -r .status)"
PG=$(psql 'postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform' -t -c "SELECT 'ok'" 2>&1 | head -1)
echo "   数据库: ${PG:-ok}"
REDIS=$(redis-cli -a kairos_platform ping 2>/dev/null || echo "unavailable")
echo "   Redis: $REDIS"

# ── 2. Create Engagement Project, Task and Run ───────────────────
echo ""
echo "▶ 2. 创建授权项目、渗透任务与 Run"
PROJECT=$(curl -s -X POST "$BASE/pent/projects" -H 'Content-Type: application/json' -H "$AUTH" -d '{
  "name": "Juice Shop M1 授权项目",
  "description": "本地靶场自动验收",
  "asset_scope": ["http://localhost:13000"]
}')
PROJECT_ID=$(echo "$PROJECT" | jq -r '.data.id')
TASK=$(curl -s -X POST "$BASE/pent/tasks" -H 'Content-Type: application/json' -H "$AUTH" -d '{
  "project_id": "'"$PROJECT_ID"'",
  "name": "Juice Shop M1 验收",
  "priority": "high"
}')
TASK_ID=$(echo "$TASK" | jq -r '.data.id')
RUN=$(curl -s -X POST "$BASE/pent/runs" -H 'Content-Type: application/json' -H "$AUTH" -d '{
  "task_id": "'"$TASK_ID"'"
}')
RUN_ID=$(echo "$RUN" | jq -r '.data.id')
echo "   Project ID: $PROJECT_ID"
echo "   Task ID: $TASK_ID"
echo "   Run ID:  $RUN_ID"

# ── 3. Wait for Agent to Pick Up ─────────────────────────────────
echo ""
echo "▶ 3. 等待 Worker 接管（最多 60s）"
for i in $(seq 1 30); do
  sleep 2
  STATUS=$(curl -s "$BASE/pent/tasks/$TASK_ID" -H "$AUTH" | jq -r '.data.latest_run.status // "unknown"')
  echo -ne "   [$i/30] Run status: $STATUS\r"
  if [ "$STATUS" = "running" ]; then
    echo ""
    echo "   ✅ Worker 已接管，Agent 正在执行"
    break
  elif [ "$STATUS" = "completed" ] || [ "$STATUS" = "failed" ]; then
    echo ""
    echo "   终态: $STATUS"
    break
  fi
done

RUN_ID=$(curl -s "$BASE/pent/tasks/$TASK_ID" -H "$AUTH" | jq -r '.data.latest_run.id')
echo "   Run ID: $RUN_ID"

# ── 4. Watch SSE Stream (60s) ────────────────────────────────────
echo ""
echo "▶ 4. SSE 实时流（60s 采样，展示关键事件）"
TOOL_COUNT=0
timeout 60 curl -s -N "$BASE/pent/tasks/$TASK_ID/stream" -H "$AUTH" 2>/dev/null | while IFS= read -r line; do
  if [[ "$line" == event:* ]]; then
    EVT="${line#event: }"
  elif [[ "$line" == data:* && "$line" != "data: [DONE]" ]]; then
    DATA="${line#data: }"
    case "$EVT" in
      conversation.tool_calls)
        TOOLS=$(echo "$DATA" | jq -r '.content' 2>/dev/null | jq -r '.tool_calls[].function.name' 2>/dev/null | paste -sd "," -)
        TOOL_COUNT=$((TOOL_COUNT + $(echo "$TOOLS" | tr ',' '\n' | wc -l)))
        echo "   🔧 工具调用: $TOOLS"
        ;;
      conversation.tool_result)
        TOOL_NAME=$(echo "$DATA" | jq -r '.tool_name' 2>/dev/null)
        SUCCESS=$(echo "$DATA" | jq -r '.success' 2>/dev/null)
        echo "   📋 $TOOL_NAME → $( [ "$SUCCESS" = "true" ] && echo '✅ 成功' || echo '❌ 失败')"
        ;;
      conversation.assistant_message)
        PREVIEW=$(echo "$DATA" | jq -r '.content' 2>/dev/null | head -c 120)
        echo "   💬 模型: ${PREVIEW}..."
        ;;
      done) echo "   🏁 SSE 流结束"; break ;;
      *) ;;
    esac
  fi
done
echo ""

# ── 5. Check Findings & Progress ─────────────────────────────────
echo "▶ 5. 运行状态检查"
FULL=$(curl -s "$BASE/pent/tasks/$TASK_ID" -H "$AUTH")
echo "   任务状态: $(echo $FULL | jq -r '.data.status')"
echo "   Run状态:  $(echo $FULL | jq -r '.data.latest_run.status')"
echo "   Session:  $(echo $FULL | jq -r '.data.latest_run.session_id // "pending"')"

if [ -n "$RUN_ID" ] && [ "$RUN_ID" != "null" ]; then
  FINDINGS=$(curl -s "$BASE/pent/runs/$RUN_ID/findings" -H "$AUTH" | jq '.data | length')
  echo "   发现漏洞: $FINDINGS 个"

  EVIDENCE=$(curl -s "$BASE/pent/runs/$RUN_ID/evidence" -H "$AUTH" | jq '.data | length')
  echo "   Evidence:  $EVIDENCE 条"

  # ── 6. Report ──────────────────────────────────────────────────
  echo ""
  echo "▶ 6. 报告"
  REPORT=$(curl -s "$BASE/pent/reports/$RUN_ID" -H "$AUTH")
  echo "   Artifact: $(echo $REPORT | jq -r '.data.artifact_id // "pending"')"
  echo "   Size:     $(echo $REPORT | jq -r '.data.size_bytes // 0') bytes"
  echo "   Format:   $(echo $REPORT | jq -r '.data.format // "pending"')"
fi

# ── 7. Cancel (if still running) ──────────────────────────────────
CUR_STATUS=$(curl -s "$BASE/pent/tasks/$TASK_ID" -H "$AUTH" | jq -r '.data.status')
if [ "$CUR_STATUS" = "running" ]; then
  echo ""
  echo "▶ 7. 取消任务（验证取消功能）"
  curl -s -X POST "$BASE/pent/tasks/$TASK_ID/cancel" -H "$AUTH" | jq .
fi

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                   M1 验收完成                               ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo "Task:   $TASK_ID"
echo "Run:    $RUN_ID"
echo "Report: http://localhost:5666/pent/runs (搜索 $RUN_ID)"
