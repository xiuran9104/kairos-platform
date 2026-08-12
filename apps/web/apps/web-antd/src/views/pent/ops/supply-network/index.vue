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
      project.value = projects.value[0]?.id;
    }
  } catch {
    projects.value = [];
  }
}

// ── 数据加载 ───────────────────────────────────────────────────────────────
const graph = ref<PentFactGraph>({ nodes: [], edges: [] });
const facts = ref<PentFact[]>([]);
const assets = ref<PentAsset[]>([]);

// 与原版一致：按多个来源分别拉取资产后合并
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
    const [g, f, assetLists] = await Promise.all([
      getPentProjectFactGraph(project.value).catch(() => ({
        nodes: [],
        edges: [],
      })),
      getPentProjectFacts(project.value).catch(() => []),
      Promise.all(
        ASSET_SOURCES.map((source) =>
          getPentAssets({
            project_id: project.value,
            source,
            page_size: 100,
          })
            .then((r) => r?.assets || [])
            .catch(() => [] as PentAsset[]),
        ),
      ),
    ]);
    graph.value = g || { nodes: [], edges: [] };
    facts.value = f || [];
    const merged = new Map<string, PentAsset>();
    for (const list of assetLists) {
      for (const a of list) merged.set(a.id, a);
    }
    assets.value = [...merged.values()];
  } catch {
    message.error('数据加载失败');
  } finally {
    loading.value = false;
  }
}

watch(project, fetchAll);
onMounted(loadProjects);

// ── fact body 解析 ─────────────────────────────────────────────────────────
const factByKey = computed(() => {
  const map = new Map<string, PentFact>();
  for (const f of facts.value) map.set(f.fact_key, f);
  return map;
});

function parseBody(factKey: string): Record<string, any> {
  const raw = factByKey.value.get(factKey)?.body || '';
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object') return parsed;
  } catch {
    /* 尝试截取 JSON 片段 */
  }
  try {
    const m = raw.match(/\{[\s\S]*\}/);
    if (m) {
      const parsed = JSON.parse(m[0]);
      if (parsed && typeof parsed === 'object') return parsed;
    }
  } catch {
    /* 忽略 */
  }
  return {};
}

// ── 供应链公司（company 类且名称含 供应链/供应/物流） ───────────────────────
const SUPPLY_RE = /供应链|供应|物流/;

const supplyCompanies = computed(() =>
  graph.value.nodes
    .filter((n) => {
      if (n.deprecated) return false;
      if (n.category !== 'company' && !n.fact_key.startsWith('company/'))
        return false;
      const body = parseBody(n.fact_key);
      const name = body.name || n.summary || n.fact_key;
      return SUPPLY_RE.test(String(name));
    })
    .map((n) => {
      const body = parseBody(n.fact_key);
      const rawRatio = body.ratio;
      const ratio =
        rawRatio === null || rawRatio === undefined || rawRatio === ''
          ? null
          : Number.parseFloat(String(rawRatio).replace('%', ''));
      return {
        key: n.fact_key,
        name: body.name || n.summary || n.fact_key,
        ratio: ratio === null || Number.isNaN(ratio) ? null : ratio,
        legal: body.legal || body.fr,
        city: body.city,
        creditCode: body.creditCode,
        regcap: body.regcap,
      };
    }),
);

// ── 供应链事实（fact_key 前缀 supplychain/ 或分类 supplychain/infra） ──────
function isSupplyFact(factKey: string, category: string) {
  return (
    factKey.startsWith('supplychain/') ||
    category === 'supplychain' ||
    category === 'infra'
  );
}

// 与原版一致：事实列表与统计均取自 fact-graph 节点，body 取自 facts 接口
const supplyFacts = computed(() =>
  graph.value.nodes
    .filter((n) => !n.deprecated && isSupplyFact(n.fact_key, n.category))
    .map((n) => {
      const body = parseBody(n.fact_key);
      const bodyText = Object.keys(body).length
        ? Object.values(body).join(' · ')
        : '';
      return {
        key: n.fact_key,
        name: n.summary || n.fact_key,
        bodyText: bodyText.slice(0, 200),
      };
    }),
);

// ── 数字化资产集群（按可注册主域名聚类） ───────────────────────────────────
const SECOND_LEVEL_SUFFIXES = new Set([
  'com.cn',
  'net.cn',
  'org.cn',
  'gov.cn',
  'ac.cn',
]);

function registrableDomain(domain: string): string {
  let d = (domain || '').toLowerCase().trim();
  if (d.startsWith('www.')) d = d.slice(4);
  if (!d || d.startsWith('xn--')) return d;
  const parts = d.split('.').filter(Boolean);
  if (parts.length <= 2) return d;
  const last2 = parts.slice(-2).join('.');
  if (SECOND_LEVEL_SUFFIXES.has(last2) && parts.length >= 3) {
    return parts.slice(-3).join('.');
  }
  return last2;
}

// 子域业务标签（对照原版 HWWT_TAGS）
const SUB_TAGS: Record<string, string> = {
  vysccp: '供应商协同门户',
  esp: '需求预测平台ESP',
  dcps: '供应链协同DCPS',
  'cps-api': '协同API',
  scmcollege: '供应链学院',
  ifoodsafety: '食安追溯',
  esign: '电子签',
  magicbox: '数据中台',
  'cmd.fis': '财务系统FIS',
  ssotest: 'SSO测试',
  ipsuat: 'IP系统UAT',
  'ms-f-ifd': '消息服务',
  portal: '门户',
  master: '主数据',
  ssdp: 'SSDP',
  vlps: 'VLPS',
  rios: 'RIOS',
};

interface SubItem {
  sub: string;
  domain: string;
  ip: string;
}

interface DomainCluster {
  root: string;
  total: number;
  mainIp: string;
  groups: Array<{ label: string; subs: SubItem[] }>;
}

const clusters = computed<DomainCluster[]>(() => {
  const map = new Map<string, { domain: string; ip: string }[]>();
  for (const a of assets.value) {
    const dom = a.domain || a.host || a.ip || '';
    const root = registrableDomain(dom);
    if (!root) continue;
    const list = map.get(root) || [];
    list.push({ domain: dom, ip: a.ip || '' });
    map.set(root, list);
  }
  return [...map.entries()]
    .map(([root, subs]) => {
      const full = subs.filter((s) => s.domain !== root);
      const groupMap = new Map<string, SubItem[]>();
      for (const s of full) {
        const sub = s.domain.toLowerCase().replace(`.${root}`, '');
        const base = sub.split('.')[0] || '';
        const label =
          SUB_TAGS[base] ||
          (sub.includes('test') || sub.includes('uat') ? '测试环境' : '其他功能');
        const list = groupMap.get(label) || [];
        list.push({ ...s, sub });
        groupMap.set(label, list);
      }
      return {
        root,
        total: subs.length,
        mainIp: subs.find((s) => s.domain === root)?.ip || '',
        groups: [...groupMap.entries()].map(([label, items]) => ({
          label,
          subs: items,
        })),
      };
    })
    .sort((a, b) => b.total - a.total);
});

// 原版默认展开，点击头部折叠
const collapsedClusters = ref<Set<string>>(new Set());
function toggleCluster(root: string) {
  const next = new Set(collapsedClusters.value);
  if (next.has(root)) next.delete(root);
  else next.add(root);
  collapsedClusters.value = next;
}
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header
      title="供应链（供应商/供应链资产）"
      style="padding: 0 0 16px"
    >
      <template #extra>
        <a-select
          v-model:value="project"
          :options="projectOptions"
          placeholder="选择项目"
          style="width: 220px"
        />
        <a-button
          style="margin-left: 8px"
          :loading="loading"
          @click="fetchAll"
        >
          刷新
        </a-button>
      </template>
    </a-page-header>

    <a-spin :spinning="loading">
      <!-- 统计卡片 -->
      <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-bottom: 14px">
        <a-card size="small" style="min-width: 110px">
          <a-statistic
            title="供应链公司"
            :value="supplyCompanies.length"
            :value-style="{ fontSize: '22px', fontWeight: 700 }"
          />
        </a-card>
        <a-card size="small" style="min-width: 110px">
          <a-statistic
            title="供应链事实"
            :value="supplyFacts.length"
            :value-style="{ fontSize: '22px', fontWeight: 700 }"
          />
        </a-card>
      </div>

      <!-- 供应链公司 -->
      <div
        style="
          font-size: 13px;
          color: var(--ant-color-text-secondary);
          margin: 14px 0 8px;
          font-weight: 600;
        "
      >
        供应链公司（名称含 供应链/供应/物流）
      </div>
      <div
        v-if="supplyCompanies.length"
        style="
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
          gap: 10px;
        "
      >
        <a-card v-for="c in supplyCompanies" :key="c.key" size="small">
          <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap">
            <span style="font-weight: 600; font-size: 14px">{{ c.name }}</span>
            <a-tag v-if="c.ratio !== null" :color="c.ratio < 50 ? 'gold' : 'blue'">
              持股 {{ c.ratio }}%
            </a-tag>
          </div>
          <div style="margin-top: 6px">
            <template v-if="c.legal || c.city || c.creditCode || c.regcap">
              <div
                v-if="c.legal"
                style="display: flex; gap: 10px; padding: 2px 0; font-size: 12px"
              >
                <span style="min-width: 100px; color: var(--ant-color-text-secondary); flex-shrink: 0">法人</span>
                <span style="word-break: break-all">{{ c.legal }}</span>
              </div>
              <div
                v-if="c.city"
                style="display: flex; gap: 10px; padding: 2px 0; font-size: 12px"
              >
                <span style="min-width: 100px; color: var(--ant-color-text-secondary); flex-shrink: 0">城市</span>
                <span style="word-break: break-all">{{ c.city }}</span>
              </div>
              <div
                v-if="c.creditCode"
                style="display: flex; gap: 10px; padding: 2px 0; font-size: 12px"
              >
                <span style="min-width: 100px; color: var(--ant-color-text-secondary); flex-shrink: 0">信用代码</span>
                <span style="word-break: break-all">{{ c.creditCode }}</span>
              </div>
              <div
                v-if="c.regcap"
                style="display: flex; gap: 10px; padding: 2px 0; font-size: 12px"
              >
                <span style="min-width: 100px; color: var(--ant-color-text-secondary); flex-shrink: 0">注册资本</span>
                <span style="word-break: break-all">{{ c.regcap }}</span>
              </div>
            </template>
            <span v-else style="font-size: 12px; color: var(--ant-color-text-secondary)">
              无详情（待补）
            </span>
          </div>
        </a-card>
      </div>
      <div
        v-else
        style="
          color: var(--ant-color-text-secondary);
          padding: 24px;
          text-align: center;
          border: 1px dashed var(--ant-color-border);
          border-radius: 8px;
        "
      >
        暂无供应链公司（company facts 名称含 供应链/供应/物流 自动识别）
      </div>

      <!-- 供应链数字化资产 -->
      <div
        style="
          font-size: 13px;
          color: var(--ant-color-text-secondary);
          margin: 14px 0 8px;
          font-weight: 600;
        "
      >
        供应链数字化资产（按主域名集群）
      </div>
      <template v-if="clusters.length">
        <div
          v-for="(c, i) in clusters"
          :key="c.root"
          style="
            border: 1px solid var(--ant-color-border-secondary);
            border-radius: 10px;
            margin: 10px 0;
            overflow: hidden;
          "
        >
          <div
            style="
              display: flex;
              align-items: center;
              gap: 10px;
              padding: 10px 14px;
              cursor: pointer;
              background: var(--ant-color-fill-quaternary);
            "
            @click="toggleCluster(c.root)"
          >
            <span style="font-size: 13px">{{ i + 1 }}.</span>
            <span style="font-weight: 700; font-size: 15px">{{ c.root }}</span>
            <a-tag>{{ c.total }} 个子域/资产</a-tag>
            <a-tag v-if="c.mainIp">主站 {{ c.mainIp }}</a-tag>
            <span style="margin-left: auto; color: var(--ant-color-text-secondary)">
              {{ collapsedClusters.has(c.root) ? '▸' : '▾' }}
            </span>
          </div>
          <div v-if="!collapsedClusters.has(c.root)" style="padding: 10px 14px">
            <template v-if="c.groups.length">
              <div v-for="g in c.groups" :key="g.label" style="margin: 6px 0">
                <div
                  style="
                    font-size: 12px;
                    color: var(--ant-color-text-secondary);
                    margin-bottom: 4px;
                  "
                >
                  {{ g.label }}
                </div>
                <a-tag
                  v-for="s in g.subs"
                  :key="s.domain"
                  style="font-family: Consolas, monospace; margin-bottom: 4px"
                >
                  {{ s.sub }}.{{ c.root
                  }}<span
                    v-if="s.ip"
                    style="color: var(--ant-color-text-secondary); font-size: 11px"
                  >
                    ({{ s.ip }})</span
                  >
                </a-tag>
              </div>
            </template>
            <span v-else style="font-size: 12px; color: var(--ant-color-text-secondary)">
              仅主域名（无子域）
            </span>
          </div>
        </div>
      </template>
      <div
        v-else
        style="
          color: var(--ant-color-text-secondary);
          padding: 24px;
          text-align: center;
          border: 1px dashed var(--ant-color-border);
          border-radius: 8px;
        "
      >
        暂无资产
      </div>

      <!-- 供应链事实 -->
      <div
        style="
          font-size: 13px;
          color: var(--ant-color-text-secondary);
          margin: 14px 0 8px;
          font-weight: 600;
        "
      >
        供应链事实（收集结论）
      </div>
      <template v-if="supplyFacts.length">
        <a-card
          v-for="f in supplyFacts"
          :key="f.key"
          size="small"
          style="margin: 6px 0"
        >
          <span style="font-weight: 600; font-size: 14px">{{ f.name }}</span>
          <div
            v-if="f.bodyText"
            style="
              font-size: 12px;
              color: var(--ant-color-text-secondary);
              margin-top: 4px;
            "
          >
            {{ f.bodyText }}
          </div>
        </a-card>
      </template>
      <div
        v-else
        style="
          color: var(--ant-color-text-secondary);
          padding: 24px;
          text-align: center;
          border: 1px dashed var(--ant-color-border);
          border-radius: 8px;
        "
      >
        暂无供应链事实
      </div>
    </a-spin>
  </div>
</template>
