<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { requestClient } from '#/api/request';
defineOptions({ name: 'PentReportDetail' });
const route = useRoute();
const reportId = computed(() => route.params.id as string);
const report = ref<any>(null);
const loading = ref(false);
async function load() {
  loading.value = true;
  try { report.value = (await requestClient.get(`/pent/reports/${reportId.value}`))?.data; }
  finally { loading.value = false; }
}
onMounted(load);
</script>
<template>
  <div class="p-4 max-w-4xl mx-auto">
    <a-page-header title="报告详情" :sub-title="reportId" />
    <a-spin :spinning="loading">
      <a-card v-if="report" :bordered="false">
        <a-descriptions :column="2" size="small" bordered>
          <a-descriptions-item label="Artifact ID">{{ report.artifact_id }}</a-descriptions-item>
          <a-descriptions-item label="Run ID">{{ report.run_id }}</a-descriptions-item>
          <a-descriptions-item label="大小">{{ (report.size_bytes / 1024).toFixed(1) }} KB</a-descriptions-item>
          <a-descriptions-item label="格式">{{ report.format }}</a-descriptions-item>
          <a-descriptions-item label="对象 Key" :span="2">{{ report.object_key }}</a-descriptions-item>
        </a-descriptions>
        <a-divider />
        <div style="white-space: pre-wrap; font-family: monospace; font-size: 13px; max-height: 70vh; overflow-y: auto;">
          {{ report.preview || '报告已生成，点击 Artifact 下载完整内容' }}
        </div>
      </a-card>
      <a-empty v-else-if="!loading" description="未找到报告" />
    </a-spin>
  </div>
</template>
