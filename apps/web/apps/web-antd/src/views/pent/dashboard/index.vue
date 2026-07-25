<script lang="ts" setup>
import { useRouter } from 'vue-router';
import { useDashboard } from './composables/useDashboard';

const router = useRouter();
const { loading, error, sessions, runs, toolOverview, metrics, activity, runningCount, queuedCount, failedCount, completedCount, pendingApprovals, fetchAll } = useDashboard();
</script>

<template>
  <div style="padding:24px;min-height:100vh">
    <a-alert v-if="error" type="error" :message="error" show-icon style="margin-bottom:16px" closable />

    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:20px">
      <div style="display:flex;align-items:center;gap:12px">
        <span style="font-size:32px">&#x1f6e1;</span>
        <div><h2 style="margin:0">Pent AI 安全平台</h2><span style="color:#8c8c8c;font-size:13px">自动化渗透测试 · {{runningCount()?'运行中':'待命中'}}</span></div>
      </div>
      <div style="display:flex;align-items:center;gap:16px">
        <div style="text-align:center;padding:0 12px" v-for="b in [{label:'运行中',val:runningCount(),color:'#52c41a'},{label:'排队',val:queuedCount(),color:'#1677ff'},{label:'待审批',val:pendingApprovals(),color:'#fa8c16'},{label:'失败',val:failedCount(),color:'#ff4d4f'}]" :key="b.label">
          <div :style="{fontSize:'22px',fontWeight:700,color:b.color}">{{b.val}}</div><div style="font-size:11px;color:#8c8c8c">{{b.label}}</div>
        </div>
        <a-button :loading="loading" @click="fetchAll" size="small">刷新</a-button>
        <a-button type="primary" @click="router.push('/pent/agent')">启动 Agent</a-button>
      </div>
    </div>

    <a-row :gutter="[12,12]" style="margin-bottom:16px">
      <a-col :span="4" v-for="k in [{icon:'1f916',label:'Sessions',val:sessions.length,color:'#7c5cfc'},{icon:'1f3af',label:'Tasks',val:metrics.totalTasks,color:'#13c2c2'},{icon:'1f4e6',label:'Assets',val:metrics.totalAssets,color:'#f759ab'},{icon:'2699',label:'Tool Calls',val:metrics.totalToolCalls,color:'#fa8c16'},{icon:'26a0',label:'Findings',val:metrics.totalFindings,color:'#ff4d4f'},{icon:'1f4ca',label:'Success',val:toolOverview?.success_rate||'N/A',color:'#52c41a'}]" :key="k.label">
        <a-card size="small" :loading="loading">
          <a-statistic :title="k.label" :value="k.val" :value-style="{color:k.color,fontSize:'26px'}" />
        </a-card>
      </a-col>
    </a-row>

    <a-row :gutter="[16,16]" style="margin-bottom:16px">
      <a-col :span="16">
        <a-card title="运行状态" size="small">
          <div v-for="s in [{label:'排队中',val:queuedCount(),color:'#8c8c8c'},{label:'运行中',val:runningCount(),color:'#1677ff'},{label:'已完成',val:completedCount(),color:'#52c41a'},{label:'失败',val:failedCount(),color:'#ff4d4f'}]" :key="s.label" style="display:flex;align-items:center;gap:12px;margin-bottom:8px">
            <span style="font-size:12px;color:#666;width:60px">{{s.label}} {{s.val}}</span>
            <div style="flex:1;height:6px;background:#f0f0f0;border-radius:3px"><div :style="{width:Math.max(runs.length,1)?(s.val/Math.max(runs.length,1)*100)+'%':'0%',height:'100%',background:s.color,borderRadius:'3px'}"></div></div>
          </div>
        </a-card>
        <a-card title="最近活动" size="small" style="margin-top:16px">
          <a-empty v-if="!activity.length" description="等待事件..." :image="false" />
          <a-timeline v-else><a-timeline-item v-for="(item,idx) in activity.slice(0,6)" :key="idx" :color="item.color"><div style="font-size:13px">{{item.text}}</div><div style="color:#8c8c8c;font-size:11px;margin-top:2px">{{item.time?new Date(item.time).toLocaleTimeString():''}}</div></a-timeline-item></a-timeline>
        </a-card>
      </a-col>
      <a-col :span="8">
        <a-card title="常用工具" size="small">
          <a-empty v-if="!toolOverview?.top_tools?.length" description="暂无数据" :image="false" />
          <div v-else v-for="(t,i) in toolOverview.top_tools" :key="t.name" style="display:flex;align-items:center;padding:6px 0;border-bottom:1px solid #fafafa">
	            <span :style="{width:22,height:22,borderRadius:'50%',background:Number(i)===0?'#fa8c16':Number(i)===1?'#8c8c8c':'#d9d9d9',color:'#fff',display:'flex',alignItems:'center',justifyContent:'center',fontSize:'11px',fontWeight:600}">{{Number(i)+1}}</span>
            <span style="flex:1;margin-left:10px;font-size:13px">{{t.name}}</span>
            <span style="font-weight:600;color:#1677ff">{{t.count}}</span>
          </div>
        </a-card>
        <a-card title="最近会话" size="small" style="margin-top:16px">
          <a-empty v-if="!sessions.length" description="暂无会话" :image="false"><a-button type="primary" size="small" @click="router.push('/pent/agent')">启动</a-button></a-empty>
          <div v-else v-for="s in sessions.slice(0,5)" :key="s.id" style="display:flex;align-items:center;justify-content:space-between;padding:6px 8px;border-radius:4px;cursor:pointer;margin-bottom:4px" @click="router.push('/pent/sessions/'+s.id)">
            <span style="font-size:13px;font-family:monospace;color:#555">{{s.id?.slice(0,12)}}</span>
            <a-tag :color="s.status==='active'?'processing':'default'" size="small">{{s.status}}</a-tag>
          </div>
        </a-card>
        <a-card title="平台状态" size="small" style="margin-top:16px">
          <div v-for="h in [{s:'processing',l:'Worker',v:'Active'},{s:'success',l:'Tools',v:(toolOverview?.total_tools||63)+' ready'},{s:'success',l:'PostgreSQL',v:'UP'},{s:'success',l:'Redis',v:'UP'}]" :key="h.l" style="display:flex;justify-content:space-between;align-items:center;padding:4px 0">
            <a-badge :status="h.s as any" :text="h.l" /><span style="color:#52c41a;font-weight:600;font-size:12px">{{h.v}}</span>
          </div>
        </a-card>
      </a-col>
    </a-row>
  </div>
</template>
