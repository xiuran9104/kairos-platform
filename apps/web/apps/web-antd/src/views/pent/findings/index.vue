<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { getPentRuns, getPentRunFindings } from '#/api/pent';
import { requestClient } from '#/api/request';

const findings = ref<any[]>([]); const loading = ref(false); const sevFilter = ref('all');
const sevColor: Record<string,string> = { critical:'red', high:'volcano', medium:'orange', low:'gold', info:'blue' };
const evidenceCache = ref<Record<string,any[]>>({}); const loadingEvidence = ref<string|null>(null); const expandedId = ref<string|null>(null);

const filtered = computed(() => sevFilter.value==='all' ? findings.value : findings.value.filter((f:any)=>f.severity===sevFilter.value));
const counts = computed(() => ({
  total:findings.value.length,
  critical:findings.value.filter((f:any)=>f.severity==='critical').length,
  high:findings.value.filter((f:any)=>f.severity==='high').length,
  open:findings.value.filter((f:any)=>f.status==='open').length,
}));

async function fetch() {
  loading.value=true; evidenceCache.value={};
  try {
    const runs=(await getPentRuns()) as any[]; const all:any[]=[];
    for(const r of runs){ try{ const d=(await getPentRunFindings(r.id)) as any[]; d.forEach((f:any)=>{f._run_id=r.id;all.push(f)}); }catch{/* */} }
    findings.value=all;
  }catch{/* */} finally{loading.value=false;}
}

async function loadEvidence(f:any) {
  if(evidenceCache.value[f.id]) return;
  loadingEvidence.value=f.id;
  try {
    const ev = (await requestClient.get(`/pent/runs/${f._run_id}/evidence`)) as any[]||[];
    const linked = ev.filter((e:any) => f.evidence_ids?.some((eid:any) => {
      const id = typeof eid === 'string' ? eid : eid?.id;
      return id && (e.id === id || e.id?.includes(id?.slice(0,12)));
    }));
    evidenceCache.value[f.id] = linked.length ? linked : ev.slice(0,3);
  } catch { evidenceCache.value[f.id]=[]; }
  finally { loadingEvidence.value=null; }
}

onMounted(fetch);
</script>
<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="漏洞发现" sub-title="扫描结果 · 点击展开查看证据">
      <template #extra><a-button @click="fetch" :loading="loading">刷新</a-button></template>
    </a-page-header>

    <a-row :gutter="[16,16]" style="margin-bottom:16px">
      <a-col :span="6" v-for="c in [
        {label:'总计',val:counts.total,color:'#1677ff'},{label:'严重',val:counts.critical,color:'#f5222d'},
        {label:'高危',val:counts.high,color:'#fa8c16'},{label:'未处理',val:counts.open,color:'#ff4d4f'}
      ]" :key="c.label">
        <a-card size="small"><a-statistic :title="c.label" :value="c.val" :value-style="{color:c.color,fontSize:'28px'}"/></a-card>
      </a-col>
    </a-row>

    <a-radio-group v-model:value="sevFilter" button-style="solid" size="small" style="margin-bottom:16px">
      <a-radio-button value="all">全部</a-radio-button>
      <a-radio-button value="critical">严重</a-radio-button>
      <a-radio-button value="high">高危</a-radio-button>
      <a-radio-button value="medium">中危</a-radio-button>
      <a-radio-button value="low">低危</a-radio-button>
      <a-radio-button value="info">信息</a-radio-button>
    </a-radio-group>

    <a-spin :spinning="loading">
      <a-empty v-if="!filtered.length" description="暂无漏洞"/>

      <div v-else v-for="f in filtered" :key="f.id" style="margin-bottom:12px">
        <a-card size="small" :style="{borderLeft: `3px solid ${sevColor[f.severity]==='red'?'#f5222d':sevColor[f.severity]==='volcano'?'#fa8c16':'#1677ff'}`}">
          <div style="display:flex;justify-content:space-between;align-items:flex-start">
            <div style="flex:1">
              <span style="font-weight:600;font-size:14px">{{f.title||'未命名漏洞'}}</span>
              <span style="color:#8c8c8c;font-size:12px;margin-left:8px">Run: {{f._run_id?.slice(0,16)}}</span>
            </div>
            <div style="display:flex;gap:6px;flex-shrink:0;align-items:center">
              <a-tag :color="sevColor[f.severity]||'default'" size="small">{{f.severity}}</a-tag>
              <a-tag size="small">{{f.status}}</a-tag>
              <a-button v-if="f.evidence_ids?.length" size="small" type="link"
                @click="async () => { if(expandedId===f.id){expandedId=null}else{expandedId=f.id;await loadEvidence(f)} }">
                {{ expandedId===f.id ? '收起证据 ▲' : `查看证据 (${Array.isArray(f.evidence_ids)?f.evidence_ids.length:0}) ▼` }}
              </a-button>
            </div>
          </div>
          <div style="color:#8c8c8c;font-size:12px;margin-top:4px">
            目标: {{f.target||f._target||'-'}}
          </div>
        </a-card>

        <!-- Inline evidence -- only visible when expanded -->
        <a-spin v-if="expandedId===f.id" :spinning="loadingEvidence===f.id" size="small">
          <div v-if="evidenceCache[f.id]?.length" style="margin-left:20px;margin-top:4px;border-left:2px solid #1677ff;padding-left:16px">
            <div v-for="(ev,idx) in evidenceCache[f.id]" :key="ev.id||idx" style="padding:8px 0;border-bottom:1px dashed #f0f0f0">
              <div style="display:flex;gap:6px;margin-bottom:4px">
                <a-tag v-if="ev.content_hash" size="small" color="blue" style="font-family:monospace">{{ev.content_hash?.slice(0,20)}}</a-tag>
                <a-tag size="small">{{ev.content_type||'text/plain'}}</a-tag>
              </div>
              <pre style="background:#f5f5f5;padding:8px;border-radius:4px;font-size:11px;max-height:120px;overflow:auto;white-space:pre-wrap;word-break:break-all;margin:0">{{ev.source||ev.content||'(空)'}}</pre>
            </div>
          </div>
          <div v-else-if="!loadingEvidence||loadingEvidence!==f.id" style="margin-left:20px;padding:8px 16px;color:#8c8c8c;font-size:12px">暂无关联证据</div>
        </a-spin>
      </div>
    </a-spin>
  </div>
</template>
