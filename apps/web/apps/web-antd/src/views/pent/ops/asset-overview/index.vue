<script lang="ts" setup>
import type { EchartsUIType } from '@vben/plugins/echarts';
import type { PentAssetStats } from '#/api/pent/ops-assets';

import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';

import { EchartsUI, useEcharts } from '@vben/plugins/echarts';
import { getPentAssetStats } from '#/api/pent/ops-assets';

const router = useRouter();

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const overviewDays = ref<7 | 30 | 90>(30);
const stats = ref<PentAssetStats | null>(null);

const trendChart = ref<EchartsUIType>();
const { renderEcharts } = useEcharts(trendChart);

const protocols = computed(() => stats.value?.protocols || []);
const protocolTotal = computed(() =>
  protocols.value.reduce((sum, p) => sum + (p.count || 0), 0),
);
const protocolMax = computed(() =>
  Math.max(1, ...protocols.value.slice(0, 5).map((p) => p.count || 0)),
);

const recentPercent = computed(() => {
  const s = stats.value;
  if (!s || !s.total) return 0;
  return Math.min(100, Math.round(((s.recent || 0) / s.total) * 100));
});

const coverageRate = computed(() => {
  const s = stats.value;
  if (!s || !s.total) return 0;
  return Math.min(100, Math.round(((s.coverage?.scanned || 0) / s.total) * 100));
});

const coverageRecentRate = computed(() => {
  const s = stats.value;
  if (!s || !s.total) return 0;
  return Math.min(
    100,
    Math.round(((s.coverage?.scanned_30d || 0) / s.total) * 100),
  );
});

const trendSummary = computed(() => {
  const trend = stats.value?.asset_trend || [];
  return trend.reduce((sum, t) => sum + (t.added || 0), 0);
});

async function fetchStats() {
  loading.value = true;
  try {
    stats.value = await getPentAssetStats(overviewDays.value);
    await nextTick();
    renderTrend();
  } catch {
    message.error('加载资产概览失败');
  } finally {
    loading.value = false;
  }
}

function renderTrend() {
  const trend = stats.value?.asset_trend || [];
  if (!trend.length) return;
  renderEcharts({
    backgroundColor: 'transparent',
    grid: { left: 40, right: 16, top: 32, bottom: 28 },
    legend: { top: 0, right: 0, data: ['新增资产'] },
    tooltip: { trigger: 'axis' },
    xAxis: {
      type: 'category',
      data: trend.map((t) => String(t.date || '').slice(5)),
      axisLabel: { fontSize: 11 },
    },
    yAxis: { type: 'value', minInterval: 1 },
    series: [
      {
        name: '新增资产',
        type: 'line',
        smooth: true,
        areaStyle: { opacity: 0.13 },
        itemStyle: { color: '#3b82f6' },
        data: trend.map((t) => t.added),
      },
    ],
  });
}

function goAssetLibrary() {
  router.push('/pent/ops/asset-library');
}

watch(overviewDays, fetchStats);
onMounted(fetchStats);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="资产概览" sub-title="持续监控资产的暴露面与风险态势">
      <template #extra>
        <a-button :loading="loading" @click="fetchStats">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="goAssetLibrary">查看资产库</a-button>
      </template>
    </a-page-header>

    <a-spin :spinning="loading && !stats">
      <!-- 第一行：资产总数 / 协议分布 / 近期发现 -->
      <a-row :gutter="16" style="margin-top: 16px">
        <a-col :xs="24" :lg="8">
          <a-card size="small" style="height: 100%">
            <div style="font-weight: 600">资产总数</div>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 2px">
              持续监控中的资产
            </div>
            <div style="font-size: 40px; font-weight: 600; line-height: 1.4">
              {{ (stats?.total ?? 0).toLocaleString() }}
            </div>
            <a-row style="margin-top: 8px">
              <a-col :span="8">
                <a-statistic title="IP 地址" :value="stats?.ips ?? 0" />
              </a-col>
              <a-col :span="8">
                <a-statistic title="域名" :value="stats?.domains ?? 0" />
              </a-col>
              <a-col :span="8">
                <a-statistic title="端口" :value="stats?.ports ?? 0" />
              </a-col>
            </a-row>
          </a-card>
        </a-col>
        <a-col :xs="24" :lg="10">
          <a-card size="small" style="height: 100%">
            <template #title>协议分布</template>
            <template #extra>
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ protocols.length }} 种协议
              </span>
            </template>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 12px">
              按识别服务统计暴露构成
            </div>
            <template v-if="protocols.length">
              <div
                style="
                  display: flex;
                  font-size: 12px;
                  color: var(--ant-color-text-tertiary);
                  padding-bottom: 6px;
                  border-bottom: 1px solid var(--ant-color-border-secondary);
                  margin-bottom: 10px;
                "
              >
                <span style="width: 96px">协议</span>
                <span style="flex: 1">资产占比</span>
                <span style="width: 64px; text-align: right">数量</span>
                <span style="width: 56px; text-align: right">占比</span>
              </div>
              <div
                v-for="p in protocols.slice(0, 5)"
                :key="p.protocol"
                style="display: flex; align-items: center; gap: 8px; margin-bottom: 10px"
              >
                <span
                  style="
                    width: 88px;
                    font-size: 12px;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  "
                >
                  {{ p.protocol || 'unknown' }}
                </span>
                <div
                  style="
                    flex: 1;
                    height: 12px;
                    background: var(--ant-color-fill-quaternary);
                    border-radius: 4px;
                    overflow: hidden;
                  "
                >
                  <div
                    :style="{
                      width: `${Math.max(3, Math.round(((p.count || 0) / protocolMax) * 100))}%`,
                      height: '100%',
                      background: 'var(--ant-color-primary)',
                      borderRadius: '4px',
                    }"
                  />
                </div>
                <span style="width: 56px; text-align: right; font-size: 12px; font-weight: 600">
                  {{ (p.count || 0).toLocaleString() }}
                </span>
                <span style="width: 56px; text-align: right; font-size: 12px; color: var(--ant-color-text-tertiary)">
                  {{ (stats?.total || protocolTotal)
                    ? `${Math.round(((p.count || 0) / (stats?.total || protocolTotal)) * 100)}%`
                    : '—' }}
                </span>
              </div>
            </template>
            <a-empty v-else description="暂无数据" />
          </a-card>
        </a-col>
        <a-col :xs="24" :lg="6">
          <a-card size="small" style="height: 100%">
            <div style="display: flex; justify-content: flex-end">
              <a-tag>{{ overviewDays }} 天</a-tag>
            </div>
            <div style="font-size: 13px; color: var(--ant-color-text-secondary)">
              近 {{ overviewDays }} 天发现
            </div>
            <div style="font-size: 40px; font-weight: 600; line-height: 1.4">
              {{ (stats?.recent ?? 0).toLocaleString() }}
            </div>
            <a-progress :percent="recentPercent" :show-info="false" size="small" />
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 6px">
              占当前资产总量的 {{ recentPercent }}%
            </div>
          </a-card>
        </a-col>
      </a-row>

      <!-- 态势变化 -->
      <div style="display: flex; align-items: flex-end; justify-content: space-between; margin-top: 24px">
        <div>
          <div style="font-size: 16px; font-weight: 600">态势变化</div>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 2px">
            跟踪所选周期内的资产与风险变化
          </div>
        </div>
        <a-radio-group v-model:value="overviewDays" size="small" option-type="button" button-style="solid">
          <a-radio-button :value="7">7 天</a-radio-button>
          <a-radio-button :value="30">30 天</a-radio-button>
          <a-radio-button :value="90">90 天</a-radio-button>
        </a-radio-group>
      </div>

      <a-row :gutter="16" style="margin-top: 12px">
        <a-col :xs="24" :lg="12">
          <a-card size="small" title="资产变化趋势">
            <template #extra>
              <span style="font-size: 12px; margin-right: 16px">
                新增资产
                <strong style="color: #3b82f6">{{ trendSummary.toLocaleString() }}</strong>
              </span>
              <span style="font-size: 12px">
                停用资产
                <strong style="color: #8b5cf6">—</strong>
              </span>
            </template>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 8px">
              新发现与当前停用的资产
            </div>
            <template v-if="stats?.asset_trend?.length">
              <EchartsUI ref="trendChart" height="260px" />
            </template>
            <a-empty v-else description="暂无数据" />
          </a-card>
        </a-col>
        <a-col :xs="24" :lg="12">
          <a-card size="small" title="风险发现趋势">
            <template #extra>
              <span style="font-size: 12px; margin-right: 16px">
                新增漏洞
                <strong style="color: #f59e0b">—</strong>
              </span>
              <span style="font-size: 12px">
                高危及严重
                <strong style="color: #ef4444">—</strong>
              </span>
            </template>
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 8px">
              新发现的漏洞与高危及严重风险
            </div>
            <a-empty description="暂无数据" style="padding: 64px 0" />
          </a-card>
        </a-col>
      </a-row>

      <!-- 扫描覆盖 -->
      <a-card size="small" title="扫描覆盖" style="margin-top: 16px">
        <div style="display: flex; align-items: center; gap: 32px; flex-wrap: wrap">
          <div style="text-align: center">
            <a-progress type="circle" :percent="coverageRate" :width="120" />
            <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 8px">
              {{ stats?.total
                ? `${(stats?.coverage?.scanned ?? 0).toLocaleString()} / ${(stats?.total ?? 0).toLocaleString()} 已覆盖`
                : '—' }}
            </div>
          </div>
          <a-row :gutter="32" style="flex: 1; min-width: 280px">
            <a-col :xs="12" :md="6">
              <a-statistic title="已扫描" :value="stats?.coverage?.scanned ?? 0" />
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="近 30 天覆盖" :value="stats?.coverage?.scanned_30d ?? 0" />
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                占全部资产 {{ coverageRecentRate }}%
              </div>
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="从未扫描" :value="stats?.coverage?.never_scanned ?? 0" />
            </a-col>
            <a-col :xs="12" :md="6">
              <a-statistic title="超 30 天未扫描" :value="stats?.coverage?.overdue_30d ?? 0" />
            </a-col>
          </a-row>
        </div>
      </a-card>
    </a-spin>
  </div>
</template>
