<script lang="ts" setup>
import type { Edge, Node } from '@vue-flow/core';
import type { PentFactGraph } from '#/api/pent/ops-facts';

import { nextTick, onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { MarkerType, useVueFlow, VueFlow } from '@vue-flow/core';

import {
  createPentFactEdge,
  deletePentFactEdge,
  getPentProjectFactGraph,
} from '#/api/pent/ops-facts';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';

const props = defineProps<{ projectId: string }>();

const CATEGORY_COLOR: Record<string, string> = {
  target: '#1677ff',
  auth: '#722ed1',
  infra: '#13c2c2',
  business: '#2f54eb',
  finding: '#f5222d',
  chain: '#fa8c16',
  exploit: '#eb2f96',
  poc: '#fa541c',
  note: '#8c8c8c',
  company: '#52c41a',
  supplychain: '#a0d911',
};

const loading = ref(false);
const graph = ref<PentFactGraph>({ nodes: [], edges: [] });
const nodes = ref<Node[]>([]);
const edges = ref<Edge[]>([]);

const { fitView, onNodeClick } = useVueFlow();

// ── BFS 分层布局 ───────────────────────────────────────────────────────────
function layoutNodes(g: PentFactGraph): Node[] {
  const nodeIds = g.nodes.map((n) => n.fact_key);
  const incoming = new Map<string, number>();
  const adj = new Map<string, string[]>();
  for (const id of nodeIds) {
    incoming.set(id, 0);
    adj.set(id, []);
  }
  for (const e of g.edges) {
    if (!incoming.has(e.source_fact_key) || !incoming.has(e.target_fact_key)) continue;
    incoming.set(
      e.target_fact_key,
      (incoming.get(e.target_fact_key) || 0) + 1,
    );
    adj.get(e.source_fact_key)?.push(e.target_fact_key);
  }
  // 按入度分层（Kahn 简化：反复剥进入度 0 的层；有环时剩余节点放最后一层）
  const layerOf = new Map<string, number>();
  let frontier = nodeIds.filter((id) => (incoming.get(id) || 0) === 0);
  const remaining = new Map(incoming);
  let layer = 0;
  const assigned = new Set<string>();
  while (frontier.length && layer < 20) {
    const next: string[] = [];
    for (const id of frontier) {
      if (assigned.has(id)) continue;
      assigned.add(id);
      layerOf.set(id, layer);
      for (const tgt of adj.get(id) || []) {
        remaining.set(tgt, (remaining.get(tgt) || 1) - 1);
        if ((remaining.get(tgt) || 0) <= 0 && !assigned.has(tgt)) next.push(tgt);
      }
    }
    frontier = next;
    layer++;
  }
  for (const id of nodeIds) if (!layerOf.has(id)) layerOf.set(id, layer);
  // 同层横排
  const perLayer = new Map<number, string[]>();
  for (const id of nodeIds) {
    const l = layerOf.get(id) || 0;
    const list = perLayer.get(l) || [];
    list.push(id);
    perLayer.set(l, list);
  }
  return g.nodes.map((n) => {
    const l = layerOf.get(n.fact_key) || 0;
    const idx = (perLayer.get(l) || []).indexOf(n.fact_key);
    const color = CATEGORY_COLOR[n.category] || '#8c8c8c';
    return {
      id: n.fact_key,
      position: { x: idx * 240, y: l * 140 },
      data: { label: n.summary || n.fact_key, category: n.category },
      style: {
        border: `2px solid ${color}`,
        borderRadius: '8px',
        fontSize: '12px',
        width: '200px',
        opacity: n.deprecated ? 0.45 : 1,
      },
    } as Node;
  });
}

function rebuild() {
  nodes.value = layoutNodes(graph.value);
  edges.value = graph.value.edges.map((e) => ({
    id: e.id,
    source: e.source_fact_key,
    target: e.target_fact_key,
    label: e.edge_type || undefined,
    markerEnd: MarkerType.ArrowClosed,
  }));
  nextTick(() => fitView({ padding: 0.2 }));
}

async function fetchGraph() {
  loading.value = true;
  try {
    graph.value = (await getPentProjectFactGraph(props.projectId)) || {
      nodes: [],
      edges: [],
    };
    rebuild();
  } catch {
    message.error('加载事实图失败');
    graph.value = { nodes: [], edges: [] };
    nodes.value = [];
    edges.value = [];
  } finally {
    loading.value = false;
  }
}

// ── 连边模式 ───────────────────────────────────────────────────────────────
const connectMode = ref(false);
const connectSource = ref<string>('');
const showEdgeModal = ref(false);
const edgeType = ref('related');
let connectTarget = '';

function onNodeClickHandler(factKey: string) {
  if (!connectMode.value) {
    openDetail(factKey);
    return;
  }
  if (!connectSource.value) {
    connectSource.value = factKey;
    message.info(`已选源节点 ${factKey}，请点击目标节点`);
    return;
  }
  if (connectSource.value === factKey) {
    message.warning('不能连接自身');
    return;
  }
  connectTarget = factKey;
  edgeType.value = 'related';
  showEdgeModal.value = true;
}

onNodeClick(({ node }) => onNodeClickHandler(node.id));

async function confirmEdge() {
  try {
    await createPentFactEdge({
      project_id: props.projectId,
      source_fact_key: connectSource.value,
      target_fact_key: connectTarget,
      edge_type: edgeType.value.trim() || 'related',
    });
    message.success('已创建连边');
    showEdgeModal.value = false;
    connectSource.value = '';
    await fetchGraph();
  } catch {
    message.error('创建失败（节点不存在或重复）');
  }
}

function cancelConnect() {
  connectMode.value = false;
  connectSource.value = '';
}

// ── 节点详情 ───────────────────────────────────────────────────────────────
const detailKey = ref('');
const showDetail = ref(false);

const detailNode = ref<PentFactGraph['nodes'][number] | null>(null);
const detailEdges = ref<PentFactGraph['edges']>([]);

function openDetail(factKey: string) {
  detailKey.value = factKey;
  detailNode.value =
    graph.value.nodes.find((n) => n.fact_key === factKey) || null;
  detailEdges.value = graph.value.edges.filter(
    (e) => e.source_fact_key === factKey || e.target_fact_key === factKey,
  );
  showDetail.value = true;
}

async function removeEdge(id: string) {
  try {
    await deletePentFactEdge(id);
    message.success('已删除连边');
    await fetchGraph();
    showDetail.value = false;
  } catch {
    message.error('删除失败');
  }
}

watch(() => props.projectId, fetchGraph);
onMounted(fetchGraph);
</script>

<template>
  <div>
    <div style="display: flex; gap: 8px; margin-bottom: 12px; align-items: center">
      <a-button size="small" :loading="loading" @click="fetchGraph">刷新</a-button>
      <a-button size="small" @click="fitView({ padding: 0.2 })">居中</a-button>
      <a-button
        size="small"
        :type="connectMode ? 'primary' : 'default'"
        @click="connectMode ? cancelConnect() : (connectMode = true)"
      >
        {{ connectMode ? '退出连边' : '连边' }}
      </a-button>
      <span v-if="connectMode" style="font-size: 12px; color: var(--ant-color-primary)">
        {{ connectSource ? `源：${connectSource}，请选择目标节点` : '连边模式：点击源节点' }}
      </span>
      <span style="margin-left: auto; font-size: 12px; color: var(--ant-color-text-tertiary)">
        节点 {{ graph.nodes.length }} · 边 {{ graph.edges.length }}
      </span>
    </div>

    <a-empty
      v-if="!graph.nodes.length && !loading"
      description="暂无事实节点，请先在事实黑板添加事实"
      style="margin: 48px 0"
    />
    <div
      v-else
      style="height: 520px; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px"
    >
      <VueFlow v-model:nodes="nodes" v-model:edges="edges" :nodes-draggable="false" fit-view-on-init>
        <Background />
        <Controls />
      </VueFlow>
    </div>

    <!-- 连边 modal -->
    <a-modal
      v-model:open="showEdgeModal"
      title="创建连边"
      @ok="confirmEdge"
    >
      <a-form layout="vertical">
        <a-form-item label="关系">
          <a-input :value="`${connectSource} → ${connectTarget}`" disabled />
        </a-form-item>
        <a-form-item label="edge_type">
          <a-input v-model:value="edgeType" placeholder="如 contains / leads-to / related" />
        </a-form-item>
      </a-form>
    </a-modal>

    <!-- 节点详情 drawer -->
    <a-drawer v-model:open="showDetail" title="节点详情" width="440">
      <template v-if="detailNode">
        <a-descriptions :column="1" size="small" bordered>
          <a-descriptions-item label="fact_key">{{ detailNode.fact_key }}</a-descriptions-item>
          <a-descriptions-item label="分类">{{ detailNode.category }}</a-descriptions-item>
          <a-descriptions-item label="摘要">{{ detailNode.summary }}</a-descriptions-item>
          <a-descriptions-item label="置信度">{{ detailNode.confidence }}</a-descriptions-item>
        </a-descriptions>
        <a-card size="small" title="关联边" style="margin-top: 12px">
          <template v-if="detailEdges.length">
            <div
              v-for="e in detailEdges"
              :key="e.id"
              style="display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--ant-color-border-secondary); font-size: 12px"
            >
              <span style="flex: 1; word-break: break-all">
                {{ e.source_fact_key }} → {{ e.target_fact_key }}
                <a-tag size="small" style="margin-left: 4px">{{ e.edge_type }}</a-tag>
              </span>
              <a-popconfirm title="删除该连边？" @confirm="removeEdge(e.id)">
                <a-button size="small" type="link" danger>删除</a-button>
              </a-popconfirm>
            </div>
          </template>
          <a-empty v-else description="无关联边" />
        </a-card>
      </template>
    </a-drawer>
  </div>
</template>
