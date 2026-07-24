<script lang="ts" setup>
import { ref, onMounted } from 'vue';
import { Page } from '@vben/common-ui';
import { message } from 'ant-design-vue';
import { getDashboardSummary } from '#/api/scan/dashboard';
import { getMachineRoomList } from '#/api/scan/machine-room';
import { getAssetList } from '#/api/scan/asset';

const loading = ref(false);
const summary = ref<any>({});
const rooms = ref<any[]>([]);
const assets = ref<any[]>([]);

async function fetchData() {
  loading.value = true;
  try {
    const [s, r, a] = await Promise.all([getDashboardSummary(), getMachineRoomList().catch(() => []), getAssetList().catch(() => [])]);
    summary.value = s; rooms.value = r as any[]; assets.value = a as any[];
  } catch { message.error('加载失败'); }
  finally { loading.value = false; }
}
onMounted(fetchData);

const statCards = [
  { title: '资产总数', value: () => assets.value.length || summary.value?.asset_count || 0, suffix: '个', icon: 'lucide:hard-drive', color: '#1677ff', bg: '#e6f4ff' },
  { title: '机房数量', value: () => rooms.value.length, suffix: '个', icon: 'lucide:warehouse', color: '#52c41a', bg: '#f6ffed' },
  { title: '扫描任务', value: () => summary.value?.task_count || 0, suffix: '个', icon: 'lucide:scan-search', color: '#fa8c16', bg: '#fff7e6' },
  { title: '运行中', value: () => summary.value?.running_task_count || 0, suffix: '个', icon: 'lucide:activity', color: '#eb2f96', bg: '#fff0f6' },
  { title: '风险总数', value: () => summary.value?.risk_count || 0, suffix: '个', icon: 'lucide:alert-triangle', color: '#f5222d', bg: '#fff1f0' },
  { title: '已扫描', value: () => summary.value?.scanned_asset_count || 0, suffix: '个', icon: 'lucide:check-circle', color: '#13c2c2', bg: '#e6fffb' },
];
</script>

<template>
  <Page auto-content-height>
    <div style="padding: 24px; background: #f5f5f5; min-height: 100%">
      <a-page-header title="资产运营中心" sub-title="资产管理 · 安全扫描 · 云平台运营">
        <template #extra><a-button type="primary" @click="fetchData" :loading="loading">刷新数据</a-button></template>
      </a-page-header>
      <a-spin :spinning="loading">
        <a-row :gutter="[16, 16]">
          <a-col :xs="12" :sm="8" :lg="4" v-for="card in statCards" :key="card.title">
            <a-card :hoverable="true" size="small" style="border-radius:8px">
              <div style="display:flex;align-items:center;gap:12px">
                <div :style="{width:44,height:44,borderRadius:10,background:card.bg,display:'flex',alignItems:'center',justifyContent:'center'}">
                  <Icon :icon="card.icon" :style="{fontSize:'22px',color:card.color}" />
                </div>
                <div>
                  <div style="color:#8c8c8c;font-size:12px">{{ card.title }}</div>
                  <span style="font-size:22px;font-weight:700;color:#262626;line-height:1.2">{{ card.value() }}</span>
                  <span style="color:#8c8c8c;font-size:11px;margin-left:2px">{{ card.suffix }}</span>
                </div>
              </div>
            </a-card>
          </a-col>
        </a-row>
        <a-row :gutter="16" style="margin-top:16px">
          <a-col :span="16">
            <a-card title="最近资产" size="small" style="border-radius:8px">
              <a-empty v-if="!assets.length" description="暂无资产数据" />
              <a-list v-else :data-source="assets.slice(0, 8)" size="small">
                <template #renderItem="{ item }">
                  <a-list-item>
                    <a-list-item-meta>
                      <template #title><Icon icon="lucide:server" style="margin-right:6px;color:#1677ff" />{{ item.name }}</template>
                      <template #description>{{ item.ip }} · {{ item.zone || '未知区域' }} · {{ item.os || '未知系统' }}</template>
                    </a-list-item-meta>
                    <template #extra><a-tag :color="(item.weight||50)>70?'red':(item.weight||50)>40?'orange':'green'">权重 {{ item.weight || 50 }}</a-tag></template>
                  </a-list-item>
                </template>
              </a-list>
            </a-card>
          </a-col>
          <a-col :span="8">
            <a-card title="机房状态" size="small" style="border-radius:8px">
              <a-empty v-if="!rooms.length" description="暂无机房" />
              <a-list v-else :data-source="rooms.slice(0, 8)" size="small">
                <template #renderItem="{ item }">
                  <a-list-item>
                    <div style="display:flex;align-items:center;gap:8px;width:100%">
                      <Icon icon="lucide:warehouse" style="color:#1677ff" />
                      <span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{{ item.room_name }}</span>
                      <a-tag :color="item.status==='active'?'green':'red'" size="small">{{ item.status==='active'?'运营中':'停用' }}</a-tag>
                    </div>
                  </a-list-item>
                </template>
              </a-list>
            </a-card>
          </a-col>
        </a-row>
      </a-spin>
    </div>
  </Page>
</template>
