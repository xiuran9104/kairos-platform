<script lang="ts" setup>
import type { Connection, Edge, Node } from '@vue-flow/core';

import type {
  PentWorkflow,
  PentWorkflowGraph,
  PentWorkflowTraceStep,
} from '#/api/pent/ops-workflows';

import { computed, nextTick, onMounted, ref } from 'vue';
import { message, Modal } from 'ant-design-vue';

import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { MarkerType, useVueFlow, VueFlow } from '@vue-flow/core';

import {
  createPentWorkflow,
  deletePentWorkflow,
  dryRunPentWorkflow,
  getPentWorkflows,
  updatePentWorkflow,
  validatePentWorkflow,
} from '#/api/pent/ops-workflows';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';

// ── 常量（对齐原版 workflows.js） ──────────────────────────────────────────
const NODE_TYPE_META: Record<string, { color: string; label: string }> = {
  start: { label: '开始', color: '#52c41a' },
  tool: { label: '工具', color: '#1677ff' },
  agent: { label: 'Agent', color: '#722ed1' },
  condition: { label: '条件', color: '#fa8c16' },
  hitl: { label: '审批', color: '#eb2f96' },
  output: { label: '输出', color: '#13c2c2' },
  end: { label: '结束', color: '#8c8c8c' },
};
const NODE_TYPES = Object.keys(NODE_TYPE_META);
const nodeLibrary = NODE_TYPES.map((type) => ({ type, ...NODE_TYPE_META[type]! }));

const AGENT_MODES = ['eino_single', 'deep', 'plan_execute', 'supervisor'];
const JOIN_STRATEGIES = ['all_merge', 'last_by_canvas', 'first_non_empty', 'fail_fast'];
const HITL_REVIEWERS = ['human', 'audit_agent'];
const BINDING_FROM_OPTIONS = ['previous', 'inputs', 'outputs'];
const CONDITION_EXAMPLES = [
  '{{previous.output}} != ""',
  '{{outputs.risk_score}} >= 8',
  '{{previous.output}} contains "success"',
  '{{previous.output}} matches "^ok"',
];

const ID_RE = /^[a-z0-9][a-z0-9-]*$/;

function defaultConfigForType(type: string): Record<string, any> {
  switch (type) {
    case 'agent': {
      return {
        agent_mode: 'eino_single',
        input_binding: { from: 'previous', field: 'output' },
        instruction: '',
        output_key: 'agent_result',
        join_strategy: 'all_merge',
      };
    }
    case 'condition': {
      return { expression: '{{previous.output}} != ""', join_strategy: 'all_merge' };
    }
    case 'end': {
      return {
        result_binding: { from: 'outputs', field: 'result' },
        join_strategy: 'all_merge',
      };
    }
    case 'hitl': {
      return {
        prompt: '请审批该步骤是否继续执行',
        prompt_binding: { from: 'previous', field: 'output' },
        reviewer: 'human',
        join_strategy: 'all_merge',
      };
    }
    case 'output': {
      return {
        output_key: 'result',
        source_binding: { from: 'previous', field: 'output' },
        join_strategy: 'all_merge',
      };
    }
    case 'start': {
      return { input_keys: 'message, conversationId, projectId' };
    }
    case 'tool': {
      return { tool_name: '', arguments: '{}', timeout_seconds: '', join_strategy: 'all_merge' };
    }
    default: {
      return {};
    }
  }
}

function typedKeysForElement(kind: 'edge' | 'node', type: string): Set<string> {
  if (kind === 'edge') return new Set(['condition', 'branch']);
  return new Set(Object.keys(defaultConfigForType(type)));
}

// ── 工作流列表 ─────────────────────────────────────────────────────────────
const listLoading = ref(false);
const workflows = ref<PentWorkflow[]>([]);
const currentId = ref<string>('');

// 元信息表单（对应原版 workflow-meta-modal 字段，画布标题同步读取）
const metaForm = ref({ id: '', name: '', description: '', enabled: true });

const currentWorkflow = computed(() =>
  workflows.value.find((w) => w.id === currentId.value),
);
const canvasTitle = computed(() => metaForm.value.name || metaForm.value.id || '未命名工作流');
const canvasSubtitle = computed(() => {
  const parts: string[] = [];
  if (metaForm.value.id) parts.push(metaForm.value.id);
  if (currentWorkflow.value?.version) parts.push(`v${currentWorkflow.value.version}`);
  parts.push(metaForm.value.enabled ? '启用' : '禁用');
  return parts.join(' · ');
});

async function fetchWorkflows() {
  listLoading.value = true;
  try {
    workflows.value = (await getPentWorkflows(true)) || [];
    if (currentId.value) {
      const wf = workflows.value.find((w) => w.id === currentId.value);
      if (wf) metaForm.value.enabled = wf.enabled !== false;
    } else if (workflows.value.length) {
      loadWorkflow(workflows.value[0]!.id);
    }
  } catch {
    message.error('加载工作流失败');
  } finally {
    listLoading.value = false;
  }
}

async function toggleEnabled(wf: PentWorkflow, enabled: boolean) {
  const previous = wf.enabled !== false;
  wf.enabled = enabled;
  if (wf.id === currentId.value) metaForm.value.enabled = enabled;
  try {
    await updatePentWorkflow(wf.id, { enabled });
    message.success('启用状态已更新');
  } catch {
    wf.enabled = previous;
    if (wf.id === currentId.value) metaForm.value.enabled = previous;
    message.error('更新启用状态失败');
  }
}

function loadWorkflow(id: string) {
  const wf = workflows.value.find((w) => w.id === id);
  if (!wf) return;
  currentId.value = id;
  metaForm.value = {
    id: wf.id,
    name: wf.name || '',
    description: wf.description || '',
    enabled: wf.enabled !== false,
  };
  const graph = wf.graph || { nodes: [], edges: [] };
  resetSequences(graph);
  nodes.value = (graph.nodes || []).map(toFlowNode);
  edges.value = (graph.edges || []).map(toFlowEdge);
  clearSelection();
  closeDryRunPanel();
  nextTick(() => fitView({ padding: 0.2 }));
}

// ── 画布 ───────────────────────────────────────────────────────────────────
const nodes = ref<Node[]>([]);
const edges = ref<Edge[]>([]);
const connectMode = ref(false);
let connectSourceId = '';
let nodeSeq = 1;
let edgeSeq = 1;

const {
  addEdges,
  fitView,
  onConnect,
  onEdgeClick,
  onNodeClick,
  onPaneClick,
  project,
  removeEdges,
  removeNodes,
} = useVueFlow();

function nextNodeId() {
  while (nodes.value.some((n) => n.id === `node-${nodeSeq}`)) nodeSeq += 1;
  return `node-${nodeSeq++}`;
}

function nextEdgeId() {
  while (edges.value.some((e) => e.id === `edge-${edgeSeq}`)) edgeSeq += 1;
  return `edge-${edgeSeq++}`;
}

function resetSequences(graph: PentWorkflowGraph) {
  nodeSeq = 1;
  edgeSeq = 1;
  for (const n of graph.nodes || []) {
    const m = /^node-(\d+)$/.exec(String(n.id || ''));
    if (m) nodeSeq = Math.max(nodeSeq, Number(m[1]) + 1);
  }
  for (const e of graph.edges || []) {
    const m = /^edge-(\d+)$/.exec(String(e.id || ''));
    if (m) edgeSeq = Math.max(edgeSeq, Number(m[1]) + 1);
  }
}

function toFlowNode(n: PentWorkflowGraph['nodes'][number]): Node {
  const meta = NODE_TYPE_META[n.type] || NODE_TYPE_META.tool!;
  return {
    id: n.id,
    type: 'default',
    position: n.position || { x: 0, y: 0 },
    data: {
      label: n.label || meta.label || n.id,
      wfType: n.type || 'tool',
      config: { ...defaultConfigForType(n.type || 'tool'), ...(n.config || {}) },
    },
    style: {
      border: `2px solid ${meta.color}`,
      borderRadius: '8px',
      fontSize: '12px',
      width: '150px',
    },
  };
}

function edgeLabelForBranch(branch: string): string {
  if (branch === 'true') return '是';
  if (branch === 'false') return '否';
  return branch;
}

function toFlowEdge(e: PentWorkflowGraph['edges'][number]): Edge {
  const branch = e.config?.branch ?? e.branch ?? '';
  return {
    id: e.id || `${e.source}->${e.target}`,
    source: e.source,
    target: e.target,
    label: edgeLabelForBranch(branch) || undefined,
    data: { branch, condition: (e.config as Record<string, any> | undefined)?.condition || '' },
    markerEnd: MarkerType.ArrowClosed,
  };
}

function currentGraph(): PentWorkflowGraph {
  return {
    nodes: nodes.value.map((n) => ({
      id: n.id,
      type: (n.data?.wfType as string) || 'tool',
      label: (n.data?.label as string) || n.id,
      position: { x: n.position.x, y: n.position.y },
      config: { ...((n.data?.config as Record<string, any>) || {}) },
    })),
    edges: edges.value.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
      branch: (e.data?.branch as string) || undefined,
      config: {
        branch: (e.data?.branch as string) || undefined,
        condition: (e.data?.condition as string) || undefined,
      } as PentWorkflowGraph['edges'][number]['config'],
    })),
  };
}

// 节点库：点击添加 / 拖拽添加（对齐原版 palette 行为）
function addNodeOfType(type: string, position?: { x: number; y: number }) {
  const meta = NODE_TYPE_META[type] || NODE_TYPE_META.tool!;
  const node = toFlowNode({
    id: nextNodeId(),
    type,
    label: meta.label,
    position: position || {
      x: 160 + Math.random() * 240,
      y: 100 + Math.random() * 160,
    },
    config: defaultConfigForType(type),
  });
  nodes.value = [...nodes.value, node];
  selectElement(node.id, 'node');
}

function onPaletteDragStart(event: DragEvent, type: string) {
  event.dataTransfer?.setData('application/x-workflow-node', type);
  event.dataTransfer?.setData('text/plain', type);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
}

const canvasWrapRef = ref<HTMLElement | null>(null);

function onCanvasDrop(event: DragEvent) {
  const type =
    event.dataTransfer?.getData('application/x-workflow-node') ||
    event.dataTransfer?.getData('text/plain') ||
    'tool';
  if (!NODE_TYPE_META[type]) return;
  const rect = canvasWrapRef.value?.getBoundingClientRect();
  const point = rect
    ? project({ x: event.clientX - rect.left, y: event.clientY - rect.top })
    : { x: 200, y: 140 };
  addNodeOfType(type, point);
}

// 连线模式：依次点击源节点与目标节点（对齐原版 connect 模式）
function toggleConnectMode() {
  connectMode.value = !connectMode.value;
  connectSourceId = '';
  message.info(connectMode.value ? '连线模式：依次点击源节点和目标节点' : '已退出连线模式');
}

function createEdge(sourceId: string, targetId: string) {
  if (edges.value.some((e) => e.source === sourceId && e.target === targetId)) {
    message.warning('这两个节点之间已经有连线');
    return;
  }
  const sourceNode = nodes.value.find((n) => n.id === sourceId);
  const sourceType = (sourceNode?.data?.wfType as string) || '';
  let label = '';
  let branch = '';
  if (sourceType === 'condition') {
    const siblingCount = edges.value.filter((e) => e.source === sourceId).length;
    if (siblingCount === 0) {
      label = '是';
      branch = 'true';
    } else if (siblingCount === 1) {
      label = '否';
      branch = 'false';
    }
  }
  addEdges([
    {
      id: nextEdgeId(),
      source: sourceId,
      target: targetId,
      label: label || undefined,
      data: { branch, condition: '' },
      markerEnd: MarkerType.ArrowClosed,
    },
  ]);
}

onConnect((conn: Connection) => {
  if (!conn.source || !conn.target) return;
  createEdge(conn.source, conn.target);
});

// ── 选中与属性面板 ─────────────────────────────────────────────────────────
const selectedId = ref<string>('');
const selectedKind = ref<'edge' | 'node' | ''>('');

function clearSelection() {
  selectedId.value = '';
  selectedKind.value = '';
}

function selectElement(id: string, kind: 'edge' | 'node') {
  selectedId.value = id;
  selectedKind.value = kind;
}

onNodeClick(({ node }) => {
  if (connectMode.value) {
    if (!connectSourceId) {
      connectSourceId = node.id;
      return;
    }
    if (connectSourceId !== node.id) createEdge(connectSourceId, node.id);
    connectSourceId = '';
    return;
  }
  selectElement(node.id, 'node');
});

onEdgeClick(({ edge }) => {
  if (connectMode.value) return;
  selectElement(edge.id, 'edge');
});

onPaneClick(() => {
  connectSourceId = '';
  clearSelection();
});

const selectedNode = computed(() =>
  selectedKind.value === 'node'
    ? nodes.value.find((n) => n.id === selectedId.value) || null
    : null,
);
const selectedEdge = computed(() =>
  selectedKind.value === 'edge'
    ? edges.value.find((e) => e.id === selectedId.value) || null
    : null,
);
const selectedType = computed(() => (selectedNode.value?.data?.wfType as string) || '');
const selectedEdgeSourceType = computed(() => {
  const edge = selectedEdge.value;
  if (!edge) return '';
  return (nodes.value.find((n) => n.id === edge.source)?.data?.wfType as string) || '';
});

const propertyTitle = computed(() => {
  if (selectedKind.value === 'node') return '节点属性';
  if (selectedKind.value === 'edge') return '连线属性';
  return '属性';
});

function nodeConfig(): Record<string, any> {
  const n = selectedNode.value;
  if (!n) return {};
  if (!n.data.config) n.data.config = {};
  return n.data.config;
}

function edgeConfig(): Record<string, any> {
  const e = selectedEdge.value;
  if (!e) return {};
  if (!e.data) e.data = {};
  return e.data;
}

function setEdgeLabel(v: any) {
  const e = selectedEdge.value;
  if (e) e.label = v || undefined;
}

function bindingFor(key: string): { field: string; from: string } {
  const b = nodeConfig()[key];
  if (b && typeof b === 'object') {
    return { from: b.from || 'previous', field: b.field || 'output' };
  }
  return { from: 'previous', field: 'output' };
}

function setBinding(key: string, part: 'field' | 'from', value: string) {
  const cfg = nodeConfig();
  const b = bindingFor(key);
  b[part] = value;
  cfg[key] = { ...b };
}

function onSelectedTypeChange(type: string) {
  const n = selectedNode.value;
  if (!n) return;
  const prev = (n.data.wfType as string) || 'tool';
  if (type === prev) return;
  n.data.wfType = type;
  n.data.config = defaultConfigForType(type);
  n.data.label = NODE_TYPE_META[type]?.label || n.data.label;
  n.style = {
    ...(typeof n.style === 'object' ? n.style : {}),
    border: `2px solid ${NODE_TYPE_META[type]?.color || '#1677ff'}`,
  };
}

function deleteSelection() {
  if (selectedKind.value === 'node' && selectedId.value) {
    removeNodes([selectedId.value]);
  } else if (selectedKind.value === 'edge' && selectedId.value) {
    removeEdges([selectedId.value]);
  }
  clearSelection();
}

// 自定义字段：config 中不属于当前类型已知字段的键
const customFields = computed(() => {
  if (!selectedNode.value && !selectedEdge.value) return [];
  const cfg =
    selectedKind.value === 'node'
      ? { ...nodeConfig() }
      : { condition: undefined, branch: undefined, ...edgeConfig() };
  const typed = typedKeysForElement(
    selectedKind.value === 'edge' ? 'edge' : 'node',
    selectedType.value,
  );
  return Object.entries(cfg)
    .filter(([key, value]) => key && !typed.has(key) && value !== undefined)
    .map(([key, value]) => ({
      key,
      value: typeof value === 'object' ? JSON.stringify(value) : String(value ?? ''),
    }));
});

function setCustomField(oldKey: string, key: string, value: string) {
  const cfg = selectedKind.value === 'node' ? nodeConfig() : edgeConfig();
  if (oldKey && oldKey !== key) delete cfg[oldKey];
  if (key) cfg[key] = value;
}

function addCustomField() {
  const cfg = selectedKind.value === 'node' ? nodeConfig() : edgeConfig();
  let i = 1;
  while (`field_${i}` in cfg) i += 1;
  cfg[`field_${i}`] = '';
}

function removeCustomField(key: string) {
  const cfg = selectedKind.value === 'node' ? nodeConfig() : edgeConfig();
  delete cfg[key];
}

const argumentsText = computed({
  get() {
    const cfg = selectedNode.value?.data?.config;
    if (!cfg || cfg.arguments === undefined) return '';
    return typeof cfg.arguments === 'string'
      ? cfg.arguments
      : JSON.stringify(cfg.arguments, null, 2);
  },
  set(v: string) {
    nodeConfig().arguments = v;
  },
});

// ── 校验（对齐原版 validateWorkflowGraph） ─────────────────────────────────
function validateGraph(graph: PentWorkflowGraph): string[] {
  const errors: string[] = [];
  const nodeList = graph.nodes || [];
  const edgeList = graph.edges || [];
  const ids = new Set(nodeList.map((n) => n.id));
  const starts = nodeList.filter((n) => n.type === 'start');
  const outputs = nodeList.filter((n) => n.type === 'output');
  const terminals = nodeList.filter((n) => n.type === 'output' || n.type === 'end');
  if (!starts.length) errors.push('至少需要一个开始节点');
  if (!outputs.length) errors.push('至少需要一个输出节点');
  for (const e of edgeList) {
    if (e.source === e.target) errors.push(`连线 ${e.id} 不能指向自身`);
    if (!ids.has(e.source)) errors.push(`连线 ${e.id} 的源节点不存在`);
    if (!ids.has(e.target)) errors.push(`连线 ${e.id} 的目标节点不存在`);
  }
  for (const n of starts) {
    if (edgeList.some((e) => e.target === n.id))
      errors.push(`开始节点 ${n.label || n.id} 不应有入边`);
  }
  for (const n of terminals) {
    if (edgeList.some((e) => e.source === n.id))
      errors.push(`输出节点 ${n.label || n.id} 不应有出边`);
  }
  for (const n of nodeList.filter((x) => x.type !== 'start')) {
    if (!edgeList.some((e) => e.target === n.id))
      errors.push(`节点 ${n.label || n.id} 需要入边`);
  }
  for (const n of nodeList.filter((x) => x.type !== 'output' && x.type !== 'end')) {
    if (!edgeList.some((e) => e.source === n.id))
      errors.push(`节点 ${n.label || n.id} 需要出边`);
  }
  for (const n of nodeList.filter((x) => x.type === 'tool')) {
    if (!String((n.config || {}).tool_name || '').trim())
      errors.push(`工具节点 ${n.label || n.id} 需要选择 MCP 工具`);
  }
  for (const n of nodeList.filter((x) => x.type === 'condition')) {
    if (!String((n.config || {}).expression || '').trim())
      errors.push(`条件节点 ${n.label || n.id} 需要条件表达式`);
    const outEdges = edgeList.filter((e) => e.source === n.id);
    if (outEdges.length === 0) {
      errors.push(`条件节点 ${n.label || n.id} 至少需要一条出边（是/否分支）`);
    } else if (outEdges.length > 2) {
      errors.push(`条件节点 ${n.label || n.id} 建议最多两条出边（是/否）；第三条及以后需配置连线条件`);
    }
    const branches = outEdges.map((e) =>
      String(e.config?.branch || e.branch || '').trim().toLowerCase(),
    );
    if (branches.some((b) => !['true', 'false', '是', '否', 'yes', 'no', 'y', 'n'].includes(b)))
      errors.push(`条件节点 ${n.label || n.id} 的出边必须标记为是/否`);
    if (new Set(branches).size !== branches.length)
      errors.push(`条件节点 ${n.label || n.id} 存在重复分支`);
  }
  for (const n of nodeList.filter((x) => x.type === 'output')) {
    if (!String((n.config || {}).output_key || '').trim())
      errors.push(`输出节点 ${n.label || n.id} 需要输出变量名`);
  }
  if (terminals.length) {
    const outgoing = new Map<string, string[]>();
    for (const e of edgeList) {
      if (!outgoing.has(e.source)) outgoing.set(e.source, []);
      outgoing.get(e.source)!.push(e.target);
    }
    const reached = new Set<string>();
    const queue = starts.map((n) => n.id);
    while (queue.length) {
      const id = queue.shift()!;
      if (reached.has(id)) continue;
      reached.add(id);
      for (const next of outgoing.get(id) || []) queue.push(next);
    }
    for (const n of nodeList) {
      if (!reached.has(n.id)) errors.push(`节点 ${n.label || n.id} 无法从开始节点到达`);
    }
    const visiting = new Set<string>();
    const visited = new Set<string>();
    const visit = (id: string): boolean => {
      if (visiting.has(id)) return true;
      if (visited.has(id)) return false;
      visiting.add(id);
      for (const next of outgoing.get(id) || []) {
        if (visit(next)) return true;
      }
      visiting.delete(id);
      visited.add(id);
      return false;
    };
    for (const n of nodeList) {
      if (visit(n.id)) errors.push(`工作流存在环路（节点 ${n.label || n.id}），请保持 DAG`);
    }
  }
  return [...new Set(errors)];
}

const validating = ref(false);

async function doValidate(quiet = false): Promise<string[]> {
  const graph = currentGraph();
  const localErrors = validateGraph(graph);
  if (localErrors.length) {
    if (!quiet) showErrors('校验未通过', localErrors);
    return localErrors;
  }
  validating.value = true;
  try {
    const res = await validatePentWorkflow(graph);
    if (res?.valid) {
      if (!quiet) message.success('校验通过');
      return [];
    }
    if (!quiet) showErrors('校验未通过', res?.errors || []);
    return res?.errors || ['后端图校验失败'];
  } catch {
    if (!quiet) message.error('校验请求失败');
    return ['校验请求失败'];
  } finally {
    validating.value = false;
  }
}

function showErrors(title: string, errors: string[]) {
  Modal.warning({ title, content: errors.join('\n') });
}

// ── 保存 / 删除工作流 ──────────────────────────────────────────────────────
const saving = ref(false);

async function doSave() {
  const meta = metaForm.value;
  if (!meta.id || !meta.name) {
    message.error('工作流 ID 和名称不能为空');
    openMetaModal();
    return;
  }
  saving.value = true;
  try {
    const errors = await doValidate(true);
    if (errors.length) {
      showErrors('校验未通过', errors.slice(0, 8));
      return;
    }
    const payload = {
      id: meta.id,
      name: meta.name,
      description: meta.description,
      enabled: meta.enabled,
      graph: currentGraph(),
    };
    if (currentId.value) {
      await updatePentWorkflow(currentId.value, payload);
    } else {
      await createPentWorkflow(payload);
      currentId.value = meta.id;
    }
    message.success('工作流已保存');
    showMeta.value = false;
    await fetchWorkflows();
  } catch {
    message.error('保存工作流失败');
  } finally {
    saving.value = false;
  }
}

function doDeleteCurrent() {
  const id = currentId.value || metaForm.value.id;
  if (!id) {
    message.warning('请选择要删除的工作流');
    return;
  }
  Modal.confirm({
    title: `确定删除工作流 ${id}？`,
    okText: '删除',
    okType: 'danger',
    cancelText: '取消',
    async onOk() {
      try {
        await deletePentWorkflow(id);
        message.success('工作流已删除');
        currentId.value = '';
        newWorkflowDraft(false);
        await fetchWorkflows();
      } catch {
        message.error('删除工作流失败');
      }
    },
  });
}

// ── 元信息弹窗（对应原版 workflow-meta-modal） ─────────────────────────────
const showMeta = ref(false);

function openMetaModal() {
  showMeta.value = true;
}

function applyMetaModal() {
  if (!metaForm.value.id.trim() || !metaForm.value.name.trim()) {
    message.error('工作流 ID 和名称不能为空');
    return;
  }
  if (!ID_RE.test(metaForm.value.id)) {
    message.error('ID 只能包含小写字母、数字与连字符，且需以字母或数字开头');
    return;
  }
  showMeta.value = false;
}

function newWorkflowDraft(openMeta = true) {
  currentId.value = '';
  metaForm.value = { id: '', name: '', description: '', enabled: true };
  nodes.value = [];
  edges.value = [];
  clearSelection();
  closeDryRunPanel();
  if (openMeta) openMetaModal();
}

function editWorkflowFromList(id: string) {
  if (id !== currentId.value) loadWorkflow(id);
  openMetaModal();
}

// ── 试运行 ─────────────────────────────────────────────────────────────────
const showDryRun = ref(false);
const dryRunMessage = ref('ping');
const dryRunLoading = ref(false);
const dryRunPanelVisible = ref(false);
const dryRunTrace = ref<PentWorkflowTraceStep[]>([]);

function openDryRun() {
  const errors = validateGraph(currentGraph());
  if (errors.length) {
    showErrors('校验未通过', errors.slice(0, 8));
    return;
  }
  dryRunMessage.value = 'ping';
  showDryRun.value = true;
}

function closeDryRunPanel() {
  dryRunPanelVisible.value = false;
  dryRunTrace.value = [];
}

async function doDryRun() {
  const msg = dryRunMessage.value.trim() || 'ping';
  showDryRun.value = false;
  dryRunLoading.value = true;
  try {
    const res = await dryRunPentWorkflow(currentGraph(), { message: msg });
    dryRunTrace.value = res?.trace || [];
    dryRunPanelVisible.value = true;
    message.success('试运行完成');
  } catch {
    message.error('试运行失败');
  } finally {
    dryRunLoading.value = false;
  }
}

// ── 自动布局：BFS 分层 ─────────────────────────────────────────────────────
function autoLayout() {
  if (!nodes.value.length) return;
  const incoming = new Map<string, number>();
  const adj = new Map<string, string[]>();
  for (const n of nodes.value) {
    incoming.set(n.id, 0);
    adj.set(n.id, []);
  }
  for (const e of edges.value) {
    incoming.set(e.target, (incoming.get(e.target) || 0) + 1);
    adj.get(e.source)?.push(e.target);
  }
  const layer = new Map<string, number>();
  const queue: string[] = [];
  for (const n of nodes.value) {
    if ((incoming.get(n.id) || 0) === 0 || (n.data?.wfType as string) === 'start') {
      layer.set(n.id, 0);
      queue.push(n.id);
    }
  }
  if (!queue.length && nodes.value[0]) {
    layer.set(nodes.value[0].id, 0);
    queue.push(nodes.value[0].id);
  }
  while (queue.length) {
    const id = queue.shift()!;
    const lv = layer.get(id) || 0;
    for (const next of adj.get(id) || []) {
      if ((layer.get(next) ?? -1) < lv + 1) {
        layer.set(next, lv + 1);
        queue.push(next);
      }
    }
  }
  const byLayer = new Map<number, Node[]>();
  for (const n of nodes.value) {
    const lv = layer.get(n.id) ?? 0;
    if (!byLayer.has(lv)) byLayer.set(lv, []);
    byLayer.get(lv)!.push(n);
  }
  for (const [lv, group] of byLayer) {
    group.forEach((n, i) => {
      n.position = { x: 80 + lv * 220, y: 60 + i * 100 };
    });
  }
  nextTick(() => fitView({ padding: 0.2 }));
}

onMounted(() => fetchWorkflows());
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <!-- 页面头部：标题 + 刷新 / 新建工作流 -->
    <a-page-header title="工作流">
      <template #extra>
        <a-button :loading="listLoading" @click="fetchWorkflows()">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="newWorkflowDraft()">
          新建工作流
        </a-button>
      </template>
    </a-page-header>

    <a-row :gutter="16" style="margin-top: 16px">
      <!-- 左栏：工作流列表 + 节点库 -->
      <a-col :xs="24" :lg="5">
        <a-card size="small" title="工作流列表" :loading="listLoading">
          <div style="display: flex; flex-direction: column; gap: 8px">
            <div
              v-for="wf in workflows"
              :key="wf.id"
              :style="{
                padding: '8px',
                borderRadius: '6px',
                cursor: 'pointer',
                border: `1px solid ${wf.id === currentId ? 'var(--ant-color-primary)' : 'var(--ant-color-border-secondary)'}`,
              }"
              @click="loadWorkflow(wf.id)"
            >
              <div style="display: flex; justify-content: space-between; align-items: center">
                <span style="font-size: 13px; font-weight: 600">{{ wf.name || wf.id }}</span>
                <span style="display: flex; align-items: center; gap: 4px">
                  <a-switch
                    :checked="wf.enabled"
                    size="small"
                    @click.stop
                    @change="(v: any) => toggleEnabled(wf, !!v)"
                  />
                  <a-button
                    size="small"
                    type="text"
                    title="编辑"
                    @click.stop="editWorkflowFromList(wf.id)"
                  >
                    编辑
                  </a-button>
                </span>
              </div>
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
                {{ wf.id }} · v{{ wf.version || 1 }}
              </div>
            </div>
            <a-empty v-if="!workflows.length && !listLoading" description="暂无工作流" />
          </div>
        </a-card>

        <a-card size="small" title="节点库" style="margin-top: 16px">
          <div
            style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px"
          >
            <a-button
              v-for="n in nodeLibrary"
              :key="n.type"
              size="small"
              draggable="true"
              @dragstart="onPaletteDragStart($event, n.type)"
              @click="addNodeOfType(n.type)"
            >
              {{ n.label }}
            </a-button>
          </div>
        </a-card>
      </a-col>

      <!-- 中栏：画布标题 + 工具栏 + 画布 -->
      <a-col :xs="24" :lg="14">
        <a-card size="small">
          <div
            style="display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px; margin-bottom: 12px"
          >
            <div>
              <span style="font-size: 14px; font-weight: 600">{{ canvasTitle }}</span>
              <span style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-left: 8px">
                {{ canvasSubtitle }}
              </span>
            </div>
            <div style="display: flex; flex-wrap: wrap; gap: 8px">
              <a-button
                size="small"
                :type="connectMode ? 'primary' : 'default'"
                @click="toggleConnectMode"
              >
                {{ connectMode ? '连线中' : '连线' }}
              </a-button>
              <a-button size="small" @click="autoLayout">自动布局</a-button>
              <a-button size="small" :loading="dryRunLoading" @click="openDryRun">试运行</a-button>
              <a-dropdown>
                <a-button size="small">更多</a-button>
                <template #overlay>
                  <a-menu>
                    <a-menu-item key="validate" :disabled="validating" @click="doValidate()">
                      校验
                    </a-menu-item>
                    <a-menu-item key="delete" danger @click="doDeleteCurrent">
                      删除工作流
                    </a-menu-item>
                  </a-menu>
                </template>
              </a-dropdown>
              <a-button size="small" type="primary" :loading="saving" @click="doSave">保存</a-button>
            </div>
          </div>
          <div
            ref="canvasWrapRef"
            style="position: relative; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; height: 560px"
            @dragover.prevent
            @drop.prevent="onCanvasDrop"
          >
            <VueFlow
              v-model:nodes="nodes"
              v-model:edges="edges"
              :delete-key-code="['Backspace', 'Delete']"
              fit-view-on-init
              :min-zoom="0.2"
              :max-zoom="2"
            >
              <Background :gap="16" />
              <Controls />
            </VueFlow>
            <div
              v-if="!nodes.length"
              style="position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; pointer-events: none; color: var(--ant-color-text-tertiary); font-size: 13px"
            >
              从左侧拖拽节点到画布，或点击节点按钮快速添加
            </div>
          </div>
        </a-card>
      </a-col>

      <!-- 右栏：属性面板 + 试运行轨迹 -->
      <a-col :xs="24" :lg="5">
        <a-card size="small">
          <template #title>{{ propertyTitle }}</template>
          <template v-if="selectedNode || selectedEdge" #extra>
            <a-button size="small" danger type="link" @click="deleteSelection">
              {{ selectedKind === 'node' ? '删除节点' : '删除连线' }}
            </a-button>
          </template>

          <a-form v-if="selectedNode" layout="vertical">
            <a-form-item label="名称">
              <a-input v-model:value="selectedNode.data.label" placeholder="节点名称" />
            </a-form-item>
            <a-form-item label="类型">
              <a-select
                :value="selectedType"
                :options="NODE_TYPES.map((t) => ({ value: t, label: NODE_TYPE_META[t]?.label || t }))"
                @update:value="(v: any) => onSelectedTypeChange(v)"
              />
            </a-form-item>

            <!-- start -->
            <a-form-item v-if="selectedType === 'start'" label="输入变量">
              <a-input
                :value="nodeConfig().input_keys"
                placeholder="message, projectId"
                @update:value="(v: any) => (nodeConfig().input_keys = v)"
              />
            </a-form-item>

            <!-- tool -->
            <template v-else-if="selectedType === 'tool'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="MCP 工具">
                <a-input
                  :value="nodeConfig().tool_name"
                  placeholder="请选择工具"
                  @update:value="(v: any) => (nodeConfig().tool_name = v)"
                />
              </a-form-item>
              <a-form-item label="工具参数（JSON）">
                <a-textarea v-model:value="argumentsText" :rows="4" placeholder='{"target":"example.com"}' />
              </a-form-item>
              <a-form-item label="超时秒数">
                <a-input
                  :value="nodeConfig().timeout_seconds"
                  placeholder="可选"
                  @update:value="(v: any) => (nodeConfig().timeout_seconds = v)"
                />
              </a-form-item>
            </template>

            <!-- agent -->
            <template v-else-if="selectedType === 'agent'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="Agent 模式">
                <a-select
                  :value="nodeConfig().agent_mode"
                  :options="AGENT_MODES.map((m) => ({ value: m, label: m }))"
                  @update:value="(v: any) => (nodeConfig().agent_mode = v)"
                />
              </a-form-item>
              <a-form-item label="输入字段绑定">
                <div style="display: flex; gap: 8px">
                  <a-select
                    :value="bindingFor('input_binding').from"
                    :options="BINDING_FROM_OPTIONS.map((o) => ({ value: o, label: o }))"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('input_binding', 'from', v)"
                  />
                  <a-input
                    :value="bindingFor('input_binding').field"
                    placeholder="output"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('input_binding', 'field', v)"
                  />
                </div>
              </a-form-item>
              <a-form-item label="节点指令">
                <a-textarea
                  :value="nodeConfig().instruction"
                  :rows="4"
                  placeholder="描述该节点要完成的任务"
                  @update:value="(v: any) => (nodeConfig().instruction = v)"
                />
              </a-form-item>
              <a-form-item label="输出变量名">
                <a-input
                  :value="nodeConfig().output_key"
                  placeholder="agent_result"
                  @update:value="(v: any) => (nodeConfig().output_key = v)"
                />
              </a-form-item>
            </template>

            <!-- condition -->
            <template v-else-if="selectedType === 'condition'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="条件表达式">
                <a-input
                  :value="nodeConfig().expression"
                  placeholder='{{previous.output}} != ""'
                  @update:value="(v: any) => (nodeConfig().expression = v)"
                />
              </a-form-item>
              <a-alert type="info" show-icon style="margin-bottom: 8px">
                <template #message>
                  <div style="font-size: 12px">
                    <div><strong>可用语法</strong></div>
                    <div>变量：previous.output、previous.matched、inputs.message、outputs.变量名</div>
                    <div>操作符：==、!=、&gt;、&gt;=、&lt;、&lt;=、contains、matches、&amp;&amp;、||</div>
                    <div style="margin-top: 4px; display: flex; flex-wrap: wrap; gap: 4px">
                      <a-button
                        v-for="expr in CONDITION_EXAMPLES"
                        :key="expr"
                        size="small"
                        @click="nodeConfig().expression = expr"
                      >
                        {{ expr }}
                      </a-button>
                    </div>
                  </div>
                </template>
              </a-alert>
            </template>

            <!-- hitl -->
            <template v-else-if="selectedType === 'hitl'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="审批提示">
                <a-textarea
                  :value="nodeConfig().prompt"
                  :rows="3"
                  placeholder="请审批是否继续"
                  @update:value="(v: any) => (nodeConfig().prompt = v)"
                />
              </a-form-item>
              <a-form-item label="提示字段绑定">
                <div style="display: flex; gap: 8px">
                  <a-select
                    :value="bindingFor('prompt_binding').from"
                    :options="BINDING_FROM_OPTIONS.map((o) => ({ value: o, label: o }))"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('prompt_binding', 'from', v)"
                  />
                  <a-input
                    :value="bindingFor('prompt_binding').field"
                    placeholder="output"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('prompt_binding', 'field', v)"
                  />
                </div>
              </a-form-item>
              <a-form-item label="审批方">
                <a-select
                  :value="nodeConfig().reviewer"
                  :options="HITL_REVIEWERS.map((r) => ({ value: r, label: r }))"
                  @update:value="(v: any) => (nodeConfig().reviewer = v)"
                />
              </a-form-item>
            </template>

            <!-- output -->
            <template v-else-if="selectedType === 'output'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="输出变量名">
                <a-input
                  :value="nodeConfig().output_key"
                  placeholder="result"
                  @update:value="(v: any) => (nodeConfig().output_key = v)"
                />
              </a-form-item>
              <a-form-item label="输出字段绑定">
                <div style="display: flex; gap: 8px">
                  <a-select
                    :value="bindingFor('source_binding').from"
                    :options="BINDING_FROM_OPTIONS.map((o) => ({ value: o, label: o }))"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('source_binding', 'from', v)"
                  />
                  <a-input
                    :value="bindingFor('source_binding').field"
                    placeholder="output"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('source_binding', 'field', v)"
                  />
                </div>
              </a-form-item>
              <a-form-item label="固定输出值">
                <a-input
                  :value="nodeConfig().static_value"
                  placeholder="可选"
                  @update:value="(v: any) => (nodeConfig().static_value = v)"
                />
              </a-form-item>
            </template>

            <!-- end -->
            <template v-else-if="selectedType === 'end'">
              <a-form-item label="汇聚策略">
                <a-select
                  :value="nodeConfig().join_strategy"
                  :options="JOIN_STRATEGIES.map((s) => ({ value: s, label: s }))"
                  @update:value="(v: any) => (nodeConfig().join_strategy = v)"
                />
              </a-form-item>
              <a-form-item label="结束摘要绑定">
                <div style="display: flex; gap: 8px">
                  <a-select
                    :value="bindingFor('result_binding').from"
                    :options="BINDING_FROM_OPTIONS.map((o) => ({ value: o, label: o }))"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('result_binding', 'from', v)"
                  />
                  <a-input
                    :value="bindingFor('result_binding').field"
                    placeholder="result"
                    style="flex: 1"
                    @update:value="(v: any) => setBinding('result_binding', 'field', v)"
                  />
                </div>
              </a-form-item>
            </template>
          </a-form>

          <!-- 连线属性 -->
          <a-form v-else-if="selectedEdge" layout="vertical">
            <a-form-item label="名称">
              <a-input
                :value="(selectedEdge.label as string) || ''"
                placeholder="连线名称"
                @update:value="(v: any) => setEdgeLabel(v)"
              />
            </a-form-item>
            <a-form-item label="连线条件">
              <a-input
                :value="edgeConfig().condition"
                :placeholder="
                  selectedEdgeSourceType === 'condition'
                    ? '{{previous.matched}} == “true”（是）或 == “false”（否）'
                    : '例如: {{previous.output}} == “ok”'
                "
                @update:value="(v: any) => (edgeConfig().condition = v)"
              />
            </a-form-item>
            <a-form-item v-if="selectedEdgeSourceType === 'condition'" label="条件分支">
              <a-select
                :value="edgeConfig().branch || undefined"
                placeholder="请选择"
                allow-clear
                :options="[
                  { value: 'true', label: 'true / 是' },
                  { value: 'false', label: 'false / 否' },
                ]"
                @update:value="(v: any) => (edgeConfig().branch = v || '')"
              />
            </a-form-item>
          </a-form>

          <a-empty
            v-else
            description="选择一个节点或连线后编辑属性"
            :image-style="{ height: '60px' }"
          />

          <!-- 自定义字段 -->
          <template v-if="selectedNode || selectedEdge">
            <div
              style="display: flex; justify-content: space-between; align-items: center; margin: 8px 0"
            >
              <span style="font-size: 13px; font-weight: 600">自定义字段</span>
              <a-button size="small" @click="addCustomField">添加字段</a-button>
            </div>
            <div v-if="customFields.length" style="display: flex; flex-direction: column; gap: 8px">
              <div
                v-for="field in customFields"
                :key="field.key"
                style="display: flex; gap: 4px; align-items: center"
              >
                <a-input
                  :value="field.key"
                  size="small"
                  placeholder="字段名"
                  @update:value="(v: any) => setCustomField(field.key, v, field.value)"
                />
                <a-input
                  :value="field.value"
                  size="small"
                  placeholder="值"
                  @update:value="(v: any) => setCustomField(field.key, field.key, v)"
                />
                <a-button size="small" type="text" danger @click="removeCustomField(field.key)">
                  ×
                </a-button>
              </div>
            </div>
            <div v-else style="font-size: 12px; color: var(--ant-color-text-tertiary)">
              暂无自定义字段
            </div>
          </template>
        </a-card>

        <!-- 试运行轨迹 -->
        <a-card v-if="dryRunPanelVisible" size="small" style="margin-top: 16px">
          <template #title>试运行轨迹</template>
          <template #extra>
            <a-button size="small" type="text" title="关闭" @click="closeDryRunPanel">×</a-button>
          </template>
          <div v-if="dryRunTrace.length" style="display: flex; flex-direction: column; gap: 8px">
            <div v-for="(step, i) in dryRunTrace" :key="i" style="font-size: 12px">
              <strong>{{ i + 1 }}. {{ step.label || step.node_id }}</strong>
              <span style="color: var(--ant-color-text-tertiary); margin-left: 8px">
                {{ step.type }} · {{ step.status }}
              </span>
            </div>
          </div>
          <div v-else style="font-size: 12px; color: var(--ant-color-text-tertiary)">暂无轨迹</div>
        </a-card>
      </a-col>
    </a-row>

    <!-- 工作流信息 modal -->
    <a-modal v-model:open="showMeta" title="工作流信息" width="560px" @ok="applyMetaModal">
      <a-form layout="vertical">
        <a-form-item label="名称" required>
          <a-input v-model:value="metaForm.name" placeholder="基础 Web 扫描" />
        </a-form-item>
        <a-form-item label="ID" required>
          <a-input
            v-if="!currentId"
            v-model:value="metaForm.id"
            placeholder="web-scan-basic"
          />
          <a-typography-text v-else code>{{ metaForm.id }}</a-typography-text>
          <div v-if="!currentId" style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            创建后不可修改，用于 API 与角色绑定
          </div>
        </a-form-item>
        <a-form-item label="描述">
          <a-input v-model:value="metaForm.description" placeholder="可选" />
        </a-form-item>
        <a-form-item label="启用">
          <a-switch v-model:checked="metaForm.enabled" />
          <span style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-left: 8px">
            禁用后无法被角色自动触发
          </span>
        </a-form-item>
      </a-form>
    </a-modal>

    <!-- 试运行 modal -->
    <a-modal
      v-model:open="showDryRun"
      title="试运行"
      width="480px"
      ok-text="试运行"
      cancel-text="取消"
      @ok="doDryRun"
    >
      <a-form layout="vertical">
        <a-form-item label="输入试运行消息">
          <a-input v-model:value="dryRunMessage" @press-enter="doDryRun" />
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
            该消息将作为开始节点的 message 输入。
          </div>
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
