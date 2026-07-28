<script lang="ts" setup>
import type { ToonflowApi } from '#/api/toonflow';

import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { Page } from '@vben/common-ui';
import { downloadFileFromBlob } from '@vben/utils';

import {
  Button,
  Card,
  Col,
  Form,
  Input,
  InputNumber,
  List,
  Modal,
  Popconfirm,
  Row,
  Select,
  Space,
  Statistic,
  Table,
  Tabs,
  Tag,
  Typography,
  Upload,
  message,
} from 'ant-design-vue';

import {
  addNovel,
  addScript,
  addStoryboard,
  addVideoTrack,
  batchDeleteStoryboards,
  batchGenerateVideoPrompts,
  batchGenerateVideos,
  clearAgentMemory,
  deleteNovel,
  deleteScripts,
  deleteTrackVideo,
  downloadStoryboardPreview,
  exportFinalVideo,
  executeAgentTool,
  editStoryboardInfo,
  extractScriptAssets,
  getAssets,
  getFlowData,
  getImageFlow,
  generateFlowImage,
  generateStoryboardImages,
  generateTrackVideo,
  generateVideoPrompt,
  previewStoryboardImages,
  pollStoryboardImages,
  pollTrackVideos,
  pollScriptAssets,
  generateNovelEvents,
  getNovelData,
  getProject,
  getProjectStatistics,
  getScripts,
  getStoryboards,
  getVideoWorkbench,
  removeStoryboard,
  reorderStoryboards,
  reorderVideoTracks,
  retryTrackVideo,
  bindTrackStoryboards,
  cancelTrackVideo,
  saveFlowData,
  saveImageFlow,
  saveScriptAgentPlan,
  selectTrackVideo,
  updateNovel,
  updateScript,
  updateStoryboardUrl,
  updateVideoTrackPrompt,
  updateImageFlow,
  uploadFlowImage,
} from '#/api/toonflow';
import { parseNovelText } from './novel-import';
import AgentChat from './AgentChat.vue';
import ImageFlowEditor from './ImageFlowEditor.vue';
import ProductionFlowCanvas from './ProductionFlowCanvas.vue';
import { assetFileUrl } from '../assets/asset-types';
import '../shared/page-card.css';

defineOptions({ name: 'ToonflowProjectDetail' });

const route = useRoute();
const router = useRouter();
const projectId = computed(() => Number(route.params.id));

const activeTab = ref('novel');
const loading = ref(false);
const project = ref<ToonflowApi.Project>();
const imageQuality = computed(() =>
  ['1K', '2K', '4K'].includes(project.value?.imageQuality ?? '')
    ? project.value!.imageQuality
    : '2K',
);
const videoMode = computed(() =>
  ['text', 'singleImage', 'startEndRequired', 'endFrameOptional', 'startFrameOptional'].includes(
    project.value?.mode ?? '',
  )
    ? project.value!.mode
    : 'text',
);
const statistics = reactive<ToonflowApi.ProjectStatistics>({ roleCount: 0, scriptCount: 0, videoCount: 0, storyboardCount: 0 });
const novels = ref<ToonflowApi.NovelChapter[]>([]);
const scripts = ref<ToonflowApi.Script[]>([]);
const assets = ref<ToonflowApi.Asset[]>([]);
const productionAssets = ref<ToonflowApi.Asset[]>([]);
const storyboards = ref<ToonflowApi.Storyboard[]>([]);
const selectedScriptId = ref<number>();
const selectedScript = computed(() =>
  scripts.value.find((item) => item.id === selectedScriptId.value),
);
const flowText = ref('{\n  "script": "",\n  "storyboard": [],\n  "workbench": { "videoList": [] }\n}');

const novelModalOpen = ref(false);
const scriptModalOpen = ref(false);
const storyboardModalOpen = ref(false);
const flowImageModalOpen = ref(false);
const imageFlowId = ref<number>();
const editingAssetId = ref<number>();
const editingStoryboardId = ref<number>();
const editingStoryboard = ref<ToonflowApi.Storyboard>();
const editingAssetName = ref('');
const imageFlowNodes = ref<any[]>([]);
const imageFlowEdges = ref<any[]>([]);
const imageFlowEditorKey = ref(0);
const generatingImageNodeId = ref('');
const storyboardPreviewOpen = ref(false);
const storyboardPreview = ref('');
const productionAgentCollapsed = ref(false);
const productionAgentActivity = ref('等待指令');
let productionAgentSyncTimer: ReturnType<typeof setInterval> | undefined;
const videoTracks = ref<any[]>([]);
const trackBindingOpen = ref(false);
const trackBindingTarget = ref<any>();
const trackBindingStoryboardIds = ref<number[]>([]);
const agentType = ref<'productionAgent' | 'scriptAgent'>('scriptAgent');
const scriptPlan = reactive({ storySkeleton: '', adaptationStrategy: '' });

// Agent workspace tabs - populated in real-time from sub-agent outputs
const workspaceTabs = reactive<{ key: string; label: string; content: string }[]>([
  { key: 'storySkeleton', label: '故事骨架', content: '' },
  { key: 'adaptationStrategy', label: '改编策略', content: '' },
  { key: 'script', label: '剧本', content: '' },
]);
const workspaceActiveTab = ref('storySkeleton');

// Pipeline stages for process visualization
type StageStatus = 'pending' | 'active' | 'completed' | 'review';
const pipelineStages = reactive<{ key: string; label: string; status: StageStatus }[]>([
  { key: 'init', label: '项目初始化', status: 'completed' },
  { key: 'skeleton', label: '故事骨架', status: 'pending' },
  { key: 'adaptation', label: '改编策略', status: 'pending' },
  { key: 'script', label: '剧本编写', status: 'pending' },
]);

function updatePipelineStage(toolName: string, status: StageStatus) {
  const stageMap: Record<string, string> = {
    storySkeleton: 'skeleton',
    run_sub_agent_storySkeleton: 'skeleton',
    adaptationStrategy: 'adaptation',
    run_sub_agent_adaptationStrategy: 'adaptation',
    script: 'script',
    run_sub_agent_script: 'script',
    save_scripts: 'script',
  };
  const stageKey = stageMap[toolName];
  if (!stageKey) return;
  const idx = pipelineStages.findIndex((s) => s.key === stageKey);
  if (idx >= 0) {
    pipelineStages[idx]!.status = status;
    // Mark previous stages as completed
    for (let i = 0; i < idx; i++) {
      if (pipelineStages[i]!.status === 'active') pipelineStages[i]!.status = 'completed';
    }
  }
}

function onAgentToolResult(payload: { toolName: string; result: any }) {
  const { toolName, result } = payload;
  if (
    [
      'add_deriveAsset',
      'del_deriveAsset',
      'generate_deriveAsset',
      'run_sub_agent_derive_assets',
      'run_sub_agent_generate_assets',
    ].includes(toolName)
  ) {
    void loadFlow().then(() => {
      if (toolName === 'generate_deriveAsset' || toolName === 'run_sub_agent_generate_assets') {
        scheduleProductionAssetRefresh();
      }
    });
    return;
  }
  if (['generate_video_prompt', 'select_video', 'update_video_prompt'].includes(toolName)) {
    loadFlow();
    return;
  }
  if (
    [
      'run_sub_agent_director_plan',
      'run_sub_agent_storyboard_table',
      'run_sub_agent_storyboard_panel',
      'run_sub_agent_storyboard_gen',
      'add_flowData_storyboard',
      'update_storyboard',
      'generate_storyboard',
    ].includes(toolName)
  ) {
    void loadFlow();
  }
  // Extract workspace content from sub-agent results
  let raw = typeof result === 'string' ? result : JSON.stringify(result);
  // If result is a JSON wrapper like {"agent":"...","content":"..."}, extract content
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && parsed.content && typeof parsed.content === 'string') {
      raw = parsed.content;
    }
  } catch { /* not JSON, use raw string */ }
  let content = raw;

  // Match tool name to workspace tab and pipeline stage
  if (toolName.includes('storySkeleton') || toolName.includes('skeleton')) {
    workspaceTabs[0]!.content = extractXmlContent(content, 'storySkeleton') || content;
    workspaceActiveTab.value = 'adaptationStrategy';
    updatePipelineStage(toolName, 'completed');
  } else if (toolName.includes('adaptationStrategy') || toolName.includes('adaptation')) {
    workspaceTabs[1]!.content = extractXmlContent(content, 'adaptationStrategy') || content;
    workspaceActiveTab.value = 'script';
    updatePipelineStage(toolName, 'completed');
  } else if (toolName.includes('script') && !toolName.includes('get_script')) {
    workspaceTabs[2]!.content = extractScriptItems(content);
    workspaceActiveTab.value = 'script';
    updatePipelineStage(toolName, 'completed');
  } else if (toolName === 'save_scripts') {
    workspaceActiveTab.value = 'script';
    updatePipelineStage(toolName, 'completed');
  } else if (toolName.includes('supervision') || toolName.includes('review')) {
    // Supervision agent reviewing - mark current stage as review
    const currentStage = pipelineStages.find((s) => s.status === 'completed');
    if (currentStage) currentStage.status = 'review';
  }
}

function extractXmlContent(text: string, tag: string): string | null {
  const regex = new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`, 'i');
  const match = text.match(regex);
  return match?.[1]?.trim() ?? null;
}

function extractScriptItems(text: string): string {
  const matches = text.matchAll(/<scriptItem\s+name="([^"]*)">([\s\S]*?)<\/scriptItem>/gi);
  const items: string[] = [];
  for (const m of matches) {
    items.push(`### ${m[1]}\n\n${m[2]?.trim() ?? ''}`);
  }
  return items.length > 0 ? items.join('\n\n---\n\n') : text;
}

function formatEventDisplay(eventJson: string): string {
  if (!eventJson) return '';
  try {
    const arr = JSON.parse(eventJson);
    if (!Array.isArray(arr)) return eventJson;
    return arr.map((e: any, i: number) => `${i + 1}.${e.name}：${e.detail}`).join('；');
  } catch {
    return eventJson;
  }
}

function renderMarkdown(text: string): string {
  if (!text) return '';
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    .replace(/\n\n/g, '</p><p>')
    .replace(/\n/g, '<br>');
}

// Agent chat messages (WebSocket-driven)
interface ChatContentBlock { type: string; id: string; data: any; status: string }
interface ChatMessage { id: string; role: 'assistant' | 'system' | 'user'; name?: string; status: string; datetime: string; content: ChatContentBlock[] }
const scriptChatMessages = ref<ChatMessage[]>([]);
const productionChatMessages = ref<ChatMessage[]>([]);
const activeAgentMessages = computed(() =>
  agentType.value === 'productionAgent'
    ? productionChatMessages.value
    : scriptChatMessages.value,
);
const agentChatRef = ref<InstanceType<typeof AgentChat> | null>(null);
const productionAgentChatRef = ref<InstanceType<typeof AgentChat> | null>(null);
const productionFlowCanvasRef = ref<InstanceType<typeof ProductionFlowCanvas> | null>(null);

const novelForm = reactive({
  id: undefined as number | undefined,
  index: 1,
  reel: '',
  chapter: '',
  chapterData: '',
  event: '',
});

const batchNovelText = ref('');

const scriptForm = reactive({
  id: undefined as number | undefined,
  name: '',
  content: '',
  assets: [] as number[],
  shouldGenerateImage: 1,
});

const storyboardForm = reactive({
  id: undefined as number | undefined,
  prompt: '',
  duration: 4,
  state: '未生成',
  videoDesc: '',
  shouldGenerateImage: 1,
  track: 'main',
  associateAssetsIds: [] as number[],
});

function scriptEpisodeNumber(script: ToonflowApi.Script) {
  const match = script.name.match(/(?:EP|第)\s*0*(\d+)/i);
  return match?.[1] ? Number(match[1]) : Number.POSITIVE_INFINITY;
}

const orderedScripts = computed(() =>
  [...scripts.value].sort(
    (left, right) =>
      scriptEpisodeNumber(left) - scriptEpisodeNumber(right) ||
      left.createTime - right.createTime,
  ),
);

const scriptOptions = computed(() =>
  orderedScripts.value.map((item) => ({ label: item.name, value: item.id })),
);

const assetOptions = computed(() =>
  assets.value.map((item) => ({ label: `${item.name} (${item.type})`, value: item.id })),
);

const novelColumns = [
  { title: '序号', dataIndex: 'index', width: 72, align: 'center' as const },
  { title: '分卷', dataIndex: 'reel', width: 120, ellipsis: true },
  { title: '章节', dataIndex: 'chapter', width: 180, ellipsis: true },
  { title: '正文摘要', dataIndex: 'chapterData', width: 360 },
  { title: '状态', dataIndex: 'eventState', width: 96, align: 'center' as const },
  { title: '事件', dataIndex: 'event', width: 360 },
  { title: '操作', key: 'action', width: 220, fixed: 'right' as const },
];

async function loadProject() {
  const [projectData, statisticData] = await Promise.all([getProject(projectId.value), getProjectStatistics(projectId.value)]);
  project.value = projectData;
  Object.assign(statistics, statisticData);
}

async function loadNovels() {
  novels.value = await getNovelData(projectId.value);
}

async function loadScripts() {
  scripts.value = await getScripts(projectId.value);
  const selectionStillExists = scripts.value.some(
    (script) => script.id === selectedScriptId.value,
  );
  if (!selectionStillExists && orderedScripts.value.length > 0) {
    selectedScriptId.value = orderedScripts.value[0]!.id;
  }
}

async function loadAssets() {
  assets.value = await getAssets(projectId.value);
}

async function loadFlow() {
  if (!selectedScriptId.value) {
    flowText.value =
      '{\n  "script": "",\n  "storyboard": [],\n  "workbench": { "videoList": [] }\n}';
    storyboards.value = [];
    productionAssets.value = [];
    return;
  }
  const flow = await getFlowData(projectId.value, selectedScriptId.value);
  flowText.value = JSON.stringify(flow, null, 2);
  productionAssets.value = Array.isArray(flow.assets) ? flow.assets : [];
  storyboards.value = await getStoryboards(projectId.value, selectedScriptId.value);
  const workbench = await getVideoWorkbench(projectId.value, selectedScriptId.value);
  videoTracks.value = workbench.trackList ?? [];
}

let productionAssetRefreshTimer: ReturnType<typeof setTimeout> | undefined;
let productionAssetRefreshAttempts = 0;

function scheduleProductionAssetRefresh() {
  if (productionAssetRefreshTimer) clearTimeout(productionAssetRefreshTimer);
  productionAssetRefreshAttempts = 0;
  const refresh = async () => {
    await loadFlow();
    productionAssetRefreshAttempts += 1;
    if (
      productionAssetRefreshAttempts < 40 &&
      productionAssets.value.some(
        (asset) =>
          asset.imageState === '生成中' ||
          asset.derive?.some((derived) => derived.imageState === '生成中'),
      )
    ) {
      productionAssetRefreshTimer = setTimeout(refresh, 3000);
    }
  };
  productionAssetRefreshTimer = setTimeout(refresh, 1500);
}

async function loadAll() {
  if (!projectId.value || isNaN(projectId.value)) return;
  loading.value = true;
  try {
    await Promise.all([loadProject(), loadNovels(), loadScripts(), loadAssets()]);
    await loadFlow();
  } finally {
    loading.value = false;
  }
}

function openNovel(chapter?: any) {
  Object.assign(novelForm, {
    id: chapter?.id,
    index: chapter?.index ?? novels.value.length + 1,
    reel: chapter?.reel ?? '',
    chapter: chapter?.chapter ?? '',
    chapterData: chapter?.chapterData ?? '',
    event: chapter?.event ?? '',
  });
  batchNovelText.value = '';
  novelModalOpen.value = true;
}

async function saveNovel() {
  if (batchNovelText.value.trim()) {
    const chapters = parseNovelText(batchNovelText.value);
    if (chapters.length === 0) {
      message.warning('没有可导入的章节内容');
      return;
    }
    await importNovelChapters(chapters);
  } else if (novelForm.id) {
    await updateNovel({ ...novelForm, id: novelForm.id });
  } else {
    await addNovel(projectId.value, [{ ...novelForm }]);
  }
  novelModalOpen.value = false;
  await loadNovels();
}

async function importNovelChapters(chapters: ReturnType<typeof parseNovelText>) {
  const batchSize = 20;
  for (let start = 0; start < chapters.length; start += batchSize) {
    await addNovel(projectId.value, chapters.slice(start, start + batchSize));
  }
}

async function importNovelFile(file: File) {
  try {
    const buffer = await file.arrayBuffer();
    let content = new TextDecoder('utf-8').decode(buffer);
    if (content.includes('\uFFFD')) content = new TextDecoder('gb18030').decode(buffer);
    const title = file.name.replace(/\.(?:md|txt)$/i, '');
    const chapters = parseNovelText(content, title);
    if (!chapters.length) {
      message.warning('文件中没有可导入的正文');
      return false;
    }
    await importNovelChapters(chapters);
    message.success(`已从文件导入 ${chapters.length} 个章节`);
    await loadNovels();
  } catch (error) {
    message.error(error instanceof Error ? error.message : '文件导入失败');
  }
  return false;
}

async function removeNovel(row: any) {
  await deleteNovel(row.id);
  await loadNovels();
}

async function extractNovelEvents(row: any) {
  await generateNovelEvents(projectId.value, [row.id]);
  message.success('事件提取任务已提交，可在任务中心查看进度');
  window.setTimeout(loadNovels, 2000);
}

async function extractAllNovelEvents() {
  const ids = novels.value.filter((chapter) => chapter.eventState !== 1).map((chapter) => chapter.id);
  if (!ids.length) {
    message.info('没有待提取事件的章节');
    return;
  }
  await generateNovelEvents(projectId.value, ids);
  message.success(`已提交 ${ids.length} 个章节的事件提取任务`);
  window.setTimeout(loadNovels, 2000);
}

function openScript(script?: any) {
  Object.assign(scriptForm, {
    id: script?.id,
    name: script?.name ?? '',
    content: script?.content ?? '',
    assets:
      script?.relatedAssets?.map((item: { id: number; name: string }) => item.id) ??
      [],
    shouldGenerateImage: script?.shouldGenerateImage ?? 1,
  });
  scriptModalOpen.value = true;
}

async function saveScript() {
  if (!scriptForm.name.trim()) {
    message.warning('请输入剧本名称');
    return;
  }
  if (scriptForm.id) {
    await updateScript({
      id: scriptForm.id,
      name: scriptForm.name,
      content: scriptForm.content,
      assets: scriptForm.assets,
      shouldGenerateImage: scriptForm.shouldGenerateImage,
    });
  } else {
    const result = await addScript({
      projectId: projectId.value,
      name: scriptForm.name,
      content: scriptForm.content,
      assets: scriptForm.assets,
      shouldGenerateImage: scriptForm.shouldGenerateImage,
    });
    selectedScriptId.value = result.id;
  }
  scriptModalOpen.value = false;
  await loadScripts();
  await loadFlow();
}

async function removeScript(script: any) {
  await deleteScripts([script.id]);
  if (selectedScriptId.value === script.id) {
    selectedScriptId.value = undefined;
  }
  await loadScripts();
  await loadFlow();
}

async function extractAssetsFromScript(script: ToonflowApi.Script) {
  await extractScriptAssets(projectId.value, [script.id]);
  message.success('资产提取任务已提交');
  for (let attempt = 0; attempt < 90; attempt += 1) {
    await new Promise((resolve) => window.setTimeout(resolve, 2000));
    const [result] = await pollScriptAssets([script.id]);
    if (!result || result.extractState === 2 || result.extractState === 0) continue;
    await Promise.all([loadScripts(), loadAssets(), loadFlow()]);
    if (result.extractState === 1) {
      message.success(
        `资产提取完成，共识别 ${result.appearanceCount ?? 0} 套人物场景服装/形态，请在分镜制作中确认后生成`,
      );
    } else {
      message.error(result.errorReason || '资产提取失败');
    }
    return;
  }
  message.warning('资产提取仍在处理中，请稍后刷新查看');
}

async function loadAgentMemory() {
  // No-op: backend sends history via WebSocket on connect
}

async function resetAgentWorkspace() {
  // Clear chat messages
  if (agentType.value === 'productionAgent') {
    productionChatMessages.value = [];
  } else {
    scriptChatMessages.value = [];
  }
  // Reset workspace tabs
  workspaceTabs.forEach((t) => (t.content = ''));
  workspaceActiveTab.value = 'storySkeleton';
  // Reset pipeline stages
  pipelineStages.forEach((s) => (s.status = s.key === 'init' ? 'completed' : 'pending'));
  // Clear server-side memory
  try {
    const agentIsolationKey = `${agentType.value}:${projectId.value}:${agentType.value === 'productionAgent' ? (selectedScriptId.value ?? 'none') : 'project'}`;
    await clearAgentMemory(agentType.value, agentIsolationKey);
  } catch { /* ignore */ }
  // Reconnect WebSocket
  agentChatRef.value?.disconnect();
  setTimeout(() => agentChatRef.value?.connect(), 200);
  message.success('已重新开始');
}

async function resetProductionAgent() {
  if (!selectedScriptId.value) {
    message.warning('请先选择剧本');
    return;
  }
  const isolationKey = `productionAgent:${projectId.value}:${selectedScriptId.value}`;
  await clearAgentMemory('productionAgent', isolationKey);
  productionChatMessages.value = [];
  productionAgentChatRef.value?.disconnect();
  window.setTimeout(() => productionAgentChatRef.value?.connect(), 200);
  message.success('当前剧本的分镜制作 Agent 已重新开始');
}
function changeProductionScript(value: unknown) {
  const scriptId = Number(value);
  if (!Number.isFinite(scriptId)) return;
  const running = productionChatMessages.value.some((item: any) =>
    item.status === 'pending' || item.status === 'streaming',
  );
  if (!running) {
    selectedScriptId.value = scriptId;
    return;
  }
  Modal.confirm({
    title: 'Agent 正在执行',
    content: '切换剧本不会停止当前 Agent，但当前画布将切换到另一份工作区。确认继续吗？',
    okText: '确认切换',
    cancelText: '留在当前剧本',
    onOk: () => { selectedScriptId.value = scriptId; },
  });
}
async function clearProductionAgentMemory(memoryType: 'all' | 'message' | 'summary') {
  if (!selectedScriptId.value) return;
  const isolationKey = `productionAgent:${projectId.value}:${selectedScriptId.value}`;
  await clearAgentMemory('productionAgent', isolationKey, memoryType);
  if (memoryType !== 'summary') productionChatMessages.value = [];
  message.success(memoryType === 'message' ? '消息记忆已清除' : memoryType === 'summary' ? '摘要记忆已清除' : '全部记忆已清除');
}

async function saveAgentWorkspace() {
  await saveScriptAgentPlan(projectId.value, {
    ...scriptPlan,
    script: scripts.value.map(({ id, name, content }) => ({ id, name, content })),
  });
  message.success('剧本 Agent 工作区已保存');
}

function openScriptGeneration() {
  agentType.value = 'scriptAgent';
  scriptChatMessages.value = [];
  agentChatRef.value?.connect();
  setTimeout(() => {
    agentChatRef.value?.send(
      '你好，请先读取当前项目的章节事件和已有剧本，分析项目的整体状态，然后告诉我你的分析和建议，并询问我是否需要开始生成下一集剧本。',
    );
  }, 300);
}

async function saveFlowText() {
  if (!selectedScriptId.value) {
    message.warning('请先选择剧本');
    return;
  }
  let data: Record<string, any>;
  try {
    data = JSON.parse(flowText.value);
  } catch {
    message.error('Flow JSON 格式不正确');
    return;
  }
  await saveFlowData(projectId.value, selectedScriptId.value, data);
  message.success('生产工作流已保存');
}

let flowCanvasSaveTimer: ReturnType<typeof setTimeout> | undefined;
function flowObject() {
  try { return JSON.parse(flowText.value || '{}') as Record<string, any>; }
  catch { return {}; }
}
function persistFlowObject(data: Record<string, any>) {
  flowText.value = JSON.stringify(data, null, 2);
  if (flowCanvasSaveTimer) clearTimeout(flowCanvasSaveTimer);
  flowCanvasSaveTimer = setTimeout(async () => {
    if (!selectedScriptId.value) return;
    await saveFlowData(projectId.value, selectedScriptId.value, data);
  }, 500);
}
function saveProductionCanvasPositions(positions: Record<string, { x: number; y: number }>) {
  const data = flowObject();
  data.canvas = { ...(data.canvas || {}), layoutVersion: 7, positions };
  persistFlowObject(data);
}
function updateProductionFlowSection(key: 'scriptPlan' | 'storyboardTable', value: string) {
  const data = flowObject();
  data[key] = value;
  persistFlowObject(data);
  message.success(key === 'scriptPlan' ? '导演规划已保存' : '分镜表已保存');
}
function previewProductionFlowSection(payload: { key: 'scriptPlan' | 'storyboardTable'; value: string }) {
  const data = flowObject();
  data[payload.key] = payload.value;
  flowText.value = JSON.stringify(data, null, 2);
}
function onProductionAgentActivity(payload: { status: string; toolName: string }) {
  const names: Record<string, string> = {
    run_sub_agent_derive_assets: '分析衍生资产', run_sub_agent_generate_assets: '生成衍生资产',
    run_sub_agent_director_plan: '生成导演规划', run_sub_agent_storyboard_table: '生成分镜表',
    run_sub_agent_storyboard_panel: '构建分镜面板', run_sub_agent_storyboard_gen: '生成分镜图',
    run_sub_agent_supervision: '监制检查', generate_storyboard: '生成分镜图',
  };
  const toolStages: Record<string, string> = {
    add_deriveAsset: 'script',
    del_deriveAsset: 'script',
    generate_deriveAsset: 'script',
    run_sub_agent_derive_assets: 'script',
    run_sub_agent_generate_assets: 'script',
    run_sub_agent_director_plan: 'scriptPlan',
    run_sub_agent_storyboard_table: 'storyboardTable',
    run_sub_agent_supervision: 'storyboardTable',
    run_sub_agent_storyboard_panel: 'storyboard',
    run_sub_agent_storyboard_gen: 'storyboard',
    generate_storyboard: 'storyboard',
    generate_video_prompt: 'workbench',
    generate_video: 'workbench',
  };
  if (payload.status === 'pending' || payload.status === 'streaming') {
    const stage = toolStages[payload.toolName];
    if (stage) nextTick(() => productionFlowCanvasRef.value?.focusStage(stage));
  }
  if (payload.toolName === '__agent__') {
    if (payload.status === 'pending' || payload.status === 'streaming') {
      if (!productionAgentSyncTimer) {
        productionAgentSyncTimer = setInterval(() => void loadFlow(), 1800);
      }
      productionAgentActivity.value = 'Agent 执行中';
    } else {
      if (productionAgentSyncTimer) clearInterval(productionAgentSyncTimer);
      productionAgentSyncTimer = undefined;
      void loadFlow();
      productionAgentActivity.value = payload.status === 'error' ? 'Agent 执行失败' : 'Agent 已完成';
    }
    return;
  }
  const name = names[payload.toolName] || payload.toolName;
  productionAgentActivity.value = payload.status === 'complete' ? `${name}完成` : payload.status === 'error' ? `${name}失败` : name;
}

function collectUpstreamNodeIds(nodeId: string) {
  const visited = new Set<string>();
  const visit = (id: string) => {
    for (const edge of imageFlowEdges.value.filter((item) => item.target === id)) {
      if (!visited.has(edge.source)) {
        visited.add(edge.source);
        visit(edge.source);
      }
    }
  };
  visit(nodeId);
  return visited;
}

async function createFlowImage(nodeId: string) {
  if (!project.value?.imageModel) return message.warning('请先配置项目图片模型');
  const generatedNode = imageFlowNodes.value.find((node) => node.id === nodeId && node.type === 'generated');
  if (!generatedNode) return;
  const upstreamIds = collectUpstreamNodeIds(nodeId);
  const upstream = imageFlowNodes.value.filter((node) => upstreamIds.has(node.id));
  const references = upstream.flatMap((node) => [node.data.image, node.data.generatedImage]).filter(Boolean);
  const prompt = [...upstream.filter((node) => node.type === 'prompt').map((node) => node.data.prompt), generatedNode.data.prompt].filter(Boolean).join('\n');
  if (!references.length) return message.warning('请先连接至少一个图片节点');
  if (!prompt.trim()) return message.warning('请先连接编辑指令节点或填写节点编辑要求');
  generatingImageNodeId.value = nodeId;
  try {
    const result = await generateFlowImage({ projectId: projectId.value, model: String(project.value.imageModel), quality: imageQuality.value, ratio: project.value.videoRatio || '16:9', prompt, references, targetType: generatedNode.data.targetType || 'storyboard' });
    generatedNode.data.generatedImage = result.url;
    generatedNode.data.references = references;
    message.success('当前节点图片已生成');
  } finally {
    generatingImageNodeId.value = '';
  }
}

function addImageFlowNode(type: 'generated' | 'prompt' | 'upload') {
  const id = `${type}-${Date.now()}`;
  imageFlowNodes.value.push({
    id,
    type,
    position: { x: imageFlowNodes.value.length * 260, y: 0 },
    data:
      type === 'upload'
        ? { image: '' }
        : type === 'prompt'
          ? { prompt: '' }
          : { generatedImage: '', prompt: '', references: [], targetType: 'storyboard' },
  });
  if (imageFlowNodes.value.length > 1) {
    const source = imageFlowNodes.value.at(-2)?.id;
    imageFlowEdges.value.push({ id: `edge-${source}-${id}`, source, target: id });
  }
}

async function openAssetImageFlow(asset: ToonflowApi.Asset) {
  if (!selectedScriptId.value) return message.warning('请先选择剧本');
  editingAssetId.value = asset.id;
  editingStoryboardId.value = undefined;
  editingAssetName.value = asset.name;
  imageFlowId.value = asset.flowId;
  imageFlowNodes.value = [];
  imageFlowEdges.value = [];
  imageFlowEditorKey.value += 1;
  if (imageFlowId.value) {
    const flow = await getImageFlow(imageFlowId.value);
    imageFlowNodes.value = flow?.nodes ?? [];
    imageFlowEdges.value = flow?.edges ?? [];
  }
  if (imageFlowNodes.value.length === 0) {
    const parentAsset = asset.parentAssetId
      ? productionAssets.value.find((item) => item.id === asset.parentAssetId)
      : undefined;
    const sourceImage = parentAsset?.imageFilePath || asset.imageFilePath;
    const initialPrompt = asset.prompt || asset.description || asset.remark || '';
    const suffix = Date.now();
    const sourceId = `upload-${suffix}`;
    const promptId = `prompt-${suffix}`;
    const generatedId = `generated-${suffix}`;
    imageFlowNodes.value = [
      {
        id: sourceId,
        type: 'upload',
        position: { x: 0, y: 0 },
        data: { image: assetFileUrl(sourceImage) },
      },
      {
        id: promptId,
        type: 'prompt',
        position: { x: 330, y: 0 },
        data: { prompt: initialPrompt },
      },
      {
        id: generatedId,
        type: 'generated',
        position: { x: 660, y: 0 },
        data: {
          generatedImage: '',
          prompt: initialPrompt,
          references: [],
          targetType: asset.type === 'scene' ? 'scene' : 'role',
        },
      },
    ];
    imageFlowEdges.value = [
      { id: `edge-${sourceId}-${generatedId}`, source: sourceId, target: generatedId },
      { id: `edge-${promptId}-${generatedId}`, source: promptId, target: generatedId },
    ];
  }
  restoreMissingImageFlowEdges();
  flowImageModalOpen.value = true;
}

async function openStoryboardImageFlow(storyboard: ToonflowApi.Storyboard) {
  if (!selectedScriptId.value) return message.warning('请先选择剧本');
  editingAssetId.value = undefined;
  editingStoryboardId.value = storyboard.id;
  editingStoryboard.value = storyboard;
  editingAssetName.value = `分镜 ${storyboard.index ?? storyboard.id}`;
  imageFlowId.value = storyboard.flowId;
  imageFlowNodes.value = [];
  imageFlowEdges.value = [];
  imageFlowEditorKey.value += 1;
  if (imageFlowId.value) {
    const flow = await getImageFlow(imageFlowId.value);
    imageFlowNodes.value = flow?.nodes ?? [];
    imageFlowEdges.value = flow?.edges ?? [];
  }
  if (imageFlowNodes.value.length === 0) {
    const suffix = Date.now();
    const promptId = `prompt-${suffix}`;
    const generatedId = `generated-${suffix}`;
    const referenceNodes = (storyboard.associateAssetsIds ?? []).flatMap((assetId, index) => {
      const asset = productionAssets.value.find((item) => item.id === assetId);
      return asset?.imageFilePath
        ? [{
            id: `upload-asset-${assetId}-${suffix}`,
            type: 'upload',
            position: { x: 0, y: (index + 1) * 220 },
            data: {
              assetId,
              assetName: asset.name,
              assetSlot: index,
              assetType: asset.type,
              image: assetFileUrl(asset.imageFilePath),
            },
          }]
        : [];
    });
    imageFlowNodes.value = [
      ...referenceNodes,
      {
        id: promptId,
        type: 'prompt',
        position: { x: 350, y: 0 },
        data: { prompt: storyboard.prompt || storyboard.videoDesc || '' },
      },
      {
        id: generatedId,
        type: 'generated',
        position: { x: 700, y: 0 },
        data: {
          generatedImage: assetFileUrl(storyboard.filePath || storyboard.src),
          prompt: storyboard.prompt || '',
          references: [],
          targetType: 'storyboard',
        },
      },
    ];
    imageFlowEdges.value = [
      ...referenceNodes.map((node) => ({ id: `edge-${node.id}-${generatedId}`, source: node.id, target: generatedId })),
      { id: `edge-${promptId}-${generatedId}`, source: promptId, target: generatedId },
    ];
  }
  const generatedNode = imageFlowNodes.value.find((node) => node.type === 'generated');
  if (generatedNode) {
    generatedNode.data.generatedImage ||= assetFileUrl(storyboard.filePath || storyboard.src);
    for (const [index, assetId] of (storyboard.associateAssetsIds ?? []).entries()) {
      if (imageFlowNodes.value.some((node) => node.type === 'upload' && node.data.assetSlot === index)) continue;
      const asset = productionAssets.value.find((item) => item.id === assetId);
      if (!asset) continue;
      const nodeId = `upload-asset-${assetId}-${Date.now()}-${index}`;
      imageFlowNodes.value.push({
        id: nodeId,
        type: 'upload',
        position: { x: 0, y: index * 220 },
        data: {
          assetId,
          assetName: asset.name,
          assetSlot: index,
          assetType: asset.type,
          image: assetFileUrl(asset.imageFilePath),
        },
      });
      imageFlowEdges.value.push({ id: `edge-${nodeId}-${generatedNode.id}`, source: nodeId, target: generatedNode.id });
    }
  }
  restoreMissingImageFlowEdges();
  flowImageModalOpen.value = true;
}

const imageFlowAssetOptions = computed(() =>
  productionAssets.value
    .filter((asset) => !!asset.imageFilePath && (asset.type !== 'role' || !!asset.parentAssetId))
    .map((asset) => ({
      id: asset.id,
      image: assetFileUrl(asset.imageFilePath),
      label: asset.name,
      type: asset.type,
    })),
);

async function selectImageFlowAsset(nodeId: string, assetId: number) {
  const node = imageFlowNodes.value.find((item) => item.id === nodeId && item.type === 'upload');
  const asset = productionAssets.value.find((item) => item.id === assetId);
  if (!node || !asset?.imageFilePath) return;
  node.data.assetId = asset.id;
  node.data.assetName = asset.name;
  node.data.assetType = asset.type;
  node.data.image = assetFileUrl(asset.imageFilePath);
  const storyboard = editingStoryboard.value;
  const slot = Number(node.data.assetSlot);
  if (storyboard && Number.isInteger(slot)) {
    const assetIds = [...(storyboard.associateAssetsIds ?? [])];
    assetIds[slot] = asset.id;
    await editStoryboardInfo({
      associateAssetsIds: assetIds,
      duration: storyboard.duration,
      id: storyboard.id,
      prompt: storyboard.prompt || '',
      shouldGenerateImage: storyboard.shouldGenerateImage ?? 1,
      track: storyboard.track,
      videoDesc: storyboard.videoDesc || '',
    });
    storyboard.associateAssetsIds = assetIds;
    message.success(`已将参考资产更换为“${asset.name}”`);
  }
}

async function saveVisualImageFlow() {
  const storyboardGeneratedImage = editingStoryboardId.value
    ? [...imageFlowNodes.value]
        .reverse()
        .find((node) => node.type === 'generated' && node.data.generatedImage)?.data.generatedImage
    : undefined;
  if (editingStoryboardId.value && !storyboardGeneratedImage) {
    return message.warning('请先执行图片编辑节点并生成结果');
  }
  if (imageFlowId.value) {
    await updateImageFlow(imageFlowId.value, imageFlowNodes.value, imageFlowEdges.value);
  } else {
    const result = await saveImageFlow(
      imageFlowNodes.value,
      imageFlowEdges.value,
      editingAssetId.value,
    );
    imageFlowId.value = result.id;
  }
  if (editingStoryboardId.value && imageFlowId.value) {
    await updateStoryboardUrl(
      editingStoryboardId.value,
      storyboardGeneratedImage,
      imageFlowId.value,
    );
  }
  await loadFlow();
  message.success('图片工作流已保存');
}

async function uploadImageFlowReference(nodeId: string, file: File) {
  if (!file || !selectedScriptId.value) return;
  const base64Data = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
  const url = await uploadFlowImage(projectId.value, selectedScriptId.value, base64Data);
  const uploadNode = imageFlowNodes.value.find((node) => node.id === nodeId && node.type === 'upload');
  if (uploadNode) uploadNode.data.image = url;
  message.success('参考图上传成功');
}

function connectImageFlow(connection: any) {
  if (!connection.source || !connection.target || connection.source === connection.target) return;
  const exists = imageFlowEdges.value.some((edge) => edge.source === connection.source && edge.target === connection.target);
  if (!exists) imageFlowEdges.value.push({ id: `edge-${connection.source}-${connection.target}-${Date.now()}`, source: connection.source, target: connection.target });
}

function restoreMissingImageFlowEdges() {
  if (imageFlowEdges.value.length || imageFlowNodes.value.length < 2) return;
  const generatedNode = imageFlowNodes.value.find((node) => node.type === 'generated');
  if (!generatedNode) return;
  imageFlowEdges.value = imageFlowNodes.value
    .filter((node) => node.id !== generatedNode.id && ['prompt', 'upload'].includes(node.type))
    .map((node) => ({
      id: `edge-${node.id}-${generatedNode.id}`,
      source: node.id,
      target: generatedNode.id,
    }));
}

async function planImageEditWithAgent(nodeId: string) {
  const node = imageFlowNodes.value.find((item) => item.id === nodeId);
  if (!node || !selectedScriptId.value) return message.warning('请先选择剧本');
  generatingImageNodeId.value = nodeId;
  try {
    const result = await executeAgentTool({
      agentType: 'productionAgent',
      projectId: projectId.value,
      scriptId: selectedScriptId.value,
      toolName: 'run_sub_agent_image_edit',
      arguments: { prompt: `请读取当前剧本、资产和分镜上下文，为一次${node.data.targetType || 'storyboard'}图片编辑生成可直接执行的中文编辑指令。只输出编辑指令，明确需要改变的内容以及必须保持不变的人物身份、构图和视觉元素，不要执行生图。` },
    });
    node.data.prompt = result.result?.content || result.result || '';
    message.success('生产 Agent 已生成当前节点的编辑指令');
  } finally {
    generatingImageNodeId.value = '';
  }
}

function removeImageFlowNode(id: string) {
  imageFlowNodes.value = imageFlowNodes.value.filter((node) => node.id !== id);
  imageFlowEdges.value = imageFlowEdges.value.filter(
    (edge) => edge.source !== id && edge.target !== id,
  );
}

const insertAfterStoryboardId = ref<number>();
function openStoryboard(item?: ToonflowApi.Storyboard, keepInsertPosition = false) {
  if (!keepInsertPosition) insertAfterStoryboardId.value = undefined;
  Object.assign(storyboardForm, {
    id: item?.id,
    prompt: item?.prompt ?? '',
    duration: item?.duration ?? 4,
    state: item?.state ?? '未生成',
    videoDesc: item?.videoDesc ?? '',
    shouldGenerateImage: item?.shouldGenerateImage ?? 1,
    track: item?.track ?? 'main',
    associateAssetsIds: [...(item?.associateAssetsIds ?? [])],
  });
  storyboardModalOpen.value = true;
}

function beginInsertStoryboard(item: ToonflowApi.Storyboard) {
  insertAfterStoryboardId.value = item.id;
  openStoryboard(undefined, true);
}

async function saveStoryboardForm() {
  if (!selectedScriptId.value) {
    message.warning('请先选择剧本');
    return;
  }
  if (storyboardForm.id) {
    await editStoryboardInfo({ ...storyboardForm, id: storyboardForm.id });
  } else {
    const created = await addStoryboard({
      ...storyboardForm,
      projectId: projectId.value,
      scriptId: selectedScriptId.value,
    });
    const afterId = insertAfterStoryboardId.value;
    await loadFlow();
    if (afterId && created.id) {
      const ids = storyboards.value.map((item) => item.id).filter((id) => id !== created.id);
      const index = ids.indexOf(afterId);
      ids.splice(index < 0 ? ids.length : index + 1, 0, created.id);
      await saveStoryboardOrder(ids);
    }
    insertAfterStoryboardId.value = undefined;
  }
  storyboardModalOpen.value = false;
  if (storyboardForm.id) await loadFlow();
}

async function deleteStoryboard(row: any) {
  await removeStoryboard(row.id);
  await loadFlow();
}

const storyboardBusy = ref(false);
let storyboardPollTimer: ReturnType<typeof setTimeout> | undefined;
let storyboardPollAttempts = 0;

function scheduleStoryboardPolling() {
  if (storyboardPollTimer) clearTimeout(storyboardPollTimer);
  storyboardPollAttempts = 0;
  const refresh = async () => {
    const generatingIds = storyboards.value
      .filter((item) => item.state === '生成中')
      .map((item) => item.id);
    if (generatingIds.length === 0 || storyboardPollAttempts >= 90) {
      storyboardBusy.value = false;
      return;
    }
    try {
      const completed = await pollStoryboardImages(generatingIds);
      const updates = new Map(completed.map((item) => [item.id, item]));
      storyboards.value = storyboards.value.map((item) =>
        updates.has(item.id) ? { ...item, ...updates.get(item.id) } : item,
      );
    } finally {
      storyboardPollAttempts += 1;
      storyboardPollTimer = setTimeout(refresh, 2000);
    }
  };
  storyboardPollTimer = setTimeout(refresh, 1200);
}

async function generateStoryboards(ids: number[], compulsory = false) {
  if (!selectedScriptId.value || ids.length === 0) return message.warning('请先选择需要生成的分镜');
  storyboardBusy.value = true;
  try {
    await generateStoryboardImages({
      storyboardIds: ids,
      projectId: projectId.value,
      scriptId: selectedScriptId.value,
      concurrentCount: 5,
      compulsory,
    });
    storyboards.value = storyboards.value.map((item) =>
      ids.includes(item.id) ? { ...item, reason: undefined, state: '生成中' } : item,
    );
    message.success(`已提交 ${ids.length} 个分镜生成任务`);
    scheduleStoryboardPolling();
  } catch (error) {
    storyboardBusy.value = false;
    throw error;
  }
}

async function batchDeleteSelectedStoryboards(ids: number[]) {
  await batchDeleteStoryboards(ids, projectId.value);
  message.success(`已删除 ${ids.length} 个分镜`);
  await loadFlow();
}

async function generateAllStoryboardImages() {
  if (!selectedScriptId.value || storyboards.value.length === 0) return message.warning('当前剧本没有可生成的分镜');
  await generateStoryboards(storyboards.value.map((item) => item.id));
}

async function previewAllStoryboardImages() {
  if (storyboards.value.length === 0) return message.warning('当前没有分镜');
  storyboardPreview.value = (await previewStoryboardImages(storyboards.value.map((item) => item.id))) || '';
  if (!storyboardPreview.value) return message.warning('还没有可预览的分镜图片');
  storyboardPreviewOpen.value = true;
}

async function downloadAllStoryboardImages() {
  const ids = storyboards.value.map((item) => item.id);
  if (ids.length === 0) return message.warning('当前没有分镜');
  const blob = await downloadStoryboardPreview(ids);
  downloadFileFromBlob({ fileName: `storyboard-${selectedScriptId.value ?? 'preview'}.png`, source: blob });
}

async function downloadSelectedStoryboardImages(ids: number[]) {
  if (ids.length === 0) return message.warning('请至少选择一个分镜');
  const blob = await downloadStoryboardPreview(ids);
  downloadFileFromBlob({ fileName: `storyboard-selected-${selectedScriptId.value ?? 'preview'}.png`, source: blob });
}

async function saveStoryboardOrder(ids: number[]) {
  if (!selectedScriptId.value) return;
  await reorderStoryboards(projectId.value, selectedScriptId.value, ids);
  const order = new Map(ids.map((id, index) => [id, index]));
  storyboards.value = [...storyboards.value]
    .sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0))
    .map((item, index) => ({ ...item, index }));
  message.success('分镜顺序已保存');
}

async function openVideoTrack(trackId: number) {
  activeTab.value = 'video';
  await nextTick();
  document.getElementById(`video-track-${trackId}`)?.scrollIntoView({
    behavior: 'smooth',
    block: 'start',
  });
}

async function createVideoTrack() {
  if (!selectedScriptId.value) return message.warning('请先选择剧本');
  const id = await addVideoTrack(projectId.value, selectedScriptId.value);
  await loadFlow();
  const track = videoTracks.value.find((item: any) => item.id === id) ?? { id, medias: [] };
  openTrackBinding(track);
  message.success('轨道已新增，请选择要移入的分镜');
}
async function moveVideoTrack(index:number,direction:-1|1){if(!selectedScriptId.value)return;const target=index+direction;if(target<0||target>=videoTracks.value.length)return;const list=[...videoTracks.value];[list[index],list[target]]=[list[target],list[index]];videoTracks.value=list;await reorderVideoTracks(projectId.value,selectedScriptId.value,list.map(track=>track.id))}
function openTrackBinding(track:any){trackBindingTarget.value=track;trackBindingStoryboardIds.value=(track.medias??[]).map((media:any)=>media.id);trackBindingOpen.value=true}
async function confirmTrackBinding(){const track=trackBindingTarget.value;if(!track)return;await bindTrackStoryboards(track.id,trackBindingStoryboardIds.value);trackBindingOpen.value=false;await loadFlow();message.success('分镜已移入该轨道')}
async function saveTrackPrompt(track:any) { await updateVideoTrackPrompt(track.id, track.prompt || ''); message.success('视频提示词已保存'); }
async function createVideoPrompt(track:any) { if (!project.value?.videoModel) return message.warning('请先配置视频模型'); track.prompt=await generateVideoPrompt({trackId:track.id,projectId:projectId.value,info:track.medias??[],model:project.value.videoModel,mode:videoMode.value}); message.success('视频提示词已生成'); }
let videoPollTimer: ReturnType<typeof setTimeout> | undefined;
function startVideoPolling() {
  if (videoPollTimer) clearTimeout(videoPollTimer);
  const refresh = async () => {
    if (!selectedScriptId.value) return;
    const generating = videoTracks.value.flatMap((track: any) =>
      (track.videoList ?? []).filter((video: any) => video.state === '生成中').map((video: any) => video.id),
    );
    if (!generating.length) {
      videoPollTimer = undefined;
      return;
    }
    try {
      const updates = await pollTrackVideos(projectId.value, selectedScriptId.value, generating);
      for (const video of updates) {
        if (video.state === '生成成功') {
          message.success({ content: `视频任务 ${video.id} 生成完成`, key: `video-${video.id}` });
        } else if (video.state === '生成失败') {
          message.error({ content: video.errorReason || `视频任务 ${video.id} 生成失败`, duration: 6, key: `video-${video.id}` });
        }
      }
      const updateMap = new Map(updates.map((video) => [video.id, video]));
      videoTracks.value = videoTracks.value.map((track: any) => ({
        ...track,
        videoList: (track.videoList ?? []).map((video: any) => ({
          ...video,
          ...(updateMap.get(video.id) ?? {}),
        })),
      }));
    } catch {
      // Keep polling; transient status failures should not hide the running task.
    }
    videoPollTimer = setTimeout(refresh, 2000);
  };
  videoPollTimer = setTimeout(refresh, 800);
}
async function generateVideo(track:any) {
  if (!selectedScriptId.value || !project.value?.videoModel) return message.warning('请先配置项目视频模型');
  const options = track.generation ?? {};
  const medias = [...(track.medias ?? [])];
  const id = await generateTrackVideo({ projectId:projectId.value,scriptId:selectedScriptId.value,trackId:track.id,prompt:track.prompt||'',model:options.model||project.value.videoModel,mode:options.mode||videoMode.value,resolution:options.resolution||'1080p',duration:options.duration||track.duration||5,audio:Boolean(options.audio),uploadData:medias });
  track.videoList = [{ id, state: '生成中', src: '', errorReason: undefined }, ...(track.videoList ?? [])];
  message.loading({ content: `轨道 ${track.id} 正在生成视频`, duration: 2, key: `video-${id}` });
  startVideoPolling();
}
async function generateAllVideoPrompts(){if(!project.value?.videoModel)return message.warning('请先配置视频模型');await batchGenerateVideoPrompts({projectId:projectId.value,model:project.value.videoModel,mode:videoMode.value,concurrentCount:5,trackData:videoTracks.value.map(track=>({trackId:track.id,info:track.medias??[]}))});message.success('批量提示词任务已提交');window.setTimeout(loadFlow,3000)}
async function generateAllVideos(){if(!selectedScriptId.value||!project.value?.videoModel)return message.warning('请先选择剧本并配置视频模型');await batchGenerateVideos({projectId:projectId.value,scriptId:selectedScriptId.value,model:project.value.videoModel,mode:videoMode.value,resolution:'1080p',audio:false,trackData:videoTracks.value.map(track=>({trackId:track.id,prompt:track.prompt||'',duration:track.duration||5,uploadData:track.medias??[]}))});message.success('批量视频任务已提交');window.setTimeout(loadFlow,3000)}
async function chooseVideo(track:any,video:any){await selectTrackVideo(track.id,video.id);track.selectVideoId=video.id;message.success('候选视频已选择')}
async function removeVideo(video:any){await deleteTrackVideo(video.id);await loadFlow()}
async function cancelVideo(video:any){await cancelTrackVideo(video.id);await loadFlow()}
async function retryVideo(video:any,track:any){if(!project.value?.videoModel)return message.warning('请先配置视频模型');const id=await retryTrackVideo({id:video.id,model:project.value.videoModel,mode:videoMode.value,resolution:'1080p',audio:false,uploadData:track.medias??[]});track.videoList=[{id,state:'生成中',src:'',errorReason:undefined},...(track.videoList??[])];message.loading({content:`视频任务 ${id} 正在重试`,duration:2,key:`video-${id}`});startVideoPolling()}
async function exportVideo(){if(!selectedScriptId.value)return message.warning('请先选择剧本');const result=await exportFinalVideo(projectId.value,selectedScriptId.value);message.success(`成片导出任务 ${result.taskId} 已提交，请到任务中心查看`)}

watch(selectedScriptId, () => {
  if (productionAgentSyncTimer) clearInterval(productionAgentSyncTimer);
  productionAgentSyncTimer = undefined;
  productionAgentActivity.value = '等待指令';
  productionChatMessages.value = [];
  loadFlow();
});

watch(activeTab, (tab) => {
  if (tab === 'script-agent') agentType.value = 'scriptAgent';
  if (tab === 'video' || tab === 'production') agentType.value = 'productionAgent';
  loadAgentMemory();
});

onMounted(loadAll);
onBeforeUnmount(() => {
  if (productionAssetRefreshTimer) clearTimeout(productionAssetRefreshTimer);
  if (storyboardPollTimer) clearTimeout(storyboardPollTimer);
  if (videoPollTimer) clearTimeout(videoPollTimer);
  if (productionAgentSyncTimer) clearInterval(productionAgentSyncTimer);
});
watch(projectId, () => loadAll());
</script>

<template>
  <Page auto-content-height>
    <Card
      :bordered="false"
      :loading="loading"
      class="toonflow-page-card h-full"
    >
      <template #title>
        <Space>
          <Button @click="router.back()">返回</Button>
          <Typography.Text strong>{{ project?.name || '项目详情' }}</Typography.Text>
          <Tag>{{ project?.videoRatio || '9:16' }}</Tag>
        </Space>
      </template>
      <template #extra>
        <Button @click="loadAll">刷新</Button>
      </template>

      <Row :gutter="16" class="mb-4">
        <Col :span="6"><Card size="small"><Statistic title="角色资产" :value="statistics.roleCount" /></Card></Col>
        <Col :span="6"><Card size="small"><Statistic title="剧本" :value="statistics.scriptCount" /></Card></Col>
        <Col :span="6"><Card size="small"><Statistic title="分镜" :value="statistics.storyboardCount" /></Card></Col>
        <Col :span="6"><Card size="small"><Statistic title="视频" :value="statistics.videoCount" /></Card></Col>
      </Row>

      <Tabs v-model:active-key="activeTab">
        <Tabs.TabPane key="novel" tab="原文">
          <div class="tab-tools">
            <Space>
              <Upload accept=".txt,.md,text/plain,text/markdown" :before-upload="importNovelFile" :show-upload-list="false">
                <Button type="primary">导入文件</Button>
              </Upload>
              <Button @click="openNovel()">手动导入</Button>
              <Button @click="extractAllNovelEvents">提取全部待处理事件</Button>
            </Space>
          </div>
          <Table
            class="novel-table"
            :columns="novelColumns"
            :data-source="novels"
            :scroll="{ x: 1408 }"
            :pagination="{ pageSize: 10, showSizeChanger: true, pageSizeOptions: ['5', '10', '20', '50'], showTotal: (total: number) => `共 ${total} 章` }"
            row-key="id"
            size="small"
          >
            <template #bodyCell="{ column, record }">
              <template v-if="column.dataIndex === 'eventState'">
                <Tag :color="record.eventState === 1 ? 'green' : record.eventState === -1 ? 'red' : 'default'">
                  {{ record.eventState === 1 ? '已提取' : record.eventState === -1 ? '失败' : '待提取' }}
                </Tag>
              </template>
              <template v-if="column.dataIndex === 'chapterData'">
                <span class="novel-cell-text" :title="record.chapterData || ''">
                  {{ record.chapterData || '暂无正文' }}
                </span>
              </template>
              <template v-if="column.dataIndex === 'event'">
                <span class="novel-cell-text" :title="formatEventDisplay(record.event)">
                  {{ formatEventDisplay(record.event) || '尚未提取事件' }}
                </span>
              </template>
              <template v-if="column.key === 'action'">
                <Space :size="4" class="novel-actions">
                  <Button size="small" type="link" @click="openNovel(record)">编辑</Button>
                  <Button size="small" type="link" @click="extractNovelEvents(record)">提取事件</Button>
                  <Popconfirm title="确认删除章节？" @confirm="removeNovel(record)">
                    <Button danger size="small" type="link">删除</Button>
                  </Popconfirm>
                </Space>
              </template>
            </template>
          </Table>
        </Tabs.TabPane>

        <Tabs.TabPane key="script-agent" tab="剧本创作">
          <Row :gutter="16">
            <Col :span="8">
              <AgentChat ref="agentChatRef" agent-type="scriptAgent" :project-id="projectId" :messages="scriptChatMessages" @tool-result="onAgentToolResult" />
            </Col>
            <Col :span="16">
              <Card class="workspace-card" size="small" title="剧本创作工作区">
                <template #extra><Space><Popconfirm title="确认清除剧本 Agent 的对话和工作区？" @confirm="resetAgentWorkspace"><Button>重新开始</Button></Popconfirm><Button @click="openScriptGeneration">开始创作</Button><Button type="primary" @click="saveAgentWorkspace">保存工作区</Button></Space></template>
                <Tabs v-model:active-key="workspaceActiveTab" size="small">
                  <Tabs.TabPane v-for="tab in workspaceTabs" :key="tab.key" :tab="tab.label">
                    <div v-if="!tab.content" class="workspace-placeholder">剧本 Agent 将在此展示{{ tab.label }}...</div>
                    <div v-else class="workspace-output" v-dompurify-html="renderMarkdown(tab.content)"></div>
                  </Tabs.TabPane>
                </Tabs>
              </Card>
            </Col>
          </Row>
        </Tabs.TabPane>

        <Tabs.TabPane key="script" tab="剧本与资产">
              <Card size="small" title="剧本与基础资产">
                <template #extra>
                  <Space>
                    <Button
                      size="small"
                      @click="router.push({ path: '/toonflow/assets', query: { projectId } })"
                    >
                      打开基础资产库（{{ assets.length }}）
                    </Button>
                    <Button size="small" @click="openScript()">手动新增</Button>
                  </Space>
                </template>
                <List :data-source="scripts" item-layout="vertical">
                  <template #renderItem="{ item }">
                    <List.Item>
                      <template #actions>
                        <Button type="link" @click="openScript(item)">编辑</Button>
                        <Button type="link" @click="extractAssetsFromScript(item)">AI 提取资产</Button>
                        <Popconfirm title="确认删除剧本？" @confirm="removeScript(item)">
                          <Button danger type="link">删除</Button>
                        </Popconfirm>
                      </template>
                      <List.Item.Meta :description="`${item.content.length} 字`" :title="item.name" />
                      <div class="script-preview">{{ item.content }}</div>
                    </List.Item>
                  </template>
                </List>
              </Card>
        </Tabs.TabPane>

        <Tabs.TabPane key="production" tab="分镜制作">
          <Row :gutter="16" class="production-layout">
            <Col :span="24">
              <Space class="production-tools mb-3" wrap>
                <Select
                  :value="selectedScriptId"
                  :options="scriptOptions"
                  placeholder="选择剧本"
                  style="width: 240px"
                  @change="changeProductionScript"
                />
                <Button @click="loadFlow">刷新制作数据</Button>
                <Button type="primary" @click="saveFlowText">保存分镜工作区</Button>
                <Button @click="openStoryboard()">新增分镜</Button>
                <Button type="primary" @click="generateAllStoryboardImages">批量生成分镜图</Button>
                <Button @click="previewAllStoryboardImages">合成预览</Button>
                <Button v-if="productionAgentCollapsed" @click="productionAgentCollapsed = false">展开 Agent</Button>
              </Space>
              <ProductionFlowCanvas
                ref="productionFlowCanvasRef"
                :assets="productionAssets"
                :flow-text="flowText"
                :script="selectedScript"
                :storyboard-busy="storyboardBusy"
                :storyboards="storyboards"
                :video-mode="videoMode"
                :video-model="project?.videoModel"
                :video-ratio="project?.videoRatio"
                :video-tracks="videoTracks"
                @cancel-track-video="cancelVideo"
                @delete-track-video="removeVideo"
                @batch-delete-storyboards="batchDeleteSelectedStoryboards"
                @edit-asset="openAssetImageFlow"
                @edit-storyboard="openStoryboard"
                @edit-storyboard-image="openStoryboardImageFlow"
                @export-storyboard-images="downloadSelectedStoryboardImages"
                @export-video="exportVideo"
                @generate-storyboards="generateStoryboards"
                @generate-track-video="generateVideo"
                @generate-video-prompt="createVideoPrompt"
                @insert-storyboard-after="beginInsertStoryboard"
                @open-video-track="openVideoTrack"
                @retry-track-video="retryVideo"
                @remove-storyboard="deleteStoryboard"
                @reorder-storyboards="saveStoryboardOrder"
                @save-positions="saveProductionCanvasPositions"
                @save-video-prompt="saveTrackPrompt"
                @select-track-video="chooseVideo"
                @update-flow-section="updateProductionFlowSection"
              />
            </Col>
            <Col v-if="!productionAgentCollapsed" :span="24" class="production-agent-panel">
              <Card class="production-agent-card" size="small" title="分镜制作 Agent">
                <template #extra>
                  <Space>
                    <Tag color="blue">{{ productionAgentActivity }}</Tag>
                    <Button size="small" @click="productionAgentCollapsed = true">折叠</Button>
                    <Button size="small" @click="resetProductionAgent">重新开始</Button>
                  </Space>
                </template>
                <div class="production-agent-context">
                  <Tag color="green">读取剧本与基础资产</Tag>
                </div>
                <AgentChat
                  ref="productionAgentChatRef"
                  agent-type="productionAgent"
                  :project-id="projectId"
                  :script-id="selectedScriptId"
                  :messages="productionChatMessages"
                  starter-label="开始制作视频"
                  starter-prompt="立即读取当前已有剧本和人物基础资产，从「人物衍生资产分析」开始制作。人物父资产都是白色基础内衣底模，必须逐一检查当前剧本中的每个角色，并为每个出场角色写入至少一套符合其身份和剧情的正式服装衍生；另行补充剧本明确出现的换装、重伤、变身或稳定形态变化。请直接调用 run_sub_agent_derive_assets 实际写入，不要重新生成剧本，不要只汇报状态，也不要在执行前询问是否开始。完成后展示衍生清单并暂停等待我确认生成。"
                  @activity="onProductionAgentActivity"
                  @clear-memory="clearProductionAgentMemory"
                  @tool-result="onAgentToolResult"
                  @workspace-preview="previewProductionFlowSection"
                />
              </Card>
            </Col>
          </Row>
        </Tabs.TabPane>
        <Tabs.TabPane key="video" tab="视频制作">
          <Card class="mb-3" size="small" title="视频生产 Agent">
            <template #extra><Space><Tag color="blue">按当前剧本连续记忆</Tag><Button @click="loadAgentMemory">恢复会话</Button></Space></template>
            <AgentChat
              ref="agentChatRef"
              agent-type="productionAgent"
              :project-id="projectId"
              :script-id="selectedScriptId"
              :messages="productionChatMessages"
              @tool-result="onAgentToolResult"
            />
          </Card>
          <Space class="mb-3"><Select v-model:value="selectedScriptId" :options="scriptOptions" placeholder="选择剧本" style="width:240px"/><Button type="primary" @click="createVideoTrack">新增轨道</Button><Button @click="generateAllVideoPrompts">批量提示词</Button><Button @click="generateAllVideos">批量视频</Button><Button type="primary" @click="exportVideo">导出成片</Button><Button @click="loadFlow">刷新状态</Button></Space>
          <Card v-for="(track,trackIndex) in videoTracks" :id="`video-track-${track.id}`" :key="track.id" class="mb-3 video-track-card" size="small">
            <template #title>轨道 {{ track.id }} · {{ track.duration || 0 }} 秒</template>
            <template #extra><Space><Button :disabled="trackIndex===0" @click="moveVideoTrack(trackIndex,-1)">上移</Button><Button :disabled="trackIndex===videoTracks.length-1" @click="moveVideoTrack(trackIndex,1)">下移</Button><Button @click="openTrackBinding(track)">调整分镜</Button><Button @click="createVideoPrompt(track)">AI 生成提示词</Button><Button @click="saveTrackPrompt(track)">保存提示词</Button><Button type="primary" @click="generateVideo(track)">生成视频</Button></Space></template>
            <Input.TextArea v-model:value="track.prompt" :rows="3" placeholder="视频生成提示词"/>
            <div class="mt-2"><Tag v-for="media in track.medias" :key="media.id">{{media.index}} · 分镜 {{media.id}}</Tag></div>
            <div class="mt-3 flex gap-3 overflow-x-auto"><div v-for="video in track.videoList" :key="video.id"><video v-if="video.src" :src="video.src" controls class="h-40 rounded border"/><Tag :color="video.state==='生成成功'?'green':video.state==='生成失败'?'red':video.state==='生成中'?'processing':'default'">{{video.state}}</Tag><div v-if="video.errorReason" class="max-w-80 whitespace-normal text-xs text-red-500">{{video.errorReason}}</div><Space><Button v-if="video.state==='生成成功'" size="small" type="link" @click="chooseVideo(track,video)">{{track.selectVideoId===video.id?'已选中':'选择'}}</Button><Button v-if="video.state==='生成中'" size="small" type="link" @click="cancelVideo(video)">取消</Button><Button v-if="['生成失败','已取消'].includes(video.state)" size="small" type="link" @click="retryVideo(video,track)">重试</Button><Button danger size="small" type="link" @click="removeVideo(video)">删除</Button></Space></div><Tag v-if="!track.videoList?.length">暂无候选视频</Tag></div>
          </Card>
          <Modal v-model:open="trackBindingOpen" :title="`调整轨道 ${trackBindingTarget?.id ?? ''} 的分镜`" width="720px" @ok="confirmTrackBinding">
            <p class="mb-3 text-gray-500">选中的分镜会从原轨道移入当前轨道，可多选。</p>
            <Select
              v-model:value="trackBindingStoryboardIds"
              mode="multiple"
              option-filter-prop="label"
              :options="storyboards.map((item:any)=>({ value:item.id, label:`${item.index ?? '-'} · ${item.videoDesc || item.prompt || `分镜 ${item.id}`}` }))"
              placeholder="选择要移入该轨道的分镜"
              style="width:100%"
            />
          </Modal>
        </Tabs.TabPane>

      </Tabs>
    </Card>

    <Modal v-model:open="novelModalOpen" title="章节" width="820px" @ok="saveNovel">
      <Form :label-col="{ span: 4 }">
        <Form.Item label="批量导入">
          <Input.TextArea
            v-model:value="batchNovelText"
            :rows="6"
            placeholder="可粘贴多章文本；留空则保存下方单章"
          />
        </Form.Item>
        <Form.Item label="序号">
          <InputNumber v-model:value="novelForm.index" :min="1" />
        </Form.Item>
        <Form.Item label="分卷">
          <Input v-model:value="novelForm.reel" />
        </Form.Item>
        <Form.Item label="章节">
          <Input v-model:value="novelForm.chapter" />
        </Form.Item>
        <Form.Item label="正文">
          <Input.TextArea v-model:value="novelForm.chapterData" :rows="8" />
        </Form.Item>
        <Form.Item label="事件">
          <Input.TextArea v-model:value="novelForm.event" :rows="3" />
        </Form.Item>
      </Form>
    </Modal>

    <Modal v-model:open="scriptModalOpen" title="剧本" width="900px" @ok="saveScript">
      <Form :label-col="{ span: 3 }">
        <Form.Item label="名称">
          <Input v-model:value="scriptForm.name" />
        </Form.Item>
        <Form.Item label="关联资产">
          <Select
            v-model:value="scriptForm.assets"
            :options="assetOptions"
            mode="multiple"
            placeholder="选择剧本使用的角色、场景、道具"
          />
        </Form.Item>
        <Form.Item label="生成图片">
          <Select
            v-model:value="scriptForm.shouldGenerateImage"
            :options="[
              { label: '需要生成', value: 1 },
              { label: '不生成', value: 0 },
            ]"
          />
        </Form.Item>
        <Form.Item label="内容">
          <Input.TextArea v-model:value="scriptForm.content" :rows="14" />
        </Form.Item>
      </Form>
    </Modal>

    <Modal v-model:open="storyboardModalOpen" :title="storyboardForm.id ? '编辑分镜' : '新增分镜'" width="820px" @ok="saveStoryboardForm">
      <Form :label-col="{ span: 4 }">
        <Form.Item label="轨道">
          <Input v-model:value="storyboardForm.track" />
        </Form.Item>
        <Form.Item label="时长">
          <InputNumber v-model:value="storyboardForm.duration" :min="1" />
        </Form.Item>
        <Form.Item label="关联资产">
          <Select
            v-model:value="storyboardForm.associateAssetsIds"
            :options="assetOptions"
            mode="multiple"
          />
        </Form.Item>
        <Form.Item label="画面描述">
          <Input.TextArea v-model:value="storyboardForm.videoDesc" :rows="4" />
        </Form.Item>
        <Form.Item label="图片提示词">
          <Input.TextArea v-model:value="storyboardForm.prompt" :rows="5" />
        </Form.Item>
      </Form>
    </Modal>
    <Modal v-model:open="flowImageModalOpen" :title="`${editingAssetName} · 图片生成流`" width="94vw" :footer="null">
      <ImageFlowEditor
        :key="imageFlowEditorKey"
        :asset-options="imageFlowAssetOptions"
        v-model:edges="imageFlowEdges"
        v-model:nodes="imageFlowNodes"
        :loading-node-id="generatingImageNodeId"
        @add="addImageFlowNode"
        @agent="planImageEditWithAgent"
        @connect="connectImageFlow"
        @generate="createFlowImage"
        @remove="removeImageFlowNode"
        @save="saveVisualImageFlow"
        @select-asset="selectImageFlowAsset"
        @upload="uploadImageFlowReference"
      />
    </Modal>
    <Modal v-model:open="storyboardPreviewOpen" title="分镜合成预览" width="1100px">
      <img v-if="storyboardPreview" :src="storyboardPreview" class="w-full rounded border" />
      <template #footer>
        <Button @click="storyboardPreviewOpen = false">关闭</Button>
        <Button type="primary" @click="downloadAllStoryboardImages">下载 PNG</Button>
      </template>
    </Modal>
  </Page>
</template>

<style scoped>
.tab-tools {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 12px;
}

.tab-tools :deep(.ant-space) {
  flex-wrap: wrap;
  justify-content: flex-end;
}

.novel-table :deep(.ant-table-cell) {
  vertical-align: middle;
}

.novel-cell-text {
  display: -webkit-box;
  overflow: hidden;
  color: var(--ant-color-text-secondary);
  line-height: 20px;
  overflow-wrap: anywhere;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

.novel-actions {
  white-space: nowrap;
}

.script-preview {
  color: #4b5563;
  display: -webkit-box;
  line-height: 22px;
  max-height: 66px;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 3;
}

.flow-editor {
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono',
    'Courier New', monospace;
  min-height: 520px;
}

.production-layout {
  position: relative;
  align-items: flex-start;
}

.production-tools {
  display: flex;
}

.production-flow-editor {
  min-height: 320px;
}

.production-agent-card {
  height: 100%;
  border-radius: 10px 0 0 10px;
  box-shadow: -8px 0 28px rgb(15 23 42 / 16%);
}

.production-agent-panel {
  position: absolute;
  z-index: 20;
  top: 52px;
  right: 0;
  width: min(420px, 92vw);
  height: calc(100% - 52px);
  padding: 0 !important;
}

.production-agent-card :deep(.ant-card-body) {
  padding: 10px;
}

.production-agent-context {
  margin-bottom: 8px;
}

@media (min-width: 1200px) {
  .production-layout:has(.production-agent-panel) :deep(.production-flow-shell) {
    margin-right: 436px;
  }
}

.production-agent-card :deep(.agent-chat-wrapper) {
  height: 600px;
}

.video-track-card {
  scroll-margin-top: 16px;
}

.video-track-card:target,
.video-track-card:focus-within {
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ant-color-primary) 25%, transparent);
}

@media (max-width: 1199px) {
  .production-agent-card :deep(.agent-chat-wrapper) {
    height: 520px;
  }
}

.mb-3 {
  margin-bottom: 12px;
}

.agent-chat {
  max-height: 560px;
  min-height: 300px;
  overflow: auto;
}

.agent-message-assistant {
  background: rgb(248 250 252);
}

.image-flow-canvas {
  display: flex;
  min-height: 280px;
  gap: 36px;
  overflow-x: auto;
  padding: 24px;
  border: 1px dashed var(--ant-color-border);
  border-radius: 8px;
  background: var(--ant-color-fill-quaternary);
}

.image-flow-node {
  position: relative;
  min-width: 280px;
  max-width: 280px;
}

.image-flow-arrow {
  position: absolute;
  top: 50%;
  right: -30px;
  color: var(--ant-color-primary);
  font-size: 24px;
}

.image-flow-empty {
  display: flex;
  height: 160px;
  align-items: center;
  justify-content: center;
  color: var(--ant-color-text-tertiary);
}
.event-text {
  font-size: 12px;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.workspace-card {
  height: 680px;
  display: flex;
  flex-direction: column;
}
/* Pipeline progress bar */
.pipeline-bar {
  display: flex;
  align-items: center;
  padding: 8px 4px 12px;
  gap: 0;
  border-bottom: 1px solid var(--ant-color-border-secondary);
  margin-bottom: 4px;
}
.pipeline-step {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  white-space: nowrap;
}
.pipeline-step.pending { color: var(--ant-color-text-quaternary); }
.pipeline-step.active { color: var(--ant-color-primary); font-weight: 600; }
.pipeline-step.completed { color: #52c41a; }
.pipeline-step.review { color: #faad14; }
.pipeline-dot { font-size: 14px; width: 18px; text-align: center; flex-shrink: 0; }
.pipeline-label { flex-shrink: 0; }
.pipeline-line {
  flex: 1;
  height: 2px;
  min-width: 12px;
  background: #e8e8e8;
  margin: 0 4px;
}
.pipeline-step.completed .pipeline-line { background: #52c41a; }
.pipeline-step.active .pipeline-line { background: var(--ant-color-primary); }
.workspace-card :deep(.ant-card-body) {
  padding: 8px 12px;
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.workspace-card :deep(.ant-tabs) {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.workspace-card :deep(.ant-tabs-content-holder) {
  flex: 1;
  overflow: hidden;
}
.workspace-card :deep(.ant-tabs-content) {
  height: 100%;
  overflow-y: auto;
}
.workspace-pane {
  height: 100%;
  min-height: 0;
}
.workspace-placeholder {
  color: var(--ant-color-text-quaternary);
  font-size: 13px;
  padding: 16px 0;
  text-align: center;
}
.workspace-output {
  font-size: 13px;
  line-height: 1.8;
  white-space: pre-wrap;
  word-break: break-word;
}
.workspace-output :deep(h2) { font-size: 16px; margin: 10px 0 6px; border-bottom: 1px solid var(--ant-color-border-secondary); padding-bottom: 4px; }
.workspace-output :deep(h3) { font-size: 15px; margin: 8px 0 4px; }
.workspace-output :deep(h4) { font-size: 14px; margin: 6px 0 3px; color: var(--ant-color-primary); }
.workspace-output :deep(strong) { font-weight: 600; }
.workspace-output :deep(li) { margin-left: 16px; }
.workspace-output :deep(p) { margin: 4px 0; }
.workspace-output :deep(hr) { border: none; border-top: 1px dashed #e8e8e8; margin: 12px 0; }

</style>
