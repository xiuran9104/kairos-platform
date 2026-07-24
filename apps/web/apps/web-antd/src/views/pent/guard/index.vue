<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentGuardRules, getPentInterceptions, updatePentGuardRule } from '#/api/pent';

const rules = ref<any[]>([]);
const interceptions = ref<any[]>([]);
const loading = ref(false);
const activeTab = ref('rules');

async function fetchAll() {
  loading.value = true;
  try {
    const [r, i] = await Promise.all([
      getPentGuardRules().catch(() => []),
      getPentInterceptions().catch(() => []),
    ]);
    rules.value = (r as any[]) || [];
    interceptions.value = (i as any[]) || [];
  } catch {
    message.error('加载拦截数据失败');
  } finally {
    loading.value = false;
  }
}

async function toggleRule(rule: any) {
  try {
    await updatePentGuardRule(rule.id, { enabled: !rule.enabled });
    message.success(`规则 ${rule.name || rule.id} 已${rule.enabled ? '禁用' : '启用'}`);
    fetchAll();
  } catch {
    message.error('更新失败');
  }
}

onMounted(fetchAll);
</script>

<template>
  <div class="p-4">
    <a-page-header title="拦截中心" sub-title="双引擎护栏：静态规则 + LLM 语义审计">
      <template #extra>
        <a-button @click="fetchAll" :loading="loading">刷新</a-button>
      </template>
    </a-page-header>

    <!-- Stats -->
    <a-row :gutter="[16, 16]" class="mb-4">
      <a-col :xs="12" :md="6">
        <a-card size="small">
          <a-statistic title="静态规则" :value="rules.filter((r: any) => r.rule_type === 'static').length" value-style="color: #1677ff" />
        </a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small">
          <a-statistic title="语义规则" :value="rules.filter((r: any) => r.rule_type === 'semantic').length" value-style="color: #722ed1" />
        </a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small">
          <a-statistic title="今日拦截" :value="interceptions.length" value-style="color: #ff4d4f" />
        </a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small">
          <a-statistic title="启用规则" :value="rules.filter((r: any) => r.enabled).length" value-style="color: #52c41a" />
        </a-card>
      </a-col>
    </a-row>

    <a-tabs v-model:activeKey="activeTab">
      <!-- 规则管理 -->
      <a-tab-pane key="rules" tab="护栏规则">
        <a-alert type="info" show-icon message="静态规则由确定性代码强制执行；语义规则由 LLM 审计提供建议，不可绕过硬拒绝规则。" :banner="false" class="mb-4" />
        <a-table
          v-if="rules.length"
          :columns="[
            { title: '规则名', dataIndex: 'name', key: 'name', width: 200 },
            { title: '类型', dataIndex: 'rule_type', key: 'type', width: 100 },
            { title: '优先级', dataIndex: 'priority', key: 'priority', width: 80 },
            { title: '状态', dataIndex: 'enabled', key: 'enabled', width: 80 },
            { title: '描述', dataIndex: 'description', key: 'desc', ellipsis: true },
            { title: '操作', key: 'action', width: 80 },
          ]"
          :data-source="rules"
          :pagination="false"
          row-key="id"
          size="small"
        >
          <template #bodyCell="{ column, record }">
            <template v-if="column.key === 'type'">
              <a-tag :color="record.rule_type === 'static' ? 'blue' : 'purple'">
                {{ record.rule_type === 'static' ? '静态规则' : '语义规则' }}
              </a-tag>
            </template>
            <template v-else-if="column.key === 'enabled'">
              <a-tag :color="record.enabled ? 'green' : 'default'">{{ record.enabled ? '启用' : '禁用' }}</a-tag>
            </template>
            <template v-else-if="column.key === 'action'">
              <a-button type="link" size="small" @click="toggleRule(record)">
                {{ record.enabled ? '禁用' : '启用' }}
              </a-button>
            </template>
          </template>
        </a-table>
        <a-empty v-else description="暂无规则" :image="false" class="py-8" />
      </a-tab-pane>

      <!-- 拦截记录 -->
      <a-tab-pane key="interceptions" tab="拦截记录">
        <a-table
          v-if="interceptions.length"
          :columns="[
            { title: '时间', dataIndex: 'created_at', key: 'time', width: 170 },
            { title: '工具', dataIndex: 'tool_name', key: 'tool', width: 150 },
            { title: '命中规则', dataIndex: 'rule_name', key: 'rule', width: 200 },
            { title: '决策', dataIndex: 'decision', key: 'decision', width: 80 },
            { title: 'Run', dataIndex: 'run_id', key: 'run', width: 120, ellipsis: true },
          ]"
          :data-source="interceptions"
          :pagination="{ pageSize: 15 }"
          row-key="id"
          size="small"
          :scroll="{ x: 720 }"
        >
          <template #bodyCell="{ column, record }">
            <template v-if="column.key === 'decision'">
              <a-tag :color="record.decision === 'blocked' ? 'red' : record.decision === 'allowed' ? 'green' : 'orange'">
                {{ record.decision === 'blocked' ? '已阻断' : record.decision === 'allowed' ? '已放行' : record.decision }}
              </a-tag>
            </template>
          </template>
        </a-table>
        <a-empty v-else description="暂无拦截记录" :image="false" class="py-8" />
      </a-tab-pane>
    </a-tabs>
  </div>
</template>
