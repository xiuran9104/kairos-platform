<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentModelRoleBindings, updatePentModelRoleBindings, getPentRuntimePolicy, updatePentRuntimePolicy } from '#/api/pent';

const modelBindings = ref<any[]>([]);
const runtimePolicy = ref<any>({});
const loading = ref(false);
const activeTab = ref('models');

async function fetchAll() {
  loading.value = true;
  try {
    const [bindings, policy] = await Promise.all([
      getPentModelRoleBindings().catch(() => []),
      getPentRuntimePolicy().catch(() => ({})),
    ]);
    modelBindings.value = (bindings as any[]) || [];
    runtimePolicy.value = policy || {};
  } catch {
    message.error('加载设置失败');
  } finally {
    loading.value = false;
  }
}

async function saveBindings() {
  try {
    await updatePentModelRoleBindings({ bindings: modelBindings.value });
    message.success('已保存');
  } catch {
    message.error('保存失败');
  }
}

async function savePolicy() {
  try {
    await updatePentRuntimePolicy(runtimePolicy.value);
    message.success('已保存');
  } catch {
    message.error('保存失败');
  }
}

const roleLabels: Record<string, string> = {
  planner: '规划', reviewer: '审查', executor: '执行',
  reflector: '反思', summarizer: '摘要', conversation: '对话',
};

onMounted(fetchAll);
</script>

<template>
  <div class="p-4">
    <a-page-header title="平台设置" sub-title="模型、角色、预算和 Agent 运行策略统一配置">
      <template #extra>
        <a-button @click="fetchAll" :loading="loading">刷新</a-button>
      </template>
    </a-page-header>

    <a-tabs v-model:activeKey="activeTab">
      <!-- 模型角色绑定 -->
      <a-tab-pane key="models" tab="模型角色绑定">
        <a-card title="模型角色分配" size="small">
          <template #extra>
            <a-button type="primary" size="small" :disabled="!modelBindings.length" @click="saveBindings">
              保存绑定
            </a-button>
          </template>
          <a-empty v-if="!modelBindings.length" description="暂无角色绑定">
            <a-alert type="info" show-icon message="模型角色绑定 API 就绪后，此处可配置规划、审查、对话等角色的模型分配。" :banner="false" />
          </a-empty>
          <template v-else>
            <a-table
              :columns="[
                { title: '角色', dataIndex: 'model_role', key: 'role', width: 150 },
                { title: '模型', dataIndex: 'model_id', key: 'model', width: 250 },
                { title: '优先级', dataIndex: 'priority', key: 'pri', width: 80 },
                { title: '权重', dataIndex: 'weight', key: 'weight', width: 80 },
                { title: '状态', dataIndex: 'is_active', key: 'active', width: 80 },
              ]"
              :data-source="modelBindings"
              :pagination="false"
              row-key="id"
              size="small"
            >
              <template #bodyCell="{ column, record }">
                <template v-if="column.key === 'role'">{{ roleLabels[record.model_role] || record.model_role }}</template>
                <template v-else-if="column.key === 'active'">
                  <a-tag :color="record.is_active ? 'green' : 'default'">{{ record.is_active ? '启用' : '禁用' }}</a-tag>
                </template>
              </template>
            </a-table>
          </template>
        </a-card>
      </a-tab-pane>

      <!-- 运行策略 -->
      <a-tab-pane key="policy" tab="运行策略">
        <a-card title="Agent 运行策略" size="small">
          <template #extra>
            <a-button type="primary" size="small" :disabled="!Object.keys(runtimePolicy).length" @click="savePolicy">
              保存策略
            </a-button>
          </template>
          <template v-if="Object.keys(runtimePolicy).length">
            <a-form :model="runtimePolicy" layout="vertical">
              <a-row :gutter="16">
                <a-col :xs="24" :md="12">
                  <a-form-item label="最大并发 Run">
                    <a-input-number v-model:value="runtimePolicy.max_concurrent_runs" :min="1" :max="100" class="w-full" />
                  </a-form-item>
                </a-col>
                <a-col :xs="24" :md="12">
                  <a-form-item label="Worker 租约（秒）">
                    <a-input-number v-model:value="runtimePolicy.lease_ttl_seconds" :min="30" :max="3600" class="w-full" />
                  </a-form-item>
                </a-col>
                <a-col :xs="24" :md="12">
                  <a-form-item label="工具超时（秒）">
                    <a-input-number v-model:value="runtimePolicy.max_tool_timeout_seconds" :min="10" :max="7200" class="w-full" />
                  </a-form-item>
                </a-col>
                <a-col :xs="24" :md="12">
                  <a-form-item label="最大重试次数">
                    <a-input-number v-model:value="runtimePolicy.max_retries_per_run" :min="0" :max="20" class="w-full" />
                  </a-form-item>
                </a-col>
                <a-col :xs="24" :md="12">
                  <a-form-item label="重试基础延迟（秒）">
                    <a-input-number v-model:value="runtimePolicy.retry_base_delay_seconds" :min="1" :max="3600" class="w-full" />
                  </a-form-item>
                </a-col>
                <a-col :xs="24" :md="12">
                  <a-form-item label="审批有效期（秒）">
                    <a-input-number v-model:value="runtimePolicy.approval_ttl_seconds" :min="60" :max="86400" class="w-full" />
                  </a-form-item>
                </a-col>
              </a-row>
              <a-form-item label="低风险工具自动放行">
                <a-switch v-model:checked="runtimePolicy.auto_approve_low_risk" />
              </a-form-item>
              <a-form-item label="需要审批的风险等级">
                <a-checkbox-group
                  v-model:value="runtimePolicy.require_approval_for"
                  :options="[
                    { label: '低', value: 'low' },
                    { label: '中', value: 'medium' },
                    { label: '高', value: 'high' },
                    { label: '严重', value: 'critical' },
                  ]"
                />
              </a-form-item>
            </a-form>
          </template>
          <a-empty v-else description="暂无运行策略">
            <a-alert type="info" show-icon message="运行策略 API 就绪后，此处可配置预算、并发、最大轮次、停止条件和重规划阈值。" :banner="false" />
          </a-empty>
        </a-card>
      </a-tab-pane>

      <!-- 成本治理 -->
      <a-tab-pane key="cost" tab="成本治理">
        <a-card title="Token 用量与成本" size="small">
          <a-row :gutter="[16, 16]">
            <a-col :xs="12" :md="6">
              <a-statistic title="本月 Token" :value="0" suffix="K" />
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="预估费用" :value="0" prefix="$" precision="2" />
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="活跃配额" :value="0" />
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="缓存命中率" :value="'0%'" />
            </a-col>
          </a-row>
        </a-card>
      </a-tab-pane>
    </a-tabs>
  </div>
</template>
