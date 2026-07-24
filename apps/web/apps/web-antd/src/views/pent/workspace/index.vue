<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getPentWorkspaceFiles, getPentWorkspaceUsage } from '#/api/pent';

const files = ref<any[]>([]);
const usage = ref<any>({});
const loading = ref(false);

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let size = bytes;
  while (size >= 1024 && i < units.length - 1) { size /= 1024; i++; }
  return `${size.toFixed(i > 0 ? 2 : 0)} ${units[i]}`;
}

async function fetchAll() {
  loading.value = true;
  try {
    const [f, u] = await Promise.all([
      getPentWorkspaceFiles().catch(() => []),
      getPentWorkspaceUsage().catch(() => ({})),
    ]);
    files.value = (f as any[]) || [];
    usage.value = u || {};
  } catch {
    message.error('加载工作区失败');
  } finally {
    loading.value = false;
  }
}

onMounted(fetchAll);
</script>

<template>
  <div class="p-4">
    <a-page-header title="工作区文件" sub-title="扫描、证据和报告产物统一管理">
      <template #extra>
        <a-button @click="fetchAll" :loading="loading">刷新</a-button>
      </template>
    </a-page-header>

    <!-- Usage stats -->
    <a-row :gutter="[16, 16]" class="mb-4">
      <a-col :xs="12" :md="6">
        <a-card size="small"><a-statistic title="项目文件" :value="usage.file_count ?? files.length" /></a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small"><a-statistic title="存储占用" :value="formatBytes(usage.total_bytes ?? 0)" /></a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small"><a-statistic title="报告文件" :value="usage.report_count ?? 0" /></a-card>
      </a-col>
      <a-col :xs="12" :md="6">
        <a-card size="small"><a-statistic title="配额使用" :value="usage.quota_percent != null ? `${usage.quota_percent}%` : '-'" /></a-card>
      </a-col>
    </a-row>

    <!-- File list -->
    <a-card title="文件列表" :bordered="false" size="small">
      <a-empty v-if="!files.length" description="暂无文件">
        <a-alert type="info" show-icon message="工作区 API 就绪后，此处将展示项目文件树，支持预览、下载和配额管理。" :banner="false" />
      </a-empty>
      <a-table
        v-else
        :columns="[
          { title: '文件名', dataIndex: 'filename', key: 'name', width: 300 },
          { title: '类型', dataIndex: 'content_type', key: 'type', width: 150 },
          { title: '大小', dataIndex: 'size_bytes', key: 'size', width: 100 },
          { title: '项目', dataIndex: 'project_id', key: 'project', width: 100, ellipsis: true },
          { title: 'Run', dataIndex: 'run_id', key: 'run', width: 100, ellipsis: true },
          { title: '创建时间', dataIndex: 'created_at', key: 'time', width: 170 },
        ]"
        :data-source="files"
        :pagination="false"
        row-key="id"
        size="small"
        :scroll="{ x: 920 }"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'size'">{{ formatBytes(record.size_bytes) }}</template>
        </template>
      </a-table>
    </a-card>
  </div>
</template>
