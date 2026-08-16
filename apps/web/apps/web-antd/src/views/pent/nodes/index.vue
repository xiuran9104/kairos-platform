<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { requestClient } from '#/api/request';
defineOptions({ name: 'PentNodes' });
const nodes = ref<any[]>([]);
const loading = ref(false);
async function load() {
  loading.value = true;
  try { nodes.value = (await requestClient.get('/pent/nodes'))?.data ?? []; }
  finally { loading.value = false; }
}
onMounted(load);
const statusColor = (s: string) => s === 'active' ? 'green' : s === 'offline' ? 'red' : 'orange';
const statusLabel = (s: string) => s === 'active' ? '在线' : s === 'offline' ? '离线' : s;
const isolationLabels: Record<string, string> = {
  mount_namespace: 'Mount NS',
  pid_namespace: 'PID NS',
  user_namespace: 'User NS',
  capabilities_dropped: 'Capabilities Drop',
  seccomp: 'Seccomp-BPF',
};
</script>
<template>
  <div class="p-4">
    <a-page-header title="扫描节点池" sub-title="分布式 Worker 节点管理与监控" />
    <a-card :bordered="false">
      <a-table :columns="[
        { title: 'Worker ID', dataIndex: 'worker_id', key: 'worker_id' },
        { title: '状态', dataIndex: 'status', key: 'status' },
        { title: '容量', dataIndex: 'capacity', key: 'capacity' },
        { title: '沙箱', dataIndex: 'sandbox_ready', key: 'sandbox_ready' },
        { title: '隔离能力', dataIndex: 'sandbox_detail', key: 'sandbox_detail' },
        { title: '最后心跳', dataIndex: 'last_heartbeat', key: 'last_heartbeat' },
        { title: '注册时间', dataIndex: 'registered_at', key: 'registered_at' },
      ]" :data-source="nodes" :loading="loading" row-key="worker_id" size="small">
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'status'">
            <a-tag :color="statusColor(record.status)">{{ statusLabel(record.status) }}</a-tag>
          </template>
          <template v-else-if="column.key === 'sandbox_ready'">
            <a-tag :color="record.sandbox_ready ? 'green' : 'red'">
              {{ record.sandbox_backend || 'bubblewrap' }} · {{ record.sandbox_ready ? '就绪' : '不可用' }}
            </a-tag>
          </template>
          <template v-else-if="column.key === 'sandbox_detail'">
            <a-space wrap>
              <a-tag
                v-for="(label, key) in isolationLabels"
                :key="key"
                :color="record.sandbox_detail?.[key] ? 'blue' : 'default'"
              >{{ label }}</a-tag>
              <a-tag color="orange">共享宿主网络</a-tag>
            </a-space>
          </template>
          <template v-else-if="column.key === 'last_heartbeat' || column.key === 'registered_at'">
            {{ record[column.key] ? new Date(record[column.key]).toLocaleString() : '-' }}
          </template>
        </template>
      </a-table>
    </a-card>
  </div>
</template>
