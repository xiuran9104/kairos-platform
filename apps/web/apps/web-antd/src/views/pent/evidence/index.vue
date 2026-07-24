<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { getPentRuns, getPentRunEvidence } from '#/api/pent';
const runs = ref<any[]>([]); const evidence = ref<any[]>([]); const loading = ref(false); const selectedRun = ref<any>(null);
async function fetch() { loading.value=true; try{runs.value=(await getPentRuns()) as any[]}catch{/* */} finally{loading.value=false} }
async function load(run:any){selectedRun.value=run; try{evidence.value=(await getPentRunEvidence(run.id)) as any[]||[]}catch{evidence.value=[]} }
onMounted(fetch);
</script>
<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="证据管理" sub-title="审计追踪 · 内容哈希可验证"><template #extra><a-button @click="fetch">刷新</a-button></template></a-page-header>
    <a-row :gutter="16">
      <a-col :span="6">
        <a-card title="Runs" size="small" :body-style="{padding:'8px',maxHeight:'70vh',overflow:'auto'}">
          <div v-for="r in runs" :key="r.id" :style="{padding:'8px 10px',borderRadius:'6px',cursor:'pointer',marginBottom:'4px',display:'flex',justifyContent:'space-between',background:selectedRun?.id===r.id?'#e6f4ff':'transparent',borderLeft:selectedRun?.id===r.id?'3px solid #1677ff':'3px solid transparent'}" @click="load(r)">
            <span style="font-size:13px;font-family:monospace">{{r.id?.slice(0,16)}}...</span>
            <a-tag size="small">{{r.status}}</a-tag>
          </div>
          <a-empty v-if="!runs.length" description="暂无 Run" :image="false"/>
        </a-card>
      </a-col>
      <a-col :span="18">
        <a-card title="证据列表" size="small">
          <a-empty v-if="!selectedRun" description="选择 Run 查看证据"/>
          <a-empty v-else-if="!evidence.length" description="暂无证据"/>
          <div v-else v-for="(ev,idx) in evidence" :key="ev.id||idx" :style="{padding:'16px',borderBottom:'1px solid #f0f0f0'}">
            <div :style="{display:'flex',justifyContent:'space-between',marginBottom:'8px'}">
              <span style="font-weight:600">{{ev.id?.slice(0,20)||'证据 #'+(idx+1)}}</span>
              <div style="display:flex;gap:6px">
                <a-tag v-if="ev.content_hash" size="small" color="blue">{{ev.content_hash?.slice(0,24)}}</a-tag>
                <a-tag size="small">{{ev.content_type||'text/plain'}}</a-tag>
              </div>
            </div>
            <pre :style="{background:'#f5f5f5',padding:'12px',borderRadius:'6px',fontSize:'12px',maxHeight:'200px',overflow:'auto',whiteSpace:'pre-wrap',wordBreak:'break-all',margin:0}">{{ev.source||ev.content||'(空)'}}</pre>
          </div>
        </a-card>
      </a-col>
    </a-row>
  </div>
</template>
