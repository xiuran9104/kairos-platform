<script lang="ts" setup>
import type { EchartsUIType } from '@vben/plugins/echarts';
import { nextTick, onMounted, ref } from 'vue';
import { EchartsUI, useEcharts } from '@vben/plugins/echarts';
import { getRmosMemories, submitRmosFeedback } from '#/api/pent';
import { requestClient } from '#/api/request';

const memories = ref<any[]>([]); const loading = ref(false); const activeTab = ref('graph');
const feedbackLoading = ref<string|null>(null); const graphData = ref<any>({nodes:[],edges:[]});
const graphChart = ref<EchartsUIType>();
const pathChart = ref<EchartsUIType>();
const { renderEcharts } = useEcharts(graphChart);

async function fetchAll() { loading.value=true; try{memories.value=(await getRmosMemories()) as any[]||[]}catch{/* */}finally{loading.value=false} }

async function buildGraph() {
  try { graphData.value = await requestClient.get('/pent/rmos/attack-graph') || {nodes:[],edges:[]} } catch { /* */ }
  if (!graphData.value.nodes?.length) return;
  await nextTick();
  renderEcharts({
    backgroundColor: 'transparent',
    tooltip: { formatter: (p:any) => p.dataType==='node'?`<b>${p.name}</b><br/>${p.data.category===3?'目标':p.data.category===0?'严重':p.data.category===1?'高危':'中低危'}`:`${p.data.source} → ${p.data.target}` },
    series: [{
      type: 'graph', layout: 'force', roam: true, draggable: true,
      label: { show: true, fontSize: 11, color: '#333', formatter: (p:any) => p.name.length>16?p.name.slice(0,14)+'..':p.name },
      force: { repulsion: 600, gravity: 0.1, edgeLength: [80, 180], layoutAnimation: true },
      categories: [
        { name: '严重', itemStyle: { color: '#e84749' } },
        { name: '高危', itemStyle: { color: '#f0a040' } },
        { name: '中低危', itemStyle: { color: '#5b8def' } },
        { name: '目标', itemStyle: { color: '#3cb371' } },
      ],
      data: graphData.value.nodes,
      edges: (graphData.value.edges||[]).map((e:any) => ({
        ...e, lineStyle: { color: '#bbb', curveness: 0.2, width: 0.6, opacity: 0.7 },
      })),
    }],
  });
}

async function submitFeedback(memoryId:string, feedback:'useful'|'useless') { feedbackLoading.value=memoryId; try{await submitRmosFeedback(memoryId,{feedback});fetchAll()}catch{/* */}finally{feedbackLoading.value=null} }

onMounted(async () => { await fetchAll(); await buildGraph(); });
</script>

<template>
  <div style="padding:24px;min-height:100vh">
    <a-page-header title="RMOS 攻击记忆" sub-title="攻击图谱 · 攻击路径 · 记忆召回">
      <template #extra><a-button @click="fetchAll" :loading="loading">刷新</a-button></template>
    </a-page-header>

    <a-tabs v-model:activeKey="activeTab" @change="async (k:string) => { if(k==='graph') { await buildGraph(); } }">
      <a-tab-pane key="graph" tab="攻击图谱">
        <a-empty v-if="!graphData.nodes?.length" description="暂无图谱数据 · 执行扫描后自动生成" />
        <EchartsUI v-else ref="graphChart" height="65vh" />
      </a-tab-pane>

      <a-tab-pane key="paths" tab="攻击路径">
        <a-empty v-if="!graphData.nodes?.length" description="暂无攻击路径" />
        <div v-else v-for="(n,i) in graphData.nodes.filter((n:any)=>n.category===3)" :key="n.id" style="margin-bottom:24px">
          <!-- Target header -->
          <div style="display:flex;align-items:center;gap:8px;margin-bottom:12px;padding:6px 0">
            <div style="width:40px;height:40px;border-radius:50%;background:#52c41a20;display:flex;align-items:center;justify-content:center;font-size:20px">🎯</div>
            <div><span style="font-weight:600;font-size:15px;color:#333">{{ n.name }}</span><span style="color:#8c8c8c;font-size:12px;margin-left:8px">攻击链</span></div>
          </div>

          <!-- Horizontal chain: each finding as a linked node -->
          <div style="display:flex;align-items:flex-start;gap:0;overflow-x:auto;padding:8px 0">
            <template v-for="(f,fi) in graphData.nodes.filter((fn:any)=>fn.category!==3&&graphData.edges.some((e:any)=>e.source===n.id&&e.target===fn.id))" :key="f.id">
              <!-- Arrow -->
              <div v-if="fi>0" style="display:flex;align-items:center;padding:0 4px;flex-shrink:0">
                <div style="width:24px;height:2px;background:#d9d9d9;position:relative">
                  <div style="position:absolute;right:-4px;top:-4px;width:0;height:0;border-left:8px solid #d9d9d9;border-top:5px solid transparent;border-bottom:5px solid transparent"></div>
                </div>
              </div>

              <!-- Finding node -->
              <div :style="{
                flexShrink:0,width:'180px',
                padding:'16px',borderRadius:'12px',
                background:f.category===0?'linear-gradient(135deg,#fff2f0,#ffccc7)':f.category===1?'linear-gradient(135deg,#fff7e6,#ffd591)':'linear-gradient(135deg,#e6f4ff,#91caff)',
                border:`2px solid ${f.category===0?'#ff4d4f':f.category===1?'#fa8c16':'#1677ff'}`,
                boxShadow:'0 2px 8px rgba(0,0,0,0.08)',
                position:'relative',
              }">
                <!-- Stage badge -->
                <div :style="{
                  position:'absolute',top:'-12px',left:'8px',
                  padding:'2px 10px',borderRadius:'10px',
                  fontSize:'10px',fontWeight:600,color:'#fff',
                  background:f.category===0?'#ff4d4f':f.category===1?'#fa8c16':'#1677ff',
                }">
                  {{ f.category===0 ? '严重' : f.category===1 ? '高危' : '中低危' }}
                </div>
                <div style="font-size:13px;font-weight:600;color:#333;margin-bottom:4px;margin-top:4px;word-break:break-all">{{ f.name }}</div>
                <div style="font-size:11px;color:#666;line-height:1.4">
                  <!-- Determine stage based on finding name -->
                  <span v-if="f.name.includes('SSH')">🔓 初始访问</span>
                  <span v-else-if="f.name.includes('弱口令')||f.name.includes('注入')">⚡ 凭据获取</span>
                  <span v-else-if="f.name.includes('CVE')||f.name.includes('漏洞')">📈 权限提升</span>
                  <span v-else-if="f.name.includes('未授权')">↔️ 横向移动</span>
                  <span v-else>📤 信息收集</span>
                </div>
                <!-- Impact indicator -->
                <div style="margin-top:8px;display:flex;gap:4px">
                  <div :style="{height:4,borderRadius:2,flex:1,background:f.category===0?'#ff4d4f':'#d9d9d9'}"></div>
                  <div :style="{height:4,borderRadius:2,flex:1,background:f.category<=1?'#fa8c16':'#d9d9d9'}"></div>
                  <div :style="{height:4,borderRadius:2,flex:1,background:'#d9d9d9'}"></div>
                </div>
              </div>
            </template>
          </div>
        </div>
      </a-tab-pane>

      <a-tab-pane key="memories" tab="记忆库">
        <a-row :gutter="[16,16]" style="margin-bottom:16px">
          <a-col :xs="12" :md="6" v-for="c in [
            {label:'记忆条目',val:memories.length,color:'#1677ff',icon:'🧠'},
            {label:'策略文档',val:memories.filter((m:any)=>m.doc_type==='guide').length,color:'#722ed1',icon:'📋'},
            {label:'代码片段',val:memories.filter((m:any)=>m.doc_type==='code').length,color:'#52c41a',icon:'💻'},
            {label:'运行时记录',val:memories.filter((m:any)=>m.doc_type==='answer').length,color:'#fa8c16',icon:'⚡'},
          ]" :key="c.label">
            <div :style="{padding:'20px',borderRadius:'10px',border:`1px solid ${c.color}20`,background:`${c.color}08`,textAlign:'center'}">
              <div style="font-size:28px;margin-bottom:4px">{{ c.icon }}</div>
              <div :style="{fontSize:'36px',fontWeight:700,color:c.color}">{{ c.val }}</div>
              <div style="fontSize:'13px',color:'#8c8c8c',marginTop:'4px'">{{ c.label }}</div>
            </div>
          </a-col>
        </a-row>
        <div v-if="!memories.length" style="text-align:center;padding:60px 0;color:#8c8c8c">
          <div style="font-size:48px;margin-bottom:12px">🧠</div>
          <div>暂无记忆 · Worker 执行后自动积累</div>
        </div>
        <div v-else style="display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));gap:12px">
          <div v-for="m in memories" :key="m.id"
            :style="{padding:'16px',borderRadius:'10px',border:'1px solid #f0f0f0',background:'#fff',cursor:'pointer',transition:'box-shadow 0.2s'}"
            @mouseenter="($el:any) => $el.style.boxShadow='0 4px 12px rgba(0,0,0,0.08)'"
            @mouseleave="($el:any) => $el.style.boxShadow='none'">
            <div style="display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:8px">
              <div style="display:flex;align-items:center;gap:8px">
                <a-tag :color="m.doc_type==='guide'?'purple':m.doc_type==='code'?'green':'orange'" size="small">{{ m.doc_type==='guide'?'策略':m.doc_type==='code'?'代码':'运行时' }}</a-tag>
                <span style="font-weight:600;font-size:14px;color:#333">{{ m.title?.slice(0,40) || m.id?.slice(0,16) }}</span>
              </div>
              <div style="display:flex;gap:4px">
                <a-button size="small" type="text" @click="submitFeedback(m.id,'useful')" :loading="feedbackLoading===m.id">👍</a-button>
                <a-button size="small" type="text" @click="submitFeedback(m.id,'useless')" :loading="feedbackLoading===m.id">👎</a-button>
              </div>
            </div>
            <div style="color:#8c8c8c;font-size:12px;margin-bottom:8px;line-height:1.5;max-height:60px;overflow:hidden">
              {{ m.content?.slice(0,150) || m.description || '(空)' }}
            </div>
            <div style="display:flex;flex-wrap:wrap;gap:4px;margin-bottom:4px">
              <a-tag v-for="t in (Array.isArray(m.tags)?m.tags:[]).slice(0,5)" :key="t" size="small" style="font-size:10px">{{ t }}</a-tag>
            </div>
            <div style="font-size:10px;color:#bbb">{{ m.created_at?new Date(m.created_at).toLocaleString():'-' }}</div>
          </div>
        </div>
      </a-tab-pane>
    </a-tabs>
  </div>
</template>
