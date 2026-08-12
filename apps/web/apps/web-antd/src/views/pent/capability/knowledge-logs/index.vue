<script lang="ts" setup>
import type { PentKnowledgeRetrievalLog } from '#/api/pent';

import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  deletePentKnowledgeRetrievalLog,
  getPentKnowledgeRetrievalLogs,
} from '#/api/pent';

// 相对时间（对齐原版 getTimeAgo）
function relativeTime(t?: string) {
  if (!t) return '—';
  const diff = Date.now() - new Date(t).getTime();
  if (Number.isNaN(diff) || diff < 0) return formatTime(t);
  const s = Math.floor(diff / 1000);
  if (s < 60) return '刚刚';
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  return `${Math.floor(h / 24)} 天前`;
}

function formatTime(t?: string) {
  return (t || '').slice(0, 19).replace('T', ' ') || '—';
}

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const logs = ref<PentKnowledgeRetrievalLog[]>([]);
const conversationId = ref('');
const messageId = ref('');

async function fetchLogs() {
  loading.value = true;
  try {
    const params: Record<string, any> = { limit: 100 };
    if (conversationId.value.trim()) params.conversation_id = conversationId.value.trim();
    if (messageId.value.trim()) params.message_id = messageId.value.trim();
    logs.value = (await getPentKnowledgeRetrievalLogs(params)) || [];
  } catch {
    logs.value = [];
    if (conversationId.value || messageId.value) {
      message.error('加载检索日志失败');
    }
  } finally {
    loading.value = false;
  }
}

// ── stats（对齐原版：总检索次数 / 成功检索 / 成功率 / 检索到知识项）─────────
const hasResults = (l: PentKnowledgeRetrievalLog) =>
  (l.retrieved_items || []).length > 0;

const stats = computed(() => {
  const total = logs.value.length;
  const success = logs.value.filter(hasResults).length;
  const items = logs.value.reduce(
    (sum, l) => sum + (l.retrieved_items || []).length,
    0,
  );
  return {
    total,
    success,
    rate: total ? ((success / total) * 100).toFixed(1) : '0',
    items,
  };
});

// ── actions ────────────────────────────────────────────────────────────────
async function copyText(text?: string) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    message.success('已复制');
  } catch {
    message.error('复制失败');
  }
}

async function doDelete(log: PentKnowledgeRetrievalLog) {
  try {
    await deletePentKnowledgeRetrievalLog(log.id);
    message.success('删除成功');
    fetchLogs();
  } catch {
    message.error('删除检索日志失败');
  }
}

// ── detail modal ───────────────────────────────────────────────────────────
const showDetail = ref(false);
const detailLog = ref<PentKnowledgeRetrievalLog | null>(null);

function openDetail(log: PentKnowledgeRetrievalLog) {
  detailLog.value = log;
  showDetail.value = true;
}

onMounted(fetchLogs);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="检索历史" sub-title="Agent 知识检索调用记录">
      <template #extra>
        <a-button :loading="loading" @click="fetchLogs">刷新</a-button>
      </template>
    </a-page-header>

    <!-- 统计栏 -->
    <div
      style="
        margin-top: 16px;
        display: flex;
        gap: 32px;
        flex-wrap: wrap;
        font-size: 13px;
        color: var(--ant-color-text-secondary);
      "
    >
      <span>总检索次数 <strong>{{ stats.total }}</strong></span>
      <span>
        成功检索
        <strong style="color: var(--ant-color-success)">{{ stats.success }}</strong>
      </span>
      <span>成功率 <strong>{{ stats.rate }}%</strong></span>
      <span>检索到知识项 <strong>{{ stats.items }}</strong></span>
    </div>

    <!-- 筛选 -->
    <div style="margin-top: 12px; display: flex; gap: 16px; flex-wrap: wrap; align-items: center">
      <label style="display: flex; align-items: center; gap: 8px">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">对话ID</span>
        <a-input
          v-model:value="conversationId"
          placeholder="可选：筛选特定对话"
          style="width: 220px"
          allow-clear
          @press-enter="fetchLogs"
        />
      </label>
      <label style="display: flex; align-items: center; gap: 8px">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">消息ID</span>
        <a-input
          v-model:value="messageId"
          placeholder="可选：筛选特定消息"
          style="width: 220px"
          allow-clear
          @press-enter="fetchLogs"
        />
      </label>
      <a-button @click="fetchLogs">筛选</a-button>
    </div>

    <!-- 日志卡片列表 -->
    <a-spin :spinning="loading">
      <div style="margin-top: 16px; display: flex; flex-direction: column; gap: 12px">
        <a-card v-for="log in logs" :key="log.id" size="small">
          <!-- 卡片头：图标 + 查询 + 元信息 + 结果徽标 -->
          <div style="display: flex; align-items: flex-start; gap: 12px">
            <span style="font-size: 20px; line-height: 1.4">
              {{ hasResults(log) ? '🔍' : '⚠️' }}
            </span>
            <div style="flex: 1; min-width: 0">
              <div style="font-weight: 600; word-break: break-all">
                {{ log.query || '（无查询内容）' }}
              </div>
              <div
                style="
                  margin-top: 4px;
                  font-size: 12px;
                  color: var(--ant-color-text-tertiary);
                  display: flex;
                  gap: 12px;
                  flex-wrap: wrap;
                "
              >
                <span :title="formatTime(log.created_at)">🕒 {{ relativeTime(log.created_at) }}</span>
                <span v-if="log.risk_type">📁 {{ log.risk_type }}</span>
              </div>
            </div>
            <a-tag :color="hasResults(log) ? 'green' : 'default'" style="white-space: nowrap">
              {{ hasResults(log) ? `${log.retrieved_items.length} 项` : '无结果' }}
            </a-tag>
          </div>

          <!-- 卡片体：明细 -->
          <div
            style="
              margin-top: 12px;
              display: flex;
              gap: 24px;
              flex-wrap: wrap;
              font-size: 12px;
            "
          >
            <div v-if="log.conversation_id">
              <div style="color: var(--ant-color-text-tertiary); margin-bottom: 2px">对话ID</div>
              <a-typography-text
                code
                style="cursor: pointer; font-size: 12px"
                title="点击复制"
                @click="copyText(log.conversation_id)"
              >
                {{ log.conversation_id }}
              </a-typography-text>
            </div>
            <div v-if="log.message_id">
              <div style="color: var(--ant-color-text-tertiary); margin-bottom: 2px">消息ID</div>
              <a-typography-text
                code
                style="cursor: pointer; font-size: 12px"
                title="点击复制"
                @click="copyText(log.message_id)"
              >
                {{ log.message_id }}
              </a-typography-text>
            </div>
            <div>
              <div style="color: var(--ant-color-text-tertiary); margin-bottom: 2px">检索结果</div>
              <span
                :style="{
                  color: hasResults(log)
                    ? 'var(--ant-color-success)'
                    : 'var(--ant-color-text-quaternary)',
                }"
              >
                {{ hasResults(log) ? `找到 ${log.retrieved_items.length} 个知识项` : '无匹配' }}
              </span>
            </div>
          </div>

          <!-- 命中项预览 -->
          <div v-if="hasResults(log)" style="margin-top: 8px; font-size: 12px">
            <span style="color: var(--ant-color-text-tertiary); margin-right: 8px">检索到的知识项</span>
            <a-tag
              v-for="(item, i) in log.retrieved_items.slice(0, 3)"
              :key="item.id || i"
              size="small"
              :title="item.title || item.id"
            >
              {{ i + 1 }}
            </a-tag>
            <a-tag v-if="log.retrieved_items.length > 3" size="small">
              +{{ log.retrieved_items.length - 3 }}
            </a-tag>
          </div>

          <!-- 操作 -->
          <div
            style="
              margin-top: 8px;
              border-top: 1px solid var(--ant-color-border-secondary);
              padding-top: 8px;
            "
          >
            <a-button size="small" type="link" @click="openDetail(log)">查看详情</a-button>
            <a-popconfirm title="确定要删除这条检索记录吗？" @confirm="doDelete(log)">
              <a-button size="small" type="link" danger>删除</a-button>
            </a-popconfirm>
          </div>
        </a-card>
      </div>
      <a-empty v-if="!logs.length && !loading" description="暂无检索记录" style="margin-top: 48px" />
    </a-spin>

    <!-- 检索详情弹窗 -->
    <a-modal v-model:open="showDetail" title="检索详情" width="900px" :footer="null">
      <template v-if="detailLog">
        <!-- 查询信息 -->
        <div>
          <h4 style="margin: 0 0 12px">查询信息</h4>
          <div
            style="
              padding: 12px;
              background: var(--ant-color-fill-quaternary);
              border-radius: 6px;
              border-left: 3px solid var(--ant-color-primary);
            "
          >
            <div style="font-weight: 500; margin-bottom: 8px">查询内容</div>
            <div style="line-height: 1.6; word-break: break-word">
              {{ detailLog.query || '（无查询内容）' }}
            </div>
          </div>
        </div>

        <!-- 检索信息 -->
        <div style="margin-top: 20px">
          <h4 style="margin: 0 0 12px">检索信息</h4>
          <a-row :gutter="[12, 12]">
            <a-col v-if="detailLog.risk_type" :xs="24" :sm="8">
              <div style="padding: 12px; background: var(--ant-color-fill-quaternary); border-radius: 6px">
                <div style="font-size: 12px; color: var(--ant-color-text-secondary); margin-bottom: 4px">
                  风险类型
                </div>
                <div style="font-weight: 500">{{ detailLog.risk_type }}</div>
              </div>
            </a-col>
            <a-col :xs="24" :sm="8">
              <div style="padding: 12px; background: var(--ant-color-fill-quaternary); border-radius: 6px">
                <div style="font-size: 12px; color: var(--ant-color-text-secondary); margin-bottom: 4px">
                  检索时间
                </div>
                <div style="font-weight: 500" :title="formatTime(detailLog.created_at)">
                  {{ relativeTime(detailLog.created_at) }}
                </div>
              </div>
            </a-col>
            <a-col :xs="24" :sm="8">
              <div style="padding: 12px; background: var(--ant-color-fill-quaternary); border-radius: 6px">
                <div style="font-size: 12px; color: var(--ant-color-text-secondary); margin-bottom: 4px">
                  检索结果
                </div>
                <div style="font-weight: 500">
                  {{ (detailLog.retrieved_items || []).length }} 个知识项
                </div>
              </div>
            </a-col>
          </a-row>
        </div>

        <!-- 关联信息 -->
        <div v-if="detailLog.conversation_id || detailLog.message_id" style="margin-top: 20px">
          <h4 style="margin: 0 0 12px">关联信息</h4>
          <a-row :gutter="[12, 12]">
            <a-col v-if="detailLog.conversation_id" :xs="24" :sm="12">
              <div style="padding: 12px; background: var(--ant-color-fill-quaternary); border-radius: 6px">
                <div style="font-size: 12px; color: var(--ant-color-text-secondary); margin-bottom: 4px">
                  对话ID
                </div>
                <a-typography-text
                  code
                  style="cursor: pointer; font-size: 12px; word-break: break-all"
                  title="点击复制"
                  @click="copyText(detailLog.conversation_id)"
                >
                  {{ detailLog.conversation_id }}
                </a-typography-text>
              </div>
            </a-col>
            <a-col v-if="detailLog.message_id" :xs="24" :sm="12">
              <div style="padding: 12px; background: var(--ant-color-fill-quaternary); border-radius: 6px">
                <div style="font-size: 12px; color: var(--ant-color-text-secondary); margin-bottom: 4px">
                  消息ID
                </div>
                <a-typography-text
                  code
                  style="cursor: pointer; font-size: 12px; word-break: break-all"
                  title="点击复制"
                  @click="copyText(detailLog.message_id)"
                >
                  {{ detailLog.message_id }}
                </a-typography-text>
              </div>
            </a-col>
          </a-row>
        </div>

        <!-- 检索到的知识项 -->
        <div style="margin-top: 20px">
          <h4 style="margin: 0 0 12px">
            检索到的知识项 ({{ (detailLog.retrieved_items || []).length }})
          </h4>
          <template v-if="(detailLog.retrieved_items || []).length">
            <div
              v-for="(item, idx) in detailLog.retrieved_items"
              :key="item.id || idx"
              style="
                margin-bottom: 12px;
                padding: 16px;
                border: 1px solid var(--ant-color-border-secondary);
                border-radius: 8px;
                background: var(--ant-color-fill-quaternary);
              "
            >
              <div style="display: flex; justify-content: space-between; align-items: flex-start">
                <span style="font-weight: 600">{{ idx + 1 }}. {{ item.title || '（无标题）' }}</span>
                <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                  {{ item.id || '—' }}
                </span>
              </div>
            </div>
          </template>
          <a-empty v-else description="无命中项详情" :image-style="{ height: '60px' }" />
        </div>
      </template>
    </a-modal>
  </div>
</template>
