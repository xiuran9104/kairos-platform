<script lang="ts" setup>
import type { PentAsset } from '#/api/pent/ops-assets';
import type { PentFact, PentFactGraph } from '#/api/pent/ops-facts';

import { computed, onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { getPentProjects } from '#/api/pent';
import { getPentAssets } from '#/api/pent/ops-assets';
import {
  getPentProjectFactGraph,
  getPentProjectFacts,
} from '#/api/pent/ops-facts';

// ── 项目选择 ───────────────────────────────────────────────────────────────
const loading = ref(false);
const projects = ref<any[]>([]);
const project = ref<string>();

const projectOptions = computed(() =>
  projects.value.map((p) => ({ label: p.name || p.id, value: p.id })),
);

async function loadProjects() {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
    if (!project.value && projects.value.length) {
      // 默认选最近更新的项目
      const sorted = [...projects.value].sort((a, b) =>
        String(b.updated_at || '').localeCompare(String(a.updated_at || '')),
      );
      project.value = sorted[0]?.id;
    }
  } catch {
    projects.value = [];
  }
}

// ── 数据加载 ───────────────────────────────────────────────────────────────
const graph = ref<PentFactGraph>({ nodes: [], edges: [] });
const facts = ref<PentFact[]>([]);
const assets = ref<PentAsset[]>([]);

const ASSET_SOURCES = ['xiaolanben', 'dns', 'websearch', 'fofa', 'icp', 'infogather'];

async function fetchAll() {
  if (!project.value) {
    graph.value = { nodes: [], edges: [] };
    facts.value = [];
    assets.value = [];
    return;
  }
  loading.value = true;
  try {
    const [g, f, a] = await Promise.all([
      getPentProjectFactGraph(project.value).catch(() => ({
        nodes: [],
        edges: [],
      })),
      getPentProjectFacts(project.value).catch(() => []),
      getPentAssets({ project_id: project.value, page_size: 100 }).catch(() => ({
        assets: [],
        total: 0,
        total_pages: 0,
        page: 1,
      })),
    ]);
    graph.value = g || { nodes: [], edges: [] };
    facts.value = f || [];
    assets.value = (a?.assets || []).filter((x) =>
      ASSET_SOURCES.includes(x.source),
    );
  } catch {
    message.error('数据加载失败');
  } finally {
    loading.value = false;
  }
}

function onRefresh() {
  fetchAll();
}

watch(project, fetchAll);
onMounted(loadProjects);

// ── 事实详情（body JSON 解析） ─────────────────────────────────────────────
const factByKey = computed(() => {
  const map = new Map<string, PentFact>();
  for (const f of facts.value) map.set(f.fact_key, f);
  return map;
});

function parseBody(factKey: string): Record<string, any> {
  const raw = factByKey.value.get(factKey)?.body || '';
  if (!raw) return {};
  try {
    const b = JSON.parse(raw);
    if (b && typeof b === 'object') return b;
  } catch {
    // 兼容平台自动追加的 "## 关联" 尾部文本，尝试截取其中的 JSON
    try {
      const m = raw.match(/\{[\s\S]*\}/);
      if (m) {
        const b = JSON.parse(m[0]);
        if (b && typeof b === 'object') return b;
      }
    } catch {
      /* 详情缺失不影响主渲染 */
    }
  }
  return {};
}

// 公司名: summary 常混入"（百胜中国100%；对外投资N家...）"等总结, 提取纯名字
function extractCompanyName(raw: string): string {
  if (!raw) return raw || '';
  const s = String(raw).trim();
  let cut = s.length;
  const reBracket =
    /[（(](?=[^（(]*?(持股|控股|对外投资|参保|注册资本|注册|法人|官网|电话|邮箱|信用代码|任务书|小蓝本|在营|注销|实缴|万|USD|%|成立|branchCnt|门店))/;
  const m = s.match(reBracket);
  if (m && m.index && m.index > 0) {
    let closeIdx = s.indexOf('）', m.index);
    if (closeIdx < 0) closeIdx = s.indexOf(')', m.index);
    if (closeIdx > 0) {
      const inner = s.slice(m.index + 1, closeIdx);
      if (!/(公司|集团|厂|店|社|部|中心|有限|总店)$/.test(inner)) {
        cut = Math.min(cut, m.index);
      }
    }
  }
  const semi = s.search(/[；;]/);
  if (semi > 0) cut = Math.min(cut, semi);
  const m2 = s.match(/\((?=[^(]*?(holding|invest|subsidiar|100%|\d+%))/i);
  if (m2 && m2.index && m2.index > 0) cut = Math.min(cut, m2.index);
  const name = s.slice(0, cut).trim();
  return name || s;
}

// ── 公司层级 ───────────────────────────────────────────────────────────────
const graphNodes = computed(() =>
  graph.value.nodes.filter((n) => !n.deprecated),
);

function isCompany(n: { category: string; fact_key: string }) {
  return n.category === 'company' || n.fact_key.startsWith('company/');
}

const companyNodes = computed(() => graphNodes.value.filter(isCompany));

const nodeByKey = computed(() => {
  const map = new Map<string, PentFactGraph['nodes'][number]>();
  for (const n of graphNodes.value) map.set(n.fact_key, n);
  return map;
});

const containsEdges = computed(() =>
  graph.value.edges.filter((e) => e.edge_type === 'contains'),
);

// source 持股/包含 target：source → 父，target → 子
const childMap = computed(() => {
  const map = new Map<string, string[]>();
  for (const e of containsEdges.value) {
    const list = map.get(e.source_fact_key) || [];
    list.push(e.target_fact_key);
    map.set(e.source_fact_key, list);
  }
  return map;
});

const rootKeys = computed(() => {
  const hasParent = new Set(containsEdges.value.map((e) => e.target_fact_key));
  return companyNodes.value
    .filter((n) => !hasParent.has(n.fact_key))
    .map((n) => n.fact_key);
});

// 未关联公司（非根且无入边的公司，原版保留该兜底区块）
const orphanCompanies = computed(() => {
  const roots = new Set(rootKeys.value);
  return companyNodes.value.filter(
    (n) =>
      !roots.has(n.fact_key) &&
      !containsEdges.value.some((e) => e.target_fact_key === n.fact_key),
  );
});

// ── 统计 ───────────────────────────────────────────────────────────────────
const stats = computed(() => {
  let lv1 = 0;
  let lv2 = 0;
  for (const rk of rootKeys.value) {
    const c1 = childMap.value.get(rk) || [];
    lv1 += c1.length;
    for (const c of c1) lv2 += (childMap.value.get(c) || []).length;
  }
  return {
    companies: companyNodes.value.length,
    roots: rootKeys.value.length,
    level1: lv1,
    level2: lv2,
  };
});

// ── 展开/收起（默认全部收起） ──────────────────────────────────────────────
const openKeys = ref<Set<string>>(new Set());

function descendantsOf(key: string): string[] {
  const out: string[] = [];
  const walk = (k: string) => {
    for (const c of childMap.value.get(k) || []) {
      out.push(c);
      walk(c);
    }
  };
  walk(key);
  return out;
}

function toggleNode(key: string) {
  const next = new Set(openKeys.value);
  if (next.has(key)) {
    // 收起时把子树内所有子节点也收起
    next.delete(key);
    for (const k of descendantsOf(key)) next.delete(k);
  } else {
    next.add(key);
  }
  openKeys.value = next;
}

function expandAll(open: boolean) {
  openKeys.value = open
    ? new Set(companyNodes.value.map((n) => n.fact_key))
    : new Set();
}

// ── 树节点视图模型（拍平 + 缩进，仅显示祖先全部展开的节点） ────────────────
interface TreeRow {
  key: string;
  depth: number;
  name: string;
  rawName: string;
  ratio: null | number;
  meta: string[];
  domains: string[];
  kidCount: number;
  detailRows: Array<{ k: string; v: string }>;
  hasDetail: boolean;
  open: boolean;
}

const DETAIL_LABELS: Record<string, string> = {
  legal: '法定代表人',
  regcap: '注册资本',
  esdate: '成立日期',
  creditCode: '统一社会信用代码',
  regno: '注册号',
  status: '经营状态',
  valuation: '估值',
  stockCode: '股票代码',
  industry: '行业',
  address: '地址',
  website: '官网',
  phone: '电话',
  email: '邮箱',
  shareholder: '股东',
  group: '集团',
  cid: '小蓝本ID',
  city: '城市',
  ratio: '持股比例',
  fr: '法定代表人',
  name: '公司名称',
};

function buildRow(key: string, depth: number, parentName: string): TreeRow | null {
  const n = nodeByKey.value.get(key);
  if (!n) return null;
  const body = parseBody(key);
  const rawName = n.summary || key;
  const name = extractCompanyName(rawName);
  const ratioRaw = body.ratio;
  const ratio =
    ratioRaw === null || ratioRaw === undefined || ratioRaw === ''
      ? null
      : Number.parseFloat(String(ratioRaw).replace('%', ''));
  const meta: string[] = [];
  if (body.legal) meta.push(`法人: ${body.legal}`);
  if (body.city) meta.push(String(body.city));
  if (body.creditCode) meta.push(`信用代码: ${body.creditCode}`);
  if (body.regcap) meta.push(`注册资本: ${body.regcap}`);
  if (parentName) meta.push(`母公司: ${parentName}`);
  const doms: string[] = [];
  if (body.website && body.website !== 'null') doms.push(String(body.website));
  for (const d of body.domains || body.websites || []) {
    if (d && d !== 'null') {
      doms.push(typeof d === 'object' ? d.url || d.domain || '' : String(d));
    }
  }
  const detailRows: Array<{ k: string; v: string }> = [];
  if (rawName !== name && rawName.trim()) {
    detailRows.push({ k: '备注', v: rawName.trim() });
  }
  for (const k of Object.keys(body)) {
    const v = body[k];
    if (v === '' || v === null || v === undefined) continue;
    detailRows.push({
      k: DETAIL_LABELS[k] || k,
      v: typeof v === 'object' ? JSON.stringify(v) : String(v),
    });
  }
  const kids = (childMap.value.get(key) || []).filter((k) =>
    nodeByKey.value.has(k),
  );
  return {
    key,
    depth,
    name,
    rawName,
    ratio: ratio === null || Number.isNaN(ratio) ? null : ratio,
    meta,
    domains: doms.filter(Boolean),
    kidCount: kids.length,
    detailRows,
    hasDetail: detailRows.length > 0,
    open: openKeys.value.has(key),
  };
}

const treeRows = computed<TreeRow[]>(() => {
  const rows: TreeRow[] = [];
  const walk = (key: string, depth: number, parentName: string, visible: boolean) => {
    if (depth > 16) return;
    const row = buildRow(key, depth, parentName);
    if (!row) return;
    if (visible) rows.push(row);
    const kids = (childMap.value.get(key) || []).filter((k) =>
      nodeByKey.value.has(k),
    );
    for (const k of kids) {
      walk(k, depth + 1, row.name, visible && row.open);
    }
  };
  for (const rk of rootKeys.value) walk(rk, 0, '', true);
  for (const n of orphanCompanies.value) walk(n.fact_key, 0, '', true);
  return rows;
});

const hasTree = computed(
  () => rootKeys.value.length > 0 || orphanCompanies.value.length > 0,
);

// ── 非公司类事实区块: 融资 / 邮箱 / ICP备案 / 其他 ─────────────────────────
interface FactBlock {
  category: string;
  title: string;
  items: any[];
}

const BLOCK_TITLES: Record<string, string> = {
  financing: '融资记录',
  contacts: '联系人邮箱',
  icp: 'ICP 备案',
  other: '其他发现',
};

const factBlocks = computed<FactBlock[]>(() => {
  const grouped = new Map<string, PentFactGraph['nodes']>();
  for (const n of graphNodes.value) {
    const cat = n.category || 'other';
    if (cat === 'company' || n.fact_key.startsWith('company/')) continue;
    const list = grouped.get(cat) || [];
    list.push(n);
    grouped.set(cat, list);
  }
  const blocks: FactBlock[] = [];
  for (const [cat, nodes] of grouped) {
    const items: any[] = [];
    if (cat === 'financing') {
      for (const n of nodes) {
        const b = parseBody(n.fact_key);
        const rows = Array.isArray(b.list) ? b.list : [b];
        for (const r of rows) {
          items.push({
            date: r.date || '',
            round: r.round || '',
            amount: r.amount || '',
            investors: r.investors || '',
          });
        }
      }
    } else if (cat === 'contacts') {
      for (const n of nodes) {
        const b = parseBody(n.fact_key);
        const list = Array.isArray(b.list) ? b.list : b.emails || [];
        for (const e of list) {
          const val =
            e && typeof e === 'object'
              ? e.email || e.addr || e.address || e.value || ''
              : e;
          const remark = e && typeof e === 'object' && e.remark ? e.remark : '';
          if (val) items.push({ val: String(val), remark });
        }
      }
    } else if (cat === 'icp') {
      for (const n of nodes) {
        const b = parseBody(n.fact_key);
        items.push({
          label: n.summary || n.fact_key,
          icp: b.icp || '',
          subject: b.subject || '',
        });
      }
    } else {
      for (const n of nodes) {
        const b = parseBody(n.fact_key);
        const parts: string[] = [];
        for (const k of Object.keys(b)) {
          const v = b[k];
          if (v === null || v === undefined) continue;
          if (typeof v === 'object') {
            const arr = Array.isArray(v) ? v : [v];
            for (const x of arr) {
              if (x && typeof x === 'object') {
                const pick =
                  x.domain ||
                  x.url ||
                  x.name ||
                  x.email ||
                  x.host ||
                  (Array.isArray(x.ips) ? x.ips.join(',') : '');
                if (pick) parts.push(String(pick));
              } else if (x !== null && x !== undefined) {
                parts.push(String(x));
              }
            }
          } else if (typeof v === 'string' && v.length < 300) {
            parts.push(v);
          }
        }
        const txt = parts.length ? parts.join(' · ') : n.summary || n.fact_key;
        items.push({ val: txt.slice(0, 200) });
      }
    }
    blocks.push({
      category: cat,
      title: BLOCK_TITLES[cat] || cat,
      items,
    });
  }
  return blocks;
});

// ── 关联域名资产（来自小蓝本/DNS，含 ICP 备案） ───────────────────────────
interface AssetChip {
  key: string;
  icp: string[];
}

const assetChips = computed<AssetChip[]>(() => {
  const seen = new Map<string, AssetChip>();
  for (const a of assets.value) {
    const k = a.domain || a.host || a.ip;
    if (!k || seen.has(k)) continue;
    const icp = (a.tags || [])
      .filter((t) => /^icp:/i.test(t))
      .map((t) => t.replace(/^icp:/i, ''));
    seen.set(k, { key: k, icp });
  }
  return [...seen.values()];
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="股权穿透（子孙公司）">
      <template #extra>
        <a-select
          v-model:value="project"
          :options="projectOptions"
          placeholder="选择项目"
          style="width: 200px"
        />
        <a-button style="margin-left: 8px" :loading="loading" @click="onRefresh">刷新</a-button>
      </template>
    </a-page-header>

    <a-spin :spinning="loading">
      <!-- 统计卡片 -->
      <a-row :gutter="16" style="margin-top: 16px">
        <a-col :xs="12" :lg="6">
          <a-card size="small"><a-statistic title="公司节点" :value="stats.companies" /></a-card>
        </a-col>
        <a-col :xs="12" :lg="6">
          <a-card size="small"><a-statistic title="根公司" :value="stats.roots" /></a-card>
        </a-col>
        <a-col :xs="12" :lg="6">
          <a-card size="small"><a-statistic title="一级子公司" :value="stats.level1" /></a-card>
        </a-col>
        <a-col :xs="12" :lg="6">
          <a-card size="small"><a-statistic title="二级孙公司" :value="stats.level2" /></a-card>
        </a-col>
      </a-row>

      <!-- 公司层级 -->
      <div style="margin: 14px 0 8px; font-size: 13px; font-weight: 600; color: var(--ant-color-text-tertiary)">
        公司层级（基于项目事实图，contains = 持股/控股关系）
      </div>
      <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 8px">
        <a-button size="small" @click="expandAll(true)">全部展开</a-button>
        <a-button size="small" @click="expandAll(false)">全部收起</a-button>
        <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
          {{ stats.companies }} 家公司 · 点击卡片或 ± 展开/收起
        </span>
      </div>

      <template v-if="hasTree">
        <div>
          <template v-for="row in treeRows" :key="row.key">
            <div
              :style="{
                margin: '6px 0',
                paddingLeft: row.depth === 0 ? '0' : '14px',
                marginLeft: row.depth === 0 ? '0' : `${row.depth * 14}px`,
                borderLeft: row.depth === 0 ? 'none' : '2px solid var(--ant-color-border-secondary)',
              }"
            >
              <div
                style="display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 10px 14px; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; cursor: pointer"
                @click="toggleNode(row.key)"
              >
                <a-button
                  v-if="row.kidCount"
                  type="text"
                  size="small"
                  style="padding: 2px 6px"
                  @click.stop="toggleNode(row.key)"
                >
                  {{ row.open ? '−' : '+' }}
                </a-button>
                <span style="font-weight: 600; font-size: 14px">{{ row.name }}</span>
                <a-tag v-if="row.ratio !== null" :color="row.ratio < 50 ? 'orange' : 'blue'">
                  持股 {{ row.ratio }}%
                </a-tag>
                <span
                  v-if="row.meta.length"
                  style="font-size: 12px; color: var(--ant-color-text-tertiary)"
                >
                  {{ row.meta.join(' · ') }}
                </span>
                <span v-if="row.domains.length" style="display: inline-flex; gap: 4px; flex-wrap: wrap">
                  <a-tag v-for="d in row.domains" :key="d" color="blue" style="font-size: 11px">{{ d }}</a-tag>
                </span>
                <span
                  v-if="row.kidCount"
                  style="margin-left: auto; font-size: 12px; color: var(--ant-color-text-tertiary)"
                >
                  {{ row.kidCount }} 家下级
                </span>
              </div>
              <div
                v-if="row.open && row.hasDetail"
                style="margin-top: 6px; padding: 8px 12px; border-radius: 6px; font-size: 12px; background: var(--ant-color-fill-quaternary)"
              >
                <div
                  v-for="(r, i) in row.detailRows"
                  :key="i"
                  style="display: flex; gap: 10px; padding: 2px 0"
                >
                  <span style="min-width: 110px; flex-shrink: 0; color: var(--ant-color-text-tertiary)">{{ r.k }}</span>
                  <span style="word-break: break-all">{{ r.v }}</span>
                </div>
              </div>
            </div>
          </template>
        </div>
      </template>
      <a-empty
        v-else
        style="padding: 24px; border: 1px dashed var(--ant-color-border-secondary); border-radius: 8px"
        description="暂无供应链数据 — 让 AI 用「信息收集 → AI 一键收集(股权穿透模式)」收集后自动落库，或先给项目导入数据"
      />

      <!-- 非公司类事实区块: 融资 / 邮箱 / ICP备案 / 其他 -->
      <template v-for="block in factBlocks" :key="block.category">
        <div style="margin: 14px 0 8px; font-size: 13px; font-weight: 600; color: var(--ant-color-text-tertiary)">
          {{ block.title }}（{{ block.items.length }}）
        </div>
        <!-- 融资记录 -->
        <div v-if="block.category === 'financing'" style="display: flex; flex-direction: column; gap: 6px">
          <div
            v-for="(it, i) in block.items"
            :key="i"
            style="display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 10px 14px; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px"
          >
            <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">{{ it.date }}</span>
            <span style="font-weight: 600; font-size: 14px">{{ it.round }}</span>
            <a-tag v-if="it.amount" color="blue">{{ it.amount }}</a-tag>
            <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">{{ it.investors }}</span>
          </div>
        </div>
        <!-- 联系人邮箱 -->
        <div v-else-if="block.category === 'contacts'" style="display: flex; flex-wrap: wrap; gap: 8px">
          <a-tag v-for="(it, i) in block.items" :key="i" :title="it.remark">
            {{ it.val }}<template v-if="it.remark"> ({{ it.remark }})</template>
          </a-tag>
        </div>
        <!-- ICP 备案 -->
        <div v-else-if="block.category === 'icp'" style="display: flex; flex-wrap: wrap; gap: 8px">
          <a-tag v-for="(it, i) in block.items" :key="i" :title="it.subject" color="purple">
            {{ it.label }}<template v-if="it.icp"> [{{ it.icp }}]</template>
          </a-tag>
        </div>
        <!-- 其他发现 -->
        <div v-else style="display: flex; flex-wrap: wrap; gap: 8px">
          <a-tag v-for="(it, i) in block.items" :key="i">{{ it.val }}</a-tag>
        </div>
      </template>

      <!-- 关联域名资产 -->
      <template v-if="assetChips.length">
        <div style="margin: 14px 0 8px; font-size: 13px; font-weight: 600; color: var(--ant-color-text-tertiary)">
          关联域名资产（{{ assetChips.length }}，来自小蓝本/DNS，含 ICP 备案）
        </div>
        <div style="display: flex; flex-wrap: wrap; gap: 8px">
          <a-tag
            v-for="chip in assetChips"
            :key="chip.key"
            color="cyan"
            :title="chip.icp.length ? `备案: ${chip.icp.join(', ')}` : chip.key"
          >
            {{ chip.key }}
            <span v-if="chip.icp.length" style="color: var(--ant-color-primary)"> [{{ chip.icp[0] }}]</span>
          </a-tag>
        </div>
      </template>
      <a-empty
        v-else
        style="padding: 24px; border: 1px dashed var(--ant-color-border-secondary); border-radius: 8px; margin-top: 14px"
        description="暂无域名资产"
      />
    </a-spin>
  </div>
</template>
