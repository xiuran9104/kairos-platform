<script lang="ts" setup>
import type { Edge, Node } from '@vue-flow/core';
import type {
  PentAttackGraphNode,
  PentProjectAttackGraph,
} from '#/api/pent/ops-attack-graph';

import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { MarkerType, useVueFlow, VueFlow } from '@vue-flow/core';
import { MiniMap } from '@vue-flow/minimap';

import { getPentProjectAttackGraph } from '#/api/pent/ops-attack-graph';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';
import '@vue-flow/minimap/dist/style.css';

const props = defineProps<{ projectId: string }>();

const KIND_META: Record<string, { color: string; label: string }> = {
  asset: { color: '#1677ff', label: '资产/目标' },
  context: { color: '#8c8c8c', label: '上下文' },
  credential: { color: '#722ed1', label: '凭据' },
  exploit: { color: '#fa541c', label: '利用/验证' },
  vulnerability: { color: '#f5222d', label: '漏洞' },
};
const SEVERITY_COLOR: Record<string, string> = {
  critical: '#a8071a',
  high: '#f5222d',
  info: '#8c8c8c',
  low: '#1677ff',
  medium: '#fa8c16',
};

const loading = ref(false);
const graph = ref<PentProjectAttackGraph>({
  edges: [],
  nodes: [],
  summary: { assets: 0, credentials: 0, exploits: 0, vulnerabilities: 0 },
});
const flowNodes = ref<Node[]>([]);
const flowEdges = ref<Edge[]>([]);
const selectedKind = ref<string>();
const detail = ref<null | PentAttackGraphNode>(null);
const showDetail = ref(false);

const { fitView, onNodeClick } = useVueFlow();

const visibleNodeIds = computed(() => {
  if (!selectedKind.value) return new Set(graph.value.nodes.map((node) => node.id));
  const direct = new Set(
    graph.value.nodes
      .filter((node) => node.kind === selectedKind.value)
      .map((node) => node.id),
  );
  for (const edge of graph.value.edges) {
    if (direct.has(edge.source) || direct.has(edge.target)) {
      direct.add(edge.source);
      direct.add(edge.target);
    }
  }
  return direct;
});

function nodeColor(node: PentAttackGraphNode) {
  return (
    SEVERITY_COLOR[node.severity || node.risk_level || ''] ||
    KIND_META[node.kind]?.color ||
    '#8c8c8c'
  );
}

function rebuild() {
  const ids = visibleNodeIds.value;
  const visible = graph.value.nodes.filter((node) => ids.has(node.id));
  const kindOrder = ['asset', 'context', 'vulnerability', 'exploit', 'credential'];
  const lanes = new Map<string, PentAttackGraphNode[]>();
  for (const node of visible) {
    const lane = lanes.get(node.kind) || [];
    lane.push(node);
    lanes.set(node.kind, lane);
  }
  flowNodes.value = visible.map((node) => {
    const column = Math.max(0, kindOrder.indexOf(node.kind));
    const row = (lanes.get(node.kind) || []).indexOf(node);
    const color = nodeColor(node);
    return {
      data: {
        label: `${KIND_META[node.kind]?.label || node.kind}\n${node.name || '未命名'}`,
      },
      id: node.id,
      position: { x: column * 250, y: row * 115 },
      style: {
        background: `${color}12`,
        border: `2px solid ${color}`,
        borderRadius: '9px',
        fontSize: '12px',
        maxWidth: '210px',
        padding: '10px',
        whiteSpace: 'pre-line',
        width: '210px',
      },
    } as Node;
  });
  flowEdges.value = graph.value.edges
    .filter((edge) => ids.has(edge.source) && ids.has(edge.target))
    .map((edge) => ({
      animated: edge.kind === 'exposes' || edge.kind === 'exploits',
      id: edge.id,
      label: edge.label || edge.kind,
      markerEnd: MarkerType.ArrowClosed,
      source: edge.source,
      target: edge.target,
    }));
  nextTick(() => fitView({ padding: 0.18 }));
}

async function fetchGraph() {
  loading.value = true;
  try {
    graph.value = await getPentProjectAttackGraph(props.projectId);
    rebuild();
  } catch (error: any) {
    message.error(error?.message || '加载项目攻击路径失败');
    graph.value = {
      edges: [],
      nodes: [],
      summary: { assets: 0, credentials: 0, exploits: 0, vulnerabilities: 0 },
    };
    rebuild();
  } finally {
    loading.value = false;
  }
}

function selectKind(kind?: string) {
  selectedKind.value = selectedKind.value === kind ? undefined : kind;
  rebuild();
}

onNodeClick(({ node }) => {
  detail.value = graph.value.nodes.find((item) => item.id === node.id) || null;
  showDetail.value = true;
});

watch(() => props.projectId, fetchGraph);
onMounted(fetchGraph);
</script>

<template>
  <div>
    <a-alert
      message="攻击路径来自项目资产、实际漏洞、证据和已确认攻击事实，不再使用可手工编辑的事实关系图。"
      type="info"
      show-icon
      style="margin-bottom: 12px"
    />
    <div style="display: flex; gap: 8px; margin-bottom: 12px; flex-wrap: wrap; align-items: center">
      <a-button size="small" :loading="loading" @click="fetchGraph">刷新</a-button>
      <a-button size="small" @click="fitView({ padding: 0.18 })">居中</a-button>
      <a-tag
        v-for="(meta, kind) in KIND_META"
        :key="kind"
        :color="selectedKind === kind ? meta.color : 'default'"
        style="cursor: pointer"
        @click="selectKind(kind)"
      >{{ meta.label }}</a-tag>
      <span style="margin-left: auto; font-size: 12px; color: var(--ant-color-text-tertiary)">
        {{ graph.summary.assets }} 资产 · {{ graph.summary.vulnerabilities }} 漏洞 ·
        {{ graph.summary.exploits }} 利用 · {{ graph.summary.credentials }} 凭据
      </span>
    </div>

    <div
      style="
        position: relative;
        height: 560px;
        overflow: hidden;
        border: 1px solid var(--ant-color-border-secondary);
        border-radius: 8px;
        background: var(--ant-color-bg-layout);
      "
    >
      <VueFlow
        v-model:nodes="flowNodes"
        v-model:edges="flowEdges"
        :nodes-draggable="true"
        :min-zoom="0.2"
        :max-zoom="2.5"
        fit-view-on-init
      >
        <Background :gap="18" :size="1.2" />
        <Controls />
        <MiniMap pannable zoomable />
      </VueFlow>
      <div
        v-if="!graph.nodes.length && !loading"
        style="
          position: absolute;
          z-index: 5;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          pointer-events: none;
        "
      >
        <a-empty description="暂无攻击路径；绑定项目资产并执行任务后会自动生成" />
      </div>
      <div
        v-if="loading"
        style="
          position: absolute;
          z-index: 6;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          background: color-mix(in srgb, var(--ant-color-bg-container) 55%, transparent);
        "
      >
        <a-spin tip="加载攻击路径..." />
      </div>
    </div>

    <a-drawer v-model:open="showDetail" title="攻击路径节点" width="480">
      <template v-if="detail">
        <a-descriptions :column="1" size="small" bordered>
          <a-descriptions-item label="类型">
            <a-tag :color="KIND_META[detail.kind]?.color">
              {{ KIND_META[detail.kind]?.label || detail.kind }}
            </a-tag>
          </a-descriptions-item>
          <a-descriptions-item label="名称">{{ detail.name }}</a-descriptions-item>
          <a-descriptions-item v-if="detail.target" label="目标">{{ detail.target }}</a-descriptions-item>
          <a-descriptions-item v-if="detail.severity" label="严重度">
            {{ detail.severity }}
          </a-descriptions-item>
          <a-descriptions-item v-if="detail.status" label="状态">{{ detail.status }}</a-descriptions-item>
          <a-descriptions-item v-if="detail.confidence !== undefined" label="置信度">
            {{ detail.confidence }}
          </a-descriptions-item>
          <a-descriptions-item label="关联关系">
            {{ graph.edges.filter((edge) => edge.source === detail?.id || edge.target === detail?.id).length }}
          </a-descriptions-item>
        </a-descriptions>
      </template>
    </a-drawer>
  </div>
</template>
