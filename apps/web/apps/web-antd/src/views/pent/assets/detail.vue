<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import { getPentAssetProfile, exportPentAssetReport } from '#/api/pent';

const route = useRoute();
const router = useRouter();
const assetId = computed(() => String(route.params.id ?? ''));
const profile = ref<any>(null);
const loading = ref(false);
const activeTab = ref('overview');
const exporting = ref(false);

const tabs = [
  { key: 'overview', label: '概览' },
  { key: 'reports', label: '报告' },
  { key: 'findings', label: '漏洞' },
  { key: 'endpoints', label: '端点' },
  { key: 'fingerprints', label: '指纹' },
  { key: 'discoveries', label: '原始发现' },
  { key: 'timeline', label: '时间线' },
  { key: 'attack-graph', label: '攻击图谱' },
  { key: 'history-paths', label: '历史路径' },
];

const riskColor = computed(() => {
  const score = profile.value?.risk_score ?? 0;
  if (score >= 7) return '#ff4d4f';
  if (score >= 4) return '#fa8c16';
  return '#52c41a';
});

async function fetch() {
  if (!assetId.value) return;
  loading.value = true;
  try {
    profile.value = await getPentAssetProfile(assetId.value);
  } catch {
    message.error('加载资产档案失败');
  } finally {
    loading.value = false;
  }
}

async function handleExport(format: 'html' | 'docx') {
  exporting.value = true;
  try {
    await exportPentAssetReport(assetId.value, format);
    message.success(`${format.toUpperCase()} 报告导出中，稍后可下载`);
  } catch {
    message.error('导出失败');
  } finally {
    exporting.value = false;
  }
}

onMounted(fetch);
</script>

<template>
  <div class="p-4">
    <a-page-header
      :title="profile?.asset_name || `资产档案 ${assetId}`"
      sub-title="单资产全维度安全档案"
      @back="router.push('/pent/assets')"
    >
      <template #extra>
        <a-space>
          <a-button :loading="exporting" @click="handleExport('html')">导出 HTML</a-button>
          <a-button type="primary" :loading="exporting" @click="handleExport('docx')">导出 DOCX</a-button>
        </a-space>
      </template>
    </a-page-header>

    <a-spin :spinning="loading">
      <a-tabs v-model:activeKey="activeTab">
        <!-- Tab 1: 概览 -->
        <a-tab-pane key="overview" tab="概览">
          <a-row :gutter="[16, 16]">
            <a-col :xs="24" :md="8">
              <a-card title="风险评分" size="small">
                <a-statistic
                  :value="profile?.risk_score ?? '-'"
                  :value-style="{ color: riskColor, fontSize: '48px' }"
                  suffix="/ 10"
                />
                <div v-if="profile?.risk_breakdown" class="mt-2">
                  <div v-for="(val, key) in profile.risk_breakdown" :key="key" class="flex justify-between text-sm">
                    <span class="text-gray-500">{{ key }}</span>
                    <span>{{ val }}</span>
                  </div>
                </div>
              </a-card>
            </a-col>
            <a-col :xs="24" :md="8">
              <a-card title="基本信息" size="small">
                <a-descriptions :column="1" size="small" bordered>
                  <a-descriptions-item label="资产 ID">{{ profile?.asset_id ?? assetId }}</a-descriptions-item>
                  <a-descriptions-item label="攻击路径">{{ profile?.attack_paths ?? 0 }}</a-descriptions-item>
                  <a-descriptions-item label="端点数量">{{ profile?.endpoint_count ?? 0 }}</a-descriptions-item>
                  <a-descriptions-item label="漏洞数量">{{ profile?.finding_count ?? 0 }}</a-descriptions-item>
                </a-descriptions>
              </a-card>
            </a-col>
            <a-col :xs="24" :md="8">
              <a-card title="业务标签" size="small">
                <a-empty v-if="!profile?.business_tags?.length" :image="false" description="暂无标签" />
                <a-space v-else wrap>
                  <a-tag v-for="tag in profile.business_tags" :key="tag" color="purple">{{ tag }}</a-tag>
                </a-space>
              </a-card>
            </a-col>
          </a-row>
          <a-row :gutter="[16, 16]" class="mt-4">
            <a-col :xs="24" :md="12">
              <a-card title="技术栈" size="small">
                <h4 class="text-gray-500">语言</h4>
                <a-tag v-for="l in profile?.languages || []" :key="l" color="blue">{{ l }}</a-tag>
                <span v-if="!profile?.languages?.length" class="text-gray-400">暂无记录</span>
                <a-divider />
                <h4 class="text-gray-500">指纹</h4>
                <template v-if="profile?.fingerprints?.length">
                  <div v-for="fp in profile.fingerprints" :key="fp.name" class="text-sm mb-1">
                    <a-tag color="geekblue">{{ fp.category }}</a-tag>
                    {{ fp.name }}
                    <span v-if="fp.version" class="text-gray-400">v{{ fp.version }}</span>
                  </div>
                </template>
                <span v-else class="text-gray-400">暂无记录</span>
              </a-card>
            </a-col>
            <a-col :xs="24" :md="12">
              <a-card title="关联 URL" size="small">
                <a-empty v-if="!profile?.urls?.length" :image="false" />
                <a-list v-else :data-source="profile.urls" size="small">
                  <template #renderItem="{ item }">
                    <a-list-item><code>{{ item }}</code></a-list-item>
                  </template>
                </a-list>
              </a-card>
            </a-col>
          </a-row>
        </a-tab-pane>

        <!-- Tabs 2-9: Staged placeholders -->
        <a-tab-pane v-for="tab in tabs.filter(t => t.key !== 'overview')" :key="tab.key" :tab="tab.label">
          <a-alert type="info" show-icon :message="`${tab.label} 模块已规划`" :banner="false" class="mb-4" />
          <a-card size="small">
            <a-empty :description="`${tab.label}功能将在后端 API 就绪后实现`" />
          </a-card>
        </a-tab-pane>
      </a-tabs>
    </a-spin>
  </div>
</template>
