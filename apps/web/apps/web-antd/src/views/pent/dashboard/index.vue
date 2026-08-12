<script lang="ts" setup>
import { useRouter } from 'vue-router';
import { useDashboard } from './composables/useDashboard';

const router = useRouter();
const { loading, error, sessions, runs, toolOverview, activity, runningCount, queuedCount, failedCount, completedCount, fetchAll } = useDashboard();
</script>

<template>
  <div style="padding:24px;background:var(--ant-color-bg-layout);min-height:100%">
    <a-alert v-if="error" type="error" :message="error" show-icon style="margin-bottom:16px" closable />

    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:20px">
      <div style="display:flex;align-items:center;gap:12px">
        <span style="font-size:32px">&#x1f6e1;</span>
        <div><h2 style="margin:0">Pent AI 安全平台</h2><span style="color:var(--ant-color-text-tertiary);font-size:13px">自动化渗透测试 · {{runningCount()?'运行中':'待命中'}}</span></div>
      </div>
      <div style="display:flex;align-items:center;gap:16px">
        <div style="text-align:center;padding:0 12px" v-for="b in [{label:'运行中',val:runningCount(),color:'var(--ant-color-success)'},{label:'排队',val:queuedCount(),color:'var(--ant-color-primary)'},{label:'已完成',val:completedCount(),color:'var(--ant-color-success)'},{label:'失败',val:failedCount(),color:'var(--ant-color-error)'}]" :key="b.label">
          <div :style="{fontSize:'22px',fontWeight:700,color:b.color}">{{b.val}}</div><div style="font-size:11px;color:var(--ant-color-text-tertiary)">{{b.label}}</div>
        </div>
        <a-button :loading="loading" @click="fetchAll" size="small">刷新</a-button>
      </div>
    </div>

    <a-row :gutter="[16,16]" style="margin-bottom:16px">
      <a-col :span="16">
        <a-card title="运行状态" size="small">
          <div v-for="s in [{label:'排队中',val:queuedCount(),color:'var(--ant-color-text-tertiary)'},{label:'运行中',val:runningCount(),color:'var(--ant-color-primary)'},{label:'已完成',val:completedCount(),color:'var(--ant-color-success)'},{label:'失败',val:failedCount(),color:'var(--ant-color-error)'}]" :key="s.label" style="display:flex;align-items:center;gap:12px;margin-bottom:8px">
            <span style="font-size:12px;color:#666;width:60px">{{s.label}} {{s.val}}</span>
            <div style="flex:1;height:6px;background:var(--ant-color-fill-secondary);border-radius:3px"><div :style="{width:Math.max(runs.length,1)?(s.val/Math.max(runs.length,1)*100)+'%':'0%',height:'100%',background:s.color,borderRadius:'3px'}"></div></div>
          </div>
        </a-card>
        <a-card title="最近活动" size="small" style="margin-top:16px">
          <a-empty v-if="!activity.length" description="等待事件..." :image="false" />
          <a-timeline v-else><a-timeline-item v-for="(item,idx) in activity.slice(0,6)" :key="idx" :color="item.color"><div style="font-size:13px">{{item.text}}</div><div style="color:var(--ant-color-text-tertiary);font-size:11px;margin-top:2px">{{item.time?new Date(item.time).toLocaleTimeString():''}}</div></a-timeline-item></a-timeline>
        </a-card>
      </a-col>
      <a-col :span="8">
        <a-card title="常用工具" size="small">
          <a-empty v-if="!toolOverview?.top_tools?.length" description="暂无数据" :image="false" />
          <div v-else v-for="(t,i) in toolOverview.top_tools" :key="t.name" style="display:flex;align-items:center;padding:6px 0;border-bottom:1px solid var(--ant-color-fill-quaternary)">
	            <span :style="{width:22,height:22,borderRadius:'50%',background:Number(i)===0?'var(--ant-color-warning)':Number(i)===1?'var(--ant-color-text-tertiary)':'var(--ant-color-fill-secondary)',color:'var(--ant-color-bg-container)',display:'flex',alignItems:'center',justifyContent:'center',fontSize:'11px',fontWeight:600}">{{Number(i)+1}}</span>
            <span style="flex:1;margin-left:10px;font-size:13px">{{t.name}}</span>
            <span style="font-weight:600;color:var(--ant-color-primary)">{{t.count}}</span>
          </div>
        </a-card>
        <a-card title="最近会话" size="small" style="margin-top:16px">
          <a-empty v-if="!sessions.length" description="暂无会话" :image="false" />
          <div v-else v-for="s in sessions.slice(0,5)" :key="s.id" style="display:flex;align-items:center;justify-content:space-between;padding:6px 8px;border-radius:4px;cursor:pointer;margin-bottom:4px" @click="router.push('/pent/tasks/'+s.id)">
            <span style="font-size:13px;font-family:monospace;color:#555">{{s.id?.slice(0,12)}}</span>
            <a-tag :color="s.status==='active'?'processing':'default'" size="small">{{s.status}}</a-tag>
          </div>
        </a-card>
        <a-card title="平台状态" size="small" style="margin-top:16px">
          <div v-for="h in [{s:'processing',l:'Worker',v:'Active'},{s:'success',l:'Tools',v:(toolOverview?.total_tools||63)+' ready'},{s:'success',l:'PostgreSQL',v:'UP'},{s:'success',l:'Redis',v:'UP'}]" :key="h.l" style="display:flex;justify-content:space-between;align-items:center;padding:4px 0">
            <a-badge :status="h.s as any" :text="h.l" /><span style="color:var(--ant-color-success);font-weight:600;font-size:12px">{{h.v}}</span>
          </div>
        </a-card>
      </a-col>
    </a-row>
  </div>
</template>
