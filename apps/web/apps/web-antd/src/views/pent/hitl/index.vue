<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  decideHitlApproval,
  getHitlApprovals,
  getHitlSettings,
  updateHitlSettings,
} from '#/api/pent';

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const saving = ref(false);
const tab = ref<'logs' | 'pending' | 'strategy' | 'whitelist'>('pending');

const settings = ref<any>({
  approval_ttl_seconds: 1800,
  hitl_audit_model_id: undefined,
  hitl_audit_system_prompt: '',
  hitl_audit_user_prompt: '',
  hitl_enabled: false,
  hitl_mode: 'approval',
  hitl_reviewer: 'human',
  hitl_tool_whitelist: [],
});
const whitelistText = ref('');
const strategyMode = ref<'approval' | 'review_edit'>('approval');

const approvals = ref<any[]>([]);
const pendingSearch = ref('');
const pendingPage = ref(1);
const pendingPageSize = 10;

const logSearch = ref('');
const logDecision = ref('all');
const logDecidedBy = ref('all');
const logPage = ref(1);
const logPageSize = 10;

// ── data ───────────────────────────────────────────────────────────────────
async function refresh() {
  loading.value = true;
  try {
    const [policy, all] = await Promise.all([getHitlSettings(), getHitlApprovals()]);
    settings.value = { ...settings.value, ...policy };
    whitelistText.value = (policy.hitl_tool_whitelist || []).join('\n');
    strategyMode.value = policy.hitl_mode || 'approval';
    approvals.value = (all as any[]) || [];
  } catch (error: any) {
    message.error(error?.message || '加载 AI 围栏失败');
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);

const pendingList = computed(() => approvals.value.filter((a) => a.status === 'pending'));
const decidedList = computed(() => approvals.value.filter((a) => a.status !== 'pending'));

const filteredPending = computed(() => {
  const q = pendingSearch.value.trim().toLowerCase();
  if (!q) return pendingList.value;
  return pendingList.value.filter(
    (a) =>
      (a.tool_name || '').toLowerCase().includes(q) ||
      (a.session_id || a.conversation_id || '').toLowerCase().includes(q) ||
      JSON.stringify(a.original_arguments || {}).toLowerCase().includes(q),
  );
});
const pendingPaged = computed(() =>
  filteredPending.value.slice(
    (pendingPage.value - 1) * pendingPageSize,
    pendingPage.value * pendingPageSize,
  ),
);

const filteredLogs = computed(() => {
  let list = decidedList.value;
  if (logDecision.value !== 'all') {
    list = list.filter((a) => (a.decision || a.status) === logDecision.value);
  }
  if (logDecidedBy.value !== 'all') {
    list = list.filter((a) => (a.decided_by || '') === logDecidedBy.value);
  }
  const q = logSearch.value.trim().toLowerCase();
  if (q) {
    list = list.filter(
      (a) =>
        (a.tool_name || '').toLowerCase().includes(q) ||
        (a.session_id || a.conversation_id || '').toLowerCase().includes(q) ||
        (a.comment || '').toLowerCase().includes(q),
    );
  }
  return [...list].sort((a, b) =>
    (b.decided_at || b.updated_at || '').localeCompare(a.decided_at || a.updated_at || ''),
  );
});
const logPaged = computed(() =>
  filteredLogs.value.slice((logPage.value - 1) * logPageSize, logPage.value * logPageSize),
);

// ── 审批方切换 ──────────────────────────────────────────────────────────────
async function switchReviewer(reviewer: 'audit_agent' | 'human') {
  if (settings.value.hitl_reviewer === reviewer) return;
  settings.value.hitl_reviewer = reviewer;
  try {
    await updateHitlSettings(settings.value);
    message.success('审批方已切换');
  } catch (error: any) {
    message.error(error?.message || '切换失败');
  }
}

// ── 保存（策略 / 白名单）──────────────────────────────────────────────────────
async function save() {
  saving.value = true;
  try {
    settings.value.hitl_mode = strategyMode.value;
    settings.value.hitl_tool_whitelist = whitelistText.value
      .split(/[\n,]/)
      .map((v: string) => v.trim())
      .filter(Boolean);
    await updateHitlSettings(settings.value);
    message.success('已保存');
    await refresh();
  } catch (error: any) {
    message.error(error?.message || '保存失败');
  } finally {
    saving.value = false;
  }
}

// ── 审批 ────────────────────────────────────────────────────────────────────
const commentMap = ref<Record<string, string>>({});
const editArgsMap = ref<Record<string, string>>({});
const deciding = ref<null | string>(null);

async function decide(record: any, decision: 'approve' | 'reject') {
  if (deciding.value) return;
  deciding.value = record.id;
  try {
    const payload: any = {
      comment: commentMap.value[record.id] || (decision === 'approve' ? '人工批准' : '人工拒绝'),
      decision,
    };
    const editText = (editArgsMap.value[record.id] || '').trim();
    if (record.hitl_mode === 'review_edit' && editText) {
      try {
        payload.arguments = JSON.parse(editText);
      } catch {
        message.warning('修改后的参数不是合法 JSON，已按原参数提交');
      }
    }
    await decideHitlApproval(record.id, payload);
    message.success(decision === 'approve' ? '已批准' : '已拒绝');
    await refresh();
  } catch (error: any) {
    message.error(error?.message || '审批失败');
  } finally {
    deciding.value = null;
  }
}

// ── 日志详情 ────────────────────────────────────────────────────────────────
const logDetail = ref<any>(null);

function decisionLabel(a: any) {
  const d = a.decision || (a.status === 'approved' ? 'approve' : a.status === 'rejected' ? 'reject' : a.status);
  return { approve: '通过', reject: '拒绝' }[d as string] || d || '—';
}
const reviewerLabel: Record<string, string> = {
  audit_agent: '审计 Agent',
  human: '人工',
  manual: '手动录入',
  system: '系统',
};

function fmtTime(t?: string) {
  return t ? t.slice(0, 19).replace('T', ' ') : '—';
}
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="AI 围栏审批">
      <template #extra>
        <a-button :loading="loading" @click="refresh">刷新</a-button>
      </template>
    </a-page-header>

    <!-- 当前审批方 -->
    <a-card size="small" style="margin-top: 16px">
      <div style="display: flex; align-items: center; gap: 12px; flex-wrap: wrap">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">当前审批方</span>
        <a-radio-group
          :value="settings.hitl_reviewer"
          size="small"
          @change="(e: any) => switchReviewer(e.target.value)"
        >
          <a-radio-button value="human">人工审批</a-radio-button>
          <a-radio-button value="audit_agent">审计 Agent</a-radio-button>
        </a-radio-group>
        <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
          切换后立即生效；规则与白名单不变。AI 围栏为「关闭」时也可预先选择。
        </span>
        <span style="margin-left: auto; display: flex; align-items: center; gap: 8px">
          <span style="font-size: 13px">启用围栏</span>
          <a-switch
            :checked="settings.hitl_enabled"
            @change="(v: any) => ((settings.hitl_enabled = v), save())"
          />
        </span>
      </div>
    </a-card>

    <!-- Tabs -->
    <a-tabs v-model:activeKey="tab" style="margin-top: 8px">
      <!-- 待审计 -->
      <a-tab-pane key="pending">
        <template #tab>
          待审计
          <a-badge
            :count="pendingList.length"
            :number-style="{ background: 'var(--ant-color-error)' }"
            style="margin-left: 4px"
          />
        </template>
        <div style="display: flex; gap: 8px; margin-bottom: 12px">
          <a-input-search
            v-model:value="pendingSearch"
            placeholder="搜索工具 / 会话 / 参数"
            allow-clear
            style="width: 280px"
            @search="pendingPage = 1"
          />
        </div>
        <a-empty v-if="!filteredPending.length && !loading" description="暂无待审批项" />
        <div style="display: flex; flex-direction: column; gap: 12px">
          <a-card v-for="a in pendingPaged" :key="a.id" size="small">
            <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap">
              <a-tag color="blue" style="font-family: monospace">{{ a.tool_name || '-' }}</a-tag>
              <a-tag>{{ a.hitl_mode === 'review_edit' ? '审查编辑' : '审批' }}</a-tag>
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                会话：{{ a.session_id || a.conversation_id || '-' }}
              </span>
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                过期时间：{{ fmtTime(a.expires_at) }}
              </span>
            </div>
            <pre
              style="
                background: var(--ant-color-bg-layout);
                padding: 8px;
                border-radius: 4px;
                font-size: 12px;
                max-height: 160px;
                overflow: auto;
                white-space: pre-wrap;
                word-break: break-all;
                margin: 12px 0 8px;
              "
            >{{ typeof a.original_arguments === 'string' ? a.original_arguments : JSON.stringify(a.original_arguments, null, 2) }}</pre>
            <template v-if="a.hitl_mode === 'review_edit'">
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 4px">
                审查编辑模式：可提供 JSON 对象覆盖工具参数，例如 {"command":"ls -la"}
              </div>
              <a-textarea
                :value="editArgsMap[a.id] || ''"
                :rows="2"
                placeholder='{"command":"ls -la"}'
                style="font-family: monospace; margin-bottom: 8px"
                @update:value="(v: string) => (editArgsMap[a.id] = v)"
              />
            </template>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 4px">
              备注（可选）：简要说明审批理由
            </div>
            <a-input
              :value="commentMap[a.id] || ''"
              placeholder="例如：只读命令，允许执行"
              style="margin-bottom: 12px"
              @update:value="(v: string) => (commentMap[a.id] = v)"
            />
            <div style="display: flex; justify-content: flex-end; gap: 8px">
              <a-button :loading="deciding === a.id" @click="decide(a, 'reject')">拒绝</a-button>
              <a-button type="primary" :loading="deciding === a.id" @click="decide(a, 'approve')">
                批准
              </a-button>
            </div>
          </a-card>
        </div>
        <a-pagination
          v-if="filteredPending.length > pendingPageSize"
          v-model:current="pendingPage"
          :page-size="pendingPageSize"
          :total="filteredPending.length"
          style="margin-top: 12px; text-align: right"
          size="small"
        />
      </a-tab-pane>

      <!-- 审计策略 -->
      <a-tab-pane key="strategy" tab="审计策略">
        <a-card size="small">
          <template #extra>
            <a-button type="primary" size="small" :loading="saving" @click="save">保存</a-button>
          </template>
          <a-form layout="vertical">
            <a-form-item label="审计策略模式">
              <a-radio-group v-model:value="strategyMode">
                <a-radio-button value="approval">审批模式</a-radio-button>
                <a-radio-button value="review_edit">审查编辑模式</a-radio-button>
              </a-radio-group>
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
                {{
                  strategyMode === 'approval'
                    ? '白名单内工具免审批；审批模式下审计 Agent 仅裁决通过/拒绝。'
                    : '审查编辑模式下审计 Agent 可通过收窄参数后放行；无法安全改参时应拒绝。'
                }}
              </div>
            </a-form-item>
            <a-row :gutter="12">
              <a-col :xs="24" :sm="12">
                <a-form-item label="审批超时（秒）">
                  <a-input-number
                    v-model:value="settings.approval_ttl_seconds"
                    :max="86400"
                    :min="60"
                    style="width: 100%"
                  />
                </a-form-item>
              </a-col>
              <a-col :xs="24" :sm="12">
                <a-form-item label="审计模型 ID（留空则使用会话模型）">
                  <a-input-number v-model:value="settings.hitl_audit_model_id" style="width: 100%" />
                </a-form-item>
              </a-col>
            </a-row>
            <a-form-item label="审计系统提示词">
              <a-textarea
                v-model:value="settings.hitl_audit_system_prompt"
                :rows="8"
                placeholder="留空使用安全默认提示词"
                style="font-family: monospace"
              />
            </a-form-item>
            <a-form-item label="审计用户提示词（支持 {{tool_name}}、{{arguments}}、{{mode}}）">
              <a-textarea
                v-model:value="settings.hitl_audit_user_prompt"
                :rows="8"
                style="font-family: monospace"
              />
            </a-form-item>
          </a-form>
        </a-card>
      </a-tab-pane>

      <!-- 工具白名单 -->
      <a-tab-pane key="whitelist" tab="工具白名单">
        <a-card size="small">
          <template #extra>
            <a-button type="primary" size="small" :loading="saving" @click="save">保存</a-button>
          </template>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 8px">
            每行一个或逗号分隔；白名单工具不会触发 AI 围栏审批，保存后立即生效。
          </div>
          <a-textarea
            v-model:value="whitelistText"
            :rows="10"
            placeholder="例如：&#10;nmap&#10;curl,whatweb"
            style="font-family: monospace"
          />
        </a-card>
      </a-tab-pane>

      <!-- 审计日志 -->
      <a-tab-pane key="logs" tab="审计日志">
        <div style="display: flex; gap: 8px; margin-bottom: 12px; flex-wrap: wrap">
          <a-input-search
            v-model:value="logSearch"
            placeholder="搜索工具 / 会话 / 备注"
            allow-clear
            style="width: 260px"
            @search="logPage = 1"
          />
          <a-select
            v-model:value="logDecision"
            style="width: 120px"
            :options="[
              { label: '全部决策', value: 'all' },
              { label: '通过', value: 'approve' },
              { label: '拒绝', value: 'reject' },
            ]"
            @change="logPage = 1"
          />
          <a-select
            v-model:value="logDecidedBy"
            style="width: 140px"
            :options="[
              { label: '全部审批方', value: 'all' },
              { label: '人工', value: 'human' },
              { label: '审计 Agent', value: 'audit_agent' },
              { label: '系统', value: 'system' },
              { label: '手动录入', value: 'manual' },
            ]"
            @change="logPage = 1"
          />
        </div>
        <a-table
          :columns="[
            { title: 'ID', dataIndex: 'id', width: 110, ellipsis: true },
            { title: '工具', dataIndex: 'tool_name' },
            { title: '会话', dataIndex: 'session_id', ellipsis: true },
            { title: '决策', key: 'decision', width: 90 },
            { title: '审批方', dataIndex: 'decided_by', width: 110 },
            { title: '时间', key: 'time', width: 160 },
            { title: '操作', key: 'action', width: 80 },
          ]"
          :data-source="logPaged"
          :loading="loading"
          :pagination="{
            current: logPage,
            pageSize: logPageSize,
            total: filteredLogs.length,
            size: 'small',
            showTotal: (t: number) => `共 ${t} 条`,
          }"
          row-key="id"
          size="small"
          @change="(p: any) => (logPage = p.current)"
        >
          <template #bodyCell="{ column, record }">
            <template v-if="column.key === 'decision'">
              <a-tag :color="decisionLabel(record) === '通过' ? 'green' : 'red'">
                {{ decisionLabel(record) }}
              </a-tag>
            </template>
            <template v-else-if="column.dataIndex === 'decided_by'">
              {{ reviewerLabel[record.decided_by] || record.decided_by || '—' }}
            </template>
            <template v-else-if="column.key === 'time'">
              {{ fmtTime(record.decided_at || record.updated_at) }}
            </template>
            <template v-else-if="column.key === 'action'">
              <a-button size="small" type="link" @click="logDetail = record">详情</a-button>
            </template>
          </template>
        </a-table>
      </a-tab-pane>
    </a-tabs>

    <!-- 日志详情弹窗 -->
    <a-modal
      :open="!!logDetail"
      title="审计日志详情"
      :footer="null"
      width="720px"
      @cancel="logDetail = null"
    >
      <template v-if="logDetail">
        <a-descriptions bordered size="small" :column="2">
          <a-descriptions-item label="ID">
            <span style="font-family: monospace; font-size: 12px">{{ logDetail.id }}</span>
          </a-descriptions-item>
          <a-descriptions-item label="工具">{{ logDetail.tool_name || '—' }}</a-descriptions-item>
          <a-descriptions-item label="会话">
            <span style="font-family: monospace; font-size: 12px">
              {{ logDetail.session_id || logDetail.conversation_id || '—' }}
            </span>
          </a-descriptions-item>
          <a-descriptions-item label="决策">{{ decisionLabel(logDetail) }}</a-descriptions-item>
          <a-descriptions-item label="审批方">
            {{ reviewerLabel[logDetail.decided_by] || logDetail.decided_by || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="时间">
            {{ fmtTime(logDetail.decided_at || logDetail.updated_at) }}
          </a-descriptions-item>
          <a-descriptions-item v-if="logDetail.comment" label="备注" :span="2">
            {{ logDetail.comment }}
          </a-descriptions-item>
        </a-descriptions>
        <div style="margin-top: 12px; font-size: 13px; font-weight: 600">载荷</div>
        <pre
          style="
            background: var(--ant-color-bg-layout);
            padding: 8px;
            border-radius: 4px;
            font-size: 12px;
            max-height: 240px;
            overflow: auto;
            white-space: pre-wrap;
            word-break: break-all;
          "
        >{{ typeof logDetail.original_arguments === 'string' ? logDetail.original_arguments : JSON.stringify(logDetail.original_arguments, null, 2) }}</pre>
      </template>
    </a-modal>
  </div>
</template>
