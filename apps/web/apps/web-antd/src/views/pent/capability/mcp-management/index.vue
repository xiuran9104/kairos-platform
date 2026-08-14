<script lang="ts" setup>
import type { PentMcpServer } from '#/api/pent';

import { computed, onActivated, onDeactivated, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  createPentMcpServer,
  deletePentMcpServer,
  disablePentTool,
  enablePentTool,
  getPentMcpServers,
  getPentTools,
  startPentMcpServer,
  stopPentMcpServer,
  updatePentMcpServer,
} from '#/api/pent';

// ── 右栏：MCP 工具配置 ─────────────────────────────────────────────────────
const tools = ref<any[]>([]);
const toolLoading = ref(false);
const toolSearch = ref('');
const toolStatusFilter = ref<'all' | 'disabled' | 'enabled'>('all');
const toolSaving = ref(false);
const toolPage = ref(1);
const TOOL_PAGE_SIZE = 20;

/** 启用勾选草稿：key = tool_id-version → 目标启用状态 */
const toolDraft = ref<Record<string, boolean>>({});

const isEnabled = (t: any) => t.lifecycle_status === 'active';
const toolKey = (t: any) => `${t.tool_id}-${t.version}`;

async function fetchTools() {
  toolLoading.value = true;
  try {
    tools.value = ((await getPentTools()) as any[]) || [];
    toolDraft.value = {};
  } catch {
    /* 静默 */
  } finally {
    toolLoading.value = false;
  }
}

/** 当前生效的勾选状态：草稿优先，其次服务端状态 */
function toolChecked(t: any) {
  const draft = toolDraft.value[toolKey(t)];
  return draft === undefined ? isEnabled(t) : draft;
}

const toolDirtyCount = computed(
  () =>
    tools.value.filter((t) => {
      const draft = toolDraft.value[toolKey(t)];
      return draft !== undefined && draft !== isEnabled(t);
    }).length,
);

const searchedTools = computed(() => {
  const q = toolSearch.value.trim().toLowerCase();
  if (!q) return tools.value;
  return tools.value.filter(
    (t) =>
      (t.name || '').toLowerCase().includes(q) ||
      (t.description || '').toLowerCase().includes(q) ||
      (t.namespace || '').toLowerCase().includes(q),
  );
});

const filteredTools = computed(() => {
  let list = searchedTools.value;
  if (toolStatusFilter.value === 'enabled') list = list.filter((t) => toolChecked(t));
  else if (toolStatusFilter.value === 'disabled') list = list.filter((t) => !toolChecked(t));
  return list;
});

const pagedTools = computed(() => {
  const start = (toolPage.value - 1) * TOOL_PAGE_SIZE;
  return filteredTools.value.slice(start, start + TOOL_PAGE_SIZE);
});

const pageEnabledCount = computed(
  () => pagedTools.value.filter((t) => toolChecked(t)).length,
);
const totalEnabledCount = computed(
  () => searchedTools.value.filter((t) => toolChecked(t)).length,
);

function onToolSearch() {
  toolPage.value = 1;
}

function setToolStatusFilter(f: 'all' | 'disabled' | 'enabled') {
  toolStatusFilter.value = f;
  toolPage.value = 1;
}

function setToolChecked(t: any, checked: boolean) {
  const key = toolKey(t);
  if (checked === isEnabled(t)) {
    const next = { ...toolDraft.value };
    delete next[key];
    toolDraft.value = next;
  } else {
    toolDraft.value = { ...toolDraft.value, [key]: checked };
  }
}

function selectAllTools() {
  const next = { ...toolDraft.value };
  filteredTools.value.forEach((t) => {
    if (!isEnabled(t)) next[toolKey(t)] = true;
  });
  toolDraft.value = next;
}

function deselectAllTools() {
  const next = { ...toolDraft.value };
  filteredTools.value.forEach((t) => {
    if (isEnabled(t)) next[toolKey(t)] = false;
  });
  toolDraft.value = next;
}

/** 保存工具配置：对比服务端状态，逐个启用/停用变更项 */
async function saveToolsConfig() {
  const changed = tools.value.filter((t) => {
    const draft = toolDraft.value[toolKey(t)];
    return draft !== undefined && draft !== isEnabled(t);
  });
  if (!changed.length) {
    message.info('没有需要保存的变更');
    return;
  }
  toolSaving.value = true;
  let failed = 0;
  for (const t of changed) {
    try {
      if (toolDraft.value[toolKey(t)]) {
        await enablePentTool(t.tool_id, t.version);
      } else {
        await disablePentTool(t.tool_id, t.version);
      }
    } catch {
      failed++;
    }
  }
  toolSaving.value = false;
  if (failed) {
    message.error(`${failed} 个工具保存失败，请重试`);
  } else {
    message.success(`已保存 ${changed.length} 项工具配置`);
  }
  await fetchTools();
}

// ── 左栏：外部 MCP 配置 ────────────────────────────────────────────────────
const servers = ref<PentMcpServer[]>([]);
const serverStats = ref({ total: 0, enabled: 0, disabled: 0, connected: 0 });
const serverLoading = ref(false);

async function fetchServers() {
  serverLoading.value = true;
  try {
    const res = await getPentMcpServers();
    servers.value = res.servers || [];
    serverStats.value = res.stats || { total: 0, enabled: 0, disabled: 0, connected: 0 };
  } catch {
    /* 静默 */
  } finally {
    serverLoading.value = false;
  }
}

const serverStatusColor: Record<string, string> = {
  connected: 'green',
  connecting: 'blue',
  error: 'red',
  disconnected: 'default',
  disabled: 'orange',
};
const serverStatusLabel: Record<string, string> = {
  connected: '已连接',
  connecting: '连接中...',
  error: '连接失败',
  disconnected: '未连接',
  disabled: '已禁用',
};

async function doStart(s: PentMcpServer) {
  try {
    await startPentMcpServer(s.id);
    message.success(`已启动 ${s.name}`);
    fetchServers();
  } catch {
    message.error('启动失败');
  }
}

async function doStop(s: PentMcpServer) {
  try {
    await stopPentMcpServer(s.id);
    message.success(`已停止 ${s.name}`);
    fetchServers();
  } catch {
    message.error('停止失败');
  }
}

async function doDeleteServer(s: PentMcpServer) {
  try {
    await deletePentMcpServer(s.id);
    message.success('已删除');
    fetchServers();
  } catch {
    message.error('删除失败');
  }
}

// ── server modal ──────────────────────────────────────────────────────────
const showModal = ref(false);
const editingId = ref<null | string>(null);
const saving = ref(false);
const form = ref({
  name: '',
  transport: 'stdio' as 'http' | 'sse' | 'stdio',
  command: '',
  argsText: '',
  envText: '',
  url: '',
  headersText: '',
  timeout_secs: 30,
  description: '',
});

function openCreate() {
  editingId.value = null;
  form.value = {
    name: '',
    transport: 'stdio',
    command: '',
    argsText: '',
    envText: '',
    url: '',
    headersText: '',
    timeout_secs: 30,
    description: '',
  };
  showModal.value = true;
}

function openEdit(s: PentMcpServer) {
  editingId.value = s.id;
  form.value = {
    name: s.name,
    transport: (s.transport as any) || 'stdio',
    command: s.command || '',
    argsText: (s.args || []).join(','),
    envText: s.env ? JSON.stringify(s.env, null, 2) : '',
    url: s.url || '',
    headersText: s.headers ? JSON.stringify(s.headers, null, 2) : '',
    timeout_secs: s.timeout_secs ?? 30,
    description: s.description || '',
  };
  showModal.value = true;
}

async function doSave() {
  if (!form.value.name) {
    message.warning('请填写名称');
    return;
  }
  const payload: Record<string, any> = {
    name: form.value.name,
    transport: form.value.transport,
    timeout_secs: form.value.timeout_secs,
    description: form.value.description,
  };
  if (form.value.transport === 'stdio') {
    payload.command = form.value.command;
    payload.args = form.value.argsText
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    if (form.value.envText.trim()) {
      try {
        payload.env = JSON.parse(form.value.envText);
      } catch {
        message.error('环境变量必须是合法 JSON');
        return;
      }
    }
  } else {
    payload.url = form.value.url;
    if (form.value.headersText.trim()) {
      try {
        payload.headers = JSON.parse(form.value.headersText);
      } catch {
        message.error('请求头必须是合法 JSON');
        return;
      }
    }
  }
  saving.value = true;
  try {
    if (editingId.value) {
      await updatePentMcpServer(editingId.value, payload);
      message.success('已更新');
    } else {
      await createPentMcpServer(payload);
      message.success('已创建');
    }
    showModal.value = false;
    fetchServers();
    fetchTools();
  } catch {
    message.error('保存失败');
  } finally {
    saving.value = false;
  }
}

// ── polling (server list 8s) ──────────────────────────────────────────────
let timer: ReturnType<typeof setInterval> | null = null;

function refreshAll() {
  fetchTools();
  fetchServers();
}

onActivated(() => {
  refreshAll();
  if (timer) clearInterval(timer);
  timer = setInterval(fetchServers, 8000);
});

onDeactivated(() => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页头：标题 + 刷新 -->
    <a-page-header title="MCP 管理">
      <template #extra>
        <a-button :loading="serverLoading || toolLoading" @click="refreshAll">刷新</a-button>
      </template>
    </a-page-header>

    <a-row :gutter="16" style="margin-top: 16px; align-items: stretch">
      <!-- 左栏：外部 MCP 配置 -->
      <a-col :xs="24" :xl="10">
        <a-card size="small">
          <template #title>外部 MCP 配置</template>
          <template #extra>
            <a-button type="primary" size="small" @click="openCreate">添加外部MCP</a-button>
          </template>

          <!-- 统计条：总数 / 已启用 / 已停用 / 已连接 -->
          <div style="display: flex; flex-wrap: wrap; gap: 16px; margin-bottom: 12px; font-size: 13px; color: var(--ant-color-text-secondary)">
            <span title="总数">📊 总数：<strong>{{ serverStats.total }}</strong></span>
            <span title="已启用">✅ 已启用：<strong>{{ serverStats.enabled }}</strong></span>
            <span title="已停用">⏸ 已停用：<strong>{{ serverStats.disabled }}</strong></span>
            <span title="已连接">🔗 已连接：<strong>{{ serverStats.connected }}</strong></span>
          </div>

          <a-spin :spinning="serverLoading">
            <div
              v-for="s in servers"
              :key="s.id"
              style="border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; padding: 12px 16px; margin-bottom: 12px"
            >
              <!-- 卡片头：名称 + 状态 + 操作 -->
              <div style="display: flex; justify-content: space-between; align-items: center; gap: 8px; flex-wrap: wrap">
                <div style="display: flex; align-items: center; gap: 8px; min-width: 0">
                  <span style="font-weight: 600">
                    {{ s.transport === 'stdio' ? '⚙️' : '🌐' }} {{ s.name }}
                  </span>
                  <a-tag v-if="(s.tool_count ?? 0) > 0" size="small" color="blue" :title="'工具数量'">
                    🔧 {{ s.tool_count }}
                  </a-tag>
                  <a-tag :color="serverStatusColor[s.status || 'disconnected']" size="small">
                    {{ serverStatusLabel[s.status || 'disconnected'] || s.status }}
                  </a-tag>
                </div>
                <a-space :size="4">
                  <a-button
                    v-if="s.status === 'connected'"
                    size="small"
                    title="停止连接"
                    @click="doStop(s)"
                  >
                    ⏸ 停止
                  </a-button>
                  <a-button
                    v-else-if="s.status === 'connecting'"
                    size="small"
                    disabled
                  >
                    ⏳ 连接中...
                  </a-button>
                  <a-button
                    v-else
                    size="small"
                    title="启动连接"
                    @click="doStart(s)"
                  >
                    ▶ 启动
                  </a-button>
                  <a-button
                    size="small"
                    title="编辑配置"
                    :disabled="s.status === 'connecting'"
                    @click="openEdit(s)"
                  >
                    ✏️ 编辑
                  </a-button>
                  <a-popconfirm title="确认删除该服务器？" @confirm="doDeleteServer(s)">
                    <a-button
                      size="small"
                      danger
                      title="删除配置"
                      :disabled="s.status === 'connecting'"
                    >
                      🗑 删除
                    </a-button>
                  </a-popconfirm>
                </a-space>
              </div>

              <!-- 错误块 -->
              <div
                v-if="(s.status === 'error' || s.status === 'disconnected') && s.error"
                :style="{
                  margin: '12px 0',
                  padding: '8px 12px',
                  borderRadius: '4px',
                  fontSize: '13px',
                  wordBreak: 'break-all',
                  background: s.status === 'error' ? '#fff1f0' : '#fffbe6',
                  borderLeft: `3px solid ${s.status === 'error' ? '#f5222d' : '#faad14'}`,
                  color: s.status === 'error' ? '#cf1322' : '#8a6d00',
                }"
              >
                <strong>{{ s.status === 'error' ? '❌' : '⚠️' }} 连接错误：</strong>{{ s.error }}
              </div>

              <!-- 详情网格 -->
              <a-row :gutter="12" style="margin-top: 12px; font-size: 13px">
                <a-col :span="12" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">传输模式</div>
                  <div>{{ s.transport === 'stdio' ? '⚙️' : '🌐' }} {{ (s.transport || 'stdio').toUpperCase() }}</div>
                </a-col>
                <a-col :span="12" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">工具数量</div>
                  <div>
                    <span v-if="(s.tool_count ?? 0) > 0" style="font-weight: 600; color: var(--ant-color-primary)">
                      🔧 {{ s.tool_count }} 个工具
                    </span>
                    <span v-else-if="s.status === 'connected'" style="color: var(--ant-color-text-tertiary)">
                      暂无工具
                    </span>
                    <span v-else>—</span>
                  </div>
                </a-col>
                <a-col v-if="s.description" :span="12" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">描述</div>
                  <div>{{ s.description }}</div>
                </a-col>
                <a-col v-if="s.timeout_secs" :span="12" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">超时时间</div>
                  <div>{{ s.timeout_secs }} 秒</div>
                </a-col>
                <a-col v-if="s.transport === 'stdio' && s.command" :span="24" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">命令</div>
                  <div style="font-family: monospace; font-size: 12px; word-break: break-all">
                    {{ s.command }} {{ (s.args || []).join(' ') }}
                  </div>
                </a-col>
                <a-col v-else-if="s.transport !== 'stdio' && s.url" :span="24" style="margin-bottom: 8px">
                  <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">URL</div>
                  <div style="font-family: monospace; font-size: 12px; word-break: break-all">{{ s.url }}</div>
                </a-col>
              </a-row>
            </div>
            <a-empty v-if="!servers.length && !serverLoading">
              <template #description>
                <div>📋 暂无外部MCP配置</div>
                <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
                  点击"添加外部MCP"按钮开始配置
                </div>
              </template>
            </a-empty>
          </a-spin>
        </a-card>
      </a-col>

      <!-- 右栏：MCP 工具配置 -->
      <a-col :xs="24" :xl="14">
        <a-card size="small">
          <template #title>MCP 工具配置</template>
          <template #extra>
            <a-button
              type="primary"
              size="small"
              :loading="toolSaving"
              @click="saveToolsConfig"
            >
              保存工具配置<template v-if="toolDirtyCount">（{{ toolDirtyCount }}）</template>
            </a-button>
          </template>

          <!-- 控制条：全选 / 全不选 / 状态筛选 / 搜索 -->
          <div style="display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-bottom: 12px">
            <a-button size="small" @click="selectAllTools">全选</a-button>
            <a-button size="small" @click="deselectAllTools">全不选</a-button>
            <a-radio-group
              :value="toolStatusFilter"
              size="small"
              @change="(e: any) => setToolStatusFilter(e.target.value)"
            >
              <a-radio-button value="all">全部</a-radio-button>
              <a-radio-button value="enabled">已启用</a-radio-button>
              <a-radio-button value="disabled">已停用</a-radio-button>
            </a-radio-group>
            <a-input-search
              v-model:value="toolSearch"
              placeholder="输入工具名称..."
              style="width: 220px"
              size="small"
              allow-clear
              @search="onToolSearch"
              @input="onToolSearch"
            />
          </div>

          <!-- 统计条：当前页已启用 / 总计已启用 -->
          <div style="display: flex; flex-wrap: wrap; gap: 16px; margin-bottom: 12px; font-size: 12px; color: var(--ant-color-text-secondary)">
            <span title="当前页已启用">✅ 当前页已启用：<strong>{{ pageEnabledCount }}</strong> / {{ pagedTools.length }}</span>
            <span title="总计已启用">📊 总计已启用：<strong>{{ totalEnabledCount }}</strong> / {{ searchedTools.length }}</span>
          </div>

          <!-- 工具列表 -->
          <a-spin :spinning="toolLoading">
            <div
              v-for="t in pagedTools"
              :key="toolKey(t)"
              style="display: flex; align-items: flex-start; gap: 10px; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; padding: 10px 14px; margin-bottom: 8px"
            >
              <a-checkbox
                :checked="toolChecked(t)"
                style="margin-top: 2px"
                @change="(e: any) => setToolChecked(t, e.target.checked)"
              />
              <div style="min-width: 0; flex: 1">
                <div style="display: flex; align-items: center; gap: 6px; flex-wrap: wrap">
                  <span style="font-weight: 600; font-size: 13px">{{ t.name }}</span>
                  <a-tag
                    :color="t.risk_level === 'high' ? 'red' : t.risk_level === 'medium' ? 'orange' : 'green'"
                    style="font-size: 11px; margin-inline-end: 0"
                  >{{ t.risk_level || 'low' }}</a-tag>
                </div>
                <div
                  style="font-size: 12px; color: var(--ant-color-text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap"
                  :title="t.description || '无描述'"
                >
                  {{ t.description || '无描述' }}
                </div>
                <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px">
                  <a-tag color="blue" style="font-size: 11px; margin-inline-end: 0">{{ t.namespace }}</a-tag>
                  <a-tag
                    v-for="cap in (t.capabilities || []).slice(0, 3)"
                    :key="cap"
                    style="font-size: 11px; margin-inline-end: 0"
                  >{{ cap }}</a-tag>
                </div>
              </div>
            </div>
            <a-empty v-if="!pagedTools.length && !toolLoading" description="暂无工具" />
            <div style="display: flex; justify-content: flex-end; margin-top: 8px">
              <a-pagination
                v-model:current="toolPage"
                :total="filteredTools.length"
                :page-size="TOOL_PAGE_SIZE"
                size="small"
                :show-size-changer="false"
                :show-total="(t: number) => `共 ${t} 条`"
              />
            </div>
          </a-spin>
        </a-card>
      </a-col>
    </a-row>

    <!-- 添加 / 编辑外部MCP 弹窗 -->
    <a-modal
      v-model:open="showModal"
      :title="editingId ? '编辑外部MCP' : '添加外部MCP'"
      width="600px"
      :confirm-loading="saving"
      @ok="doSave"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="名称" required>
          <a-input v-model:value="form.name" placeholder="如 filesystem-server" :disabled="!!editingId" />
        </a-form-item>
        <a-form-item label="传输方式" required>
          <a-select
            v-model:value="form.transport"
            :options="[
              { label: 'stdio', value: 'stdio' },
              { label: 'HTTP', value: 'http' },
              { label: 'SSE', value: 'sse' },
            ]"
          />
        </a-form-item>
        <template v-if="form.transport === 'stdio'">
          <a-form-item label="命令" required>
            <a-input v-model:value="form.command" placeholder="如 npx / uvx / node" />
          </a-form-item>
          <a-form-item label="参数（逗号分隔）">
            <a-input v-model:value="form.argsText" placeholder="如 -y,@modelcontextprotocol/server-filesystem,/data" />
          </a-form-item>
          <a-form-item label="环境变量（JSON）">
            <a-textarea v-model:value="form.envText" :rows="3" placeholder='{"KEY": "value"}' />
          </a-form-item>
        </template>
        <a-form-item v-else label="URL" required>
          <a-input v-model:value="form.url" placeholder="http://127.0.0.1:3000/mcp" />
        </a-form-item>
        <a-form-item v-if="form.transport !== 'stdio'" label="请求头（JSON）">
          <a-textarea v-model:value="form.headersText" :rows="3" placeholder='{"Authorization": "Bearer ..."}' />
        </a-form-item>
        <a-form-item label="超时时间（秒）">
          <a-input-number v-model:value="form.timeout_secs" :min="1" :max="3600" style="width: 100%" />
        </a-form-item>
        <a-form-item label="描述">
          <a-textarea v-model:value="form.description" :rows="2" placeholder="简要说明该服务器用途" />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
