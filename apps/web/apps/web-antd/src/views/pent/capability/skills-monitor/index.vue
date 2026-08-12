<script lang="ts" setup>
import type { PentSkillStatItem, PentSkillStats } from '#/api/pent';

import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import { clearPentSkillStats, getPentSkillStats } from '#/api/pent';

function fmtTime(t?: string) {
  return t && t !== '-' ? t.slice(0, 19).replace('T', ' ') : '—';
}

const loading = ref(false);
const data = ref<PentSkillStats>({
  total_skills: 0,
  total_calls: 0,
  total_success: 0,
  total_failed: 0,
  stats: [],
});

async function fetchStats() {
  loading.value = true;
  try {
    data.value = await getPentSkillStats();
  } catch {
    message.error('加载技能监控数据失败');
  } finally {
    loading.value = false;
  }
}

// 与原版一致：保留一位小数
const successRate = computed(() =>
  data.value.total_calls
    ? ((data.value.total_success / data.value.total_calls) * 100).toFixed(1)
    : '0.0',
);

// 按调用次数降序，次数相同按名称排序（与原版一致）
const sortedStats = computed(() =>
  [...(data.value.stats || [])].sort((a, b) => {
    if (b.total_calls !== a.total_calls) return b.total_calls - a.total_calls;
    return (a.skill_name || '').localeCompare(b.skill_name || '');
  }),
);

const columns = [
  { title: '技能名称', dataIndex: 'skill_name', key: 'skill_name' },
  { title: '总调用', dataIndex: 'total_calls', key: 'total_calls', width: 110 },
  { title: '成功', dataIndex: 'success_calls', key: 'success_calls', width: 100 },
  { title: '失败', dataIndex: 'failed_calls', key: 'failed_calls', width: 100 },
  { title: '成功率', key: 'rate', width: 220 },
  { title: '最后调用时间', dataIndex: 'last_call_time', key: 'last_call_time', width: 180 },
];

function rateOf(item: PentSkillStatItem) {
  return item.total_calls
    ? ((item.success_calls / item.total_calls) * 100).toFixed(1)
    : '0.0';
}

async function refreshMonitor() {
  await fetchStats();
  message.success('已刷新');
}

async function doClear() {
  try {
    await clearPentSkillStats();
    message.success('已清空所有技能统计数据');
    fetchStats();
  } catch {
    message.error('清空统计数据失败');
  }
}

onMounted(fetchStats);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="技能状态">
      <template #extra>
        <a-button :loading="loading" @click="refreshMonitor">刷新</a-button>
      </template>
    </a-page-header>

    <!-- 调用统计 -->
    <div style="margin-top: 16px; margin-bottom: 12px; font-size: 15px; font-weight: 600">
      调用统计
    </div>
    <a-row :gutter="16">
      <a-col :flex="1">
        <a-card size="small">
          <a-statistic title="总技能数" :value="data.total_skills" />
        </a-card>
      </a-col>
      <a-col :flex="1">
        <a-card size="small">
          <a-statistic title="总调用次数" :value="data.total_calls" />
        </a-card>
      </a-col>
      <a-col :flex="1">
        <a-card size="small">
          <a-statistic
            title="成功调用"
            :value="data.total_success"
            :value-style="{ color: '#52c41a' }"
          />
        </a-card>
      </a-col>
      <a-col :flex="1">
        <a-card size="small">
          <a-statistic
            title="失败调用"
            :value="data.total_failed"
            :value-style="{ color: '#f5222d' }"
          />
        </a-card>
      </a-col>
      <a-col :flex="1">
        <a-card size="small">
          <a-statistic title="成功率" :value="successRate" suffix="%" />
        </a-card>
      </a-col>
    </a-row>

    <!-- 技能调用统计表格 -->
    <a-card size="small" style="margin-top: 16px">
      <template #title>技能调用统计</template>
      <template #extra>
        <a-popconfirm
          title="确定要清空所有技能统计数据吗？此操作不可恢复。"
          @confirm="doClear"
        >
          <a-button size="small">清空统计</a-button>
        </a-popconfirm>
      </template>
      <a-table
        :columns="columns"
        :data-source="sortedStats"
        :loading="loading"
        row-key="skill_name"
        size="middle"
        :pagination="{ pageSize: 20, showSizeChanger: false }"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'skill_name'">
            <strong>{{ record.skill_name }}</strong>
          </template>
          <template v-else-if="column.key === 'success_calls'">
            <span style="color: #52c41a">{{ record.success_calls }}</span>
          </template>
          <template v-else-if="column.key === 'failed_calls'">
            <span style="color: #f5222d">{{ record.failed_calls }}</span>
          </template>
          <template v-else-if="column.key === 'rate'">
            <span style="margin-right: 8px">{{ rateOf(record) }}%</span>
            <a-progress
              :percent="Number(rateOf(record))"
              :show-info="false"
              size="small"
              style="width: 120px; display: inline-block; vertical-align: middle"
            />
          </template>
          <template v-else-if="column.key === 'last_call_time'">
            {{ fmtTime(record.last_call_time) }}
          </template>
        </template>
        <template #emptyText>暂无技能调用记录</template>
      </a-table>
    </a-card>
  </div>
</template>
