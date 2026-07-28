<script lang="ts" setup>
import type { Edge, Node } from '@vue-flow/core';
import type { ToonflowApi } from '#/api/toonflow';

import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { Handle, Position, VueFlow } from '@vue-flow/core';
import { Button, Empty, Input, Modal, Tag } from 'ant-design-vue';

import { MarkdownView } from '#/components/markdown-view';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';

import ProductionAssetStrip from './ProductionAssetStrip.vue';
import { normalizeProductionDocument } from './production-flow-document';
import StoryboardPanel from './StoryboardPanel.vue';
import VideoWorkbenchPanel from './VideoWorkbenchPanel.vue';

const props = defineProps<{
  assets: ToonflowApi.Asset[];
  storyboardBusy?: boolean;
  flowText: string;
  script?: ToonflowApi.Script;
  storyboards: ToonflowApi.Storyboard[];
  videoMode?: string;
  videoModel?: number;
  videoRatio?: string;
  videoTracks: any[];
}>();

const emit = defineEmits<{
  batchDeleteStoryboards: [ids: number[]];
  cancelTrackVideo: [video: any];
  deleteTrackVideo: [video: any];
  editAsset: [asset: ToonflowApi.Asset];
  editStoryboard: [storyboard: ToonflowApi.Storyboard];
  editStoryboardImage: [storyboard: ToonflowApi.Storyboard];
  exportStoryboardImages: [ids: number[]];
  exportVideo: [];
  generateStoryboards: [ids: number[], compulsory: boolean];
  insertStoryboardAfter: [storyboard: ToonflowApi.Storyboard];
  generateTrackVideo: [track: any];
  generateVideoPrompt: [track: any];
  openVideoTrack: [trackId: number];
  retryTrackVideo: [video: any, track: any];
  savePositions: [positions: Record<string, { x: number; y: number }>];
  saveVideoPrompt: [track: any];
  selectTrackVideo: [track: any, video: any];
  updateFlowSection: [key: 'scriptPlan' | 'storyboardTable', value: string];
  removeStoryboard: [storyboard: ToonflowApi.Storyboard];
  reorderStoryboards: [ids: number[]];
}>();

const flowInstance = ref<any>();
const spacePressed = ref(false);
const editorOpen = ref(false);
const workbenchOpen = ref(false);
const editorKey = ref<'scriptPlan' | 'storyboardTable'>('scriptPlan');
const editorValue = ref('');
const flowData = computed<Record<string, any>>(() => {
  try {
    return JSON.parse(props.flowText || '{}');
  } catch {
    return {};
  }
});

const directorPlan = computed(() =>
  normalizeProductionDocument(
    flowData.value.scriptPlan || flowData.value.directorPlan,
    'scriptPlan',
  ),
);
const storyboardPlan = computed(() =>
  normalizeProductionDocument(flowData.value.storyboardTable, 'storyboardTable'),
);
const workbenchCover = computed(() => {
  for (const track of props.videoTracks) {
    const videos = track.videoList ?? track.video_list ?? [];
    const selectedId = track.selectVideoId ?? track.select_video_id ?? track.videoId ?? track.video_id;
    const selected = videos.find((video: any) => video.id === selectedId);
    const available = selected ?? videos.find((video: any) => video.src);
    if (available?.src) return available.src;
  }
  return '';
});
const defaultPositions = {
  script: { x: 0, y: 0 },
  scriptPlan: { x: 1000, y: 0 },
  storyboardTable: { x: 2000, y: 0 },
  storyboard: { x: 3000, y: 0 },
  workbench: { x: 4000, y: 0 },
};
const savedPositions = computed(() =>
  flowData.value.canvas?.layoutVersion === 7 ? flowData.value.canvas.positions || {} : {},
);
const nodes = computed<Node[]>(() => [
  ...Object.entries(defaultPositions).map(([id, position]) => ({
    id, type: id, dragHandle: '.dragHandle', position: savedPositions.value[id] || position, data: {},
  })),
]);

const edges: Edge[] = [
  { id: 'script-plan', source: 'script', target: 'scriptPlan', animated: false, style: { stroke: '#000', strokeWidth: 4 } },
  { id: 'plan-table', source: 'scriptPlan', target: 'storyboardTable', animated: false, style: { stroke: '#000', strokeWidth: 4 } },
  { id: 'table-panel', source: 'storyboardTable', target: 'storyboard', animated: false, style: { stroke: '#000', strokeWidth: 4 } },
  { id: 'panel-workbench', source: 'storyboard', target: 'workbench', animated: false, style: { stroke: '#000', strokeWidth: 4 } },
];

function openEditor(key: 'scriptPlan' | 'storyboardTable') {
  editorKey.value = key;
  editorValue.value = key === 'scriptPlan' ? directorPlan.value : storyboardPlan.value;
  editorOpen.value = true;
}
function saveEditor() {
  emit('updateFlowSection', editorKey.value, editorValue.value);
  editorOpen.value = false;
}
function saveNodePositions(event: any) {
  const positions: Record<string, { x: number; y: number }> = {
    ...defaultPositions,
    ...savedPositions.value,
  };
  for (const node of flowInstance.value?.getNodes?.() || []) {
    positions[node.id] = { x: node.position.x, y: node.position.y };
  }
  for (const node of event?.nodes || []) {
    positions[node.id] = { x: node.position.x, y: node.position.y };
  }
  if (event?.node) positions[event.node.id] = event.node.position;
  emit('savePositions', positions);
}

function focusStage(stageId: string) {
  void flowInstance.value?.fitView({
    duration: 350,
    maxZoom: 1,
    nodes: [{ id: stageId }],
    padding: 0.12,
  });
}

defineExpose({ focusStage });

function handleSpaceDown(event: KeyboardEvent) {
  if (event.code !== 'Space' || event.repeat) return;
  const target = event.target as HTMLElement | null;
  if (target?.matches('input, textarea, [contenteditable="true"]')) return;
  event.preventDefault();
  spacePressed.value = true;
}
function handleSpaceUp(event: KeyboardEvent) {
  if (event.code === 'Space') spacePressed.value = false;
}
onMounted(() => {
  window.addEventListener('keydown', handleSpaceDown);
  window.addEventListener('keyup', handleSpaceUp);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleSpaceDown);
  window.removeEventListener('keyup', handleSpaceUp);
});
</script>

<template>
  <div class="production-flow-shell" :class="{ 'space-panning': spacePressed }">
    <VueFlow
      :nodes="nodes"
      :edges="edges"
      :min-zoom="0.35"
      :max-zoom="1.4"
      :default-viewport="{ x: 20, y: 20, zoom: 0.68 }"
      :pan-on-scroll="true"
      :zoom-on-scroll="false"
      :nodes-draggable="!spacePressed"
      :pan-on-drag="spacePressed ? [0] : true"
      :nodes-connectable="false"
      class="production-flow"
      @init="flowInstance = $event"
      @node-drag-stop="saveNodePositions"
    >
      <Background :gap="18" :size="1" pattern-color="#cbd5e1" />
      <Controls position="bottom-left" />

      <template #node-script>
        <section class="flow-stage-node stage-script">
          <Handle type="source" :position="Position.Right" />
          <header class="stage-header dragHandle">
            <div><span class="stage-index">1</span> 剧本与衍生资产</div>
            <Tag :color="script ? 'green' : 'default'">{{ script ? '已就绪' : '等待选择' }}</Tag>
          </header>
          <div v-if="script" class="script-body">
            <h3>{{ script.name }}</h3>
            <pre>{{ script.content }}</pre>
          </div>
          <Empty v-else :image="Empty.PRESENTED_IMAGE_SIMPLE" description="请先选择需要制作的剧本" />
          <section class="embedded-assets" :class="{ waiting: !script }">
            <header class="embedded-assets-header">
              <div>衍生资产</div>
              <Tag :color="assets.length ? 'green' : 'gold'">
                {{ assets.length ? '资产已载入' : '等待 Agent 分析' }}
              </Tag>
            </header>
            <ProductionAssetStrip :assets="assets" @edit="emit('editAsset', $event)" />
          </section>
        </section>
      </template>

      <template #node-scriptPlan>
        <section class="flow-stage-node stage-text stage-director" :class="{ waiting: !directorPlan }">
          <Handle type="target" :position="Position.Left" />
          <Handle type="source" :position="Position.Right" />
          <header class="stage-header dragHandle">
            <div>导演规划</div>
            <Button size="small" type="link" @click="openEditor('scriptPlan')">编辑</Button>
          </header>
          <div v-if="directorPlan" class="director-content">
            <MarkdownView :content="directorPlan" />
          </div>
          <div v-else class="stage-waiting">Agent 将根据剧本和人物资产生成导演规划</div>
        </section>
      </template>

      <template #node-storyboardTable>
        <section class="flow-stage-node stage-table" :class="{ waiting: !storyboardPlan }">
          <Handle type="target" :position="Position.Left" />
          <Handle type="source" :position="Position.Right" />
          <header class="stage-header dragHandle">
            <div>分镜表</div>
            <Button size="small" type="link" @click="openEditor('storyboardTable')">编辑</Button>
          </header>
          <div v-if="storyboardPlan" class="storyboard-table-content">
            <MarkdownView :content="storyboardPlan" />
          </div>
          <div v-else class="stage-waiting">导演规划确认后，Agent 将构建结构化分镜表</div>
        </section>
      </template>

      <template #node-storyboard>
        <section class="flow-stage-node stage-storyboard" :class="{ waiting: !storyboards.length }">
          <Handle type="target" :position="Position.Left" />
          <Handle type="source" :position="Position.Right" />
          <header class="stage-header dragHandle">
            <div>分镜面板</div>
          </header>
          <StoryboardPanel
            :busy="storyboardBusy"
            :storyboards="storyboards"
            @batch-delete="emit('batchDeleteStoryboards', $event)"
            @edit="emit('editStoryboard', $event)"
            @edit-image="emit('editStoryboardImage', $event)"
            @export-images="emit('exportStoryboardImages', $event)"
            @generate="(ids, compulsory) => emit('generateStoryboards', ids, compulsory)"
            @insert-after="emit('insertStoryboardAfter', $event)"
            @open-track="emit('openVideoTrack', $event)"
            @remove="emit('removeStoryboard', $event)"
            @reorder="emit('reorderStoryboards', $event)"
          />
        </section>
      </template>

      <template #node-workbench>
        <section class="flow-stage-node stage-workbench" @click.stop="workbenchOpen = true">
          <Handle type="target" :position="Position.Left" />
          <header class="stage-header dragHandle">
            <div>视频工作台</div>
          </header>
          <div class="workbench-preview">
            <video v-if="workbenchCover" :src="workbenchCover" muted preload="metadata" />
            <div class="workbench-play" aria-hidden="true"><span /></div>
          </div>
        </section>
      </template>
    </VueFlow>
    <Modal
      v-model:open="workbenchOpen"
      :closable="false"
      :footer="null"
      :keyboard="true"
      :mask-closable="false"
      :style="{ top: '0', paddingBottom: '0' }"
      :width="'100%'"
      wrap-class-name="toonflow-workbench-modal"
    >
      <VideoWorkbenchPanel
        :assets="assets"
        :storyboards="storyboards"
        :tracks="videoTracks"
        :video-mode="videoMode"
        :video-model="videoModel"
        :video-ratio="videoRatio"
        @close="workbenchOpen = false"
        @cancel-video="emit('cancelTrackVideo', $event)"
        @delete-video="emit('deleteTrackVideo', $event)"
        @generate-prompt="emit('generateVideoPrompt', $event)"
        @generate-video="emit('generateTrackVideo', $event)"
        @open-track="emit('openVideoTrack', $event)"
        @retry-video="(video, track) => emit('retryTrackVideo', video, track)"
        @reorder-storyboards="emit('reorderStoryboards', $event)"
        @save-prompt="emit('saveVideoPrompt', $event)"
        @select-video="(track, video) => emit('selectTrackVideo', track, video)"
        @export-storyboard-images="emit('exportStoryboardImages', $event)"
        @export-video="emit('exportVideo')"
      />
    </Modal>
    <Modal v-model:open="editorOpen" :title="editorKey === 'scriptPlan' ? '编辑导演规划' : '编辑分镜表'" width="90vw" @ok="saveEditor">
      <Input.TextArea v-model:value="editorValue" :auto-size="{ minRows: 18, maxRows: 32 }" />
    </Modal>
  </div>
</template>

<style scoped>
.production-flow-shell {
  position: relative;
  height: 100%;
  min-height: 720px;
  overflow: hidden;
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 10px;
  background: var(--ant-color-bg-layout);
}

.production-flow {
  width: 100%;
  height: 100%;
}
.production-flow-shell.space-panning,
.production-flow-shell.space-panning * { cursor: grab !important; }

.flow-stage-node {
  width: 520px;
  overflow: hidden;
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 12px;
  background: var(--ant-color-bg-container);
  box-shadow: 0 8px 24px rgb(15 23 42 / 8%);
}

.stage-header {
  border-bottom: 0;
}

.stage-header > div:first-child {
  width: fit-content;
  padding: 5px 10px;
  border-radius: 8px 0;
  color: #fff;
  background: #000;
}

.flow-stage-node.waiting {
  border-style: dashed;
}

.stage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 0;
  font-weight: 600;
}

.simple-header {
  border-bottom: 0;
}

.stage-index {
  display: inline-flex;
  width: 24px;
  height: 24px;
  align-items: center;
  justify-content: center;
  margin-right: 8px;
  border-radius: 50%;
  color: #fff;
  font-size: 12px;
  background: var(--ant-color-primary);
}

.stage-script {
  width: 920px;
  max-width: 920px;
  min-height: 250px;
}

.embedded-assets {
  border-top: 1px solid var(--ant-color-border-secondary);
}

.embedded-assets-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px 4px;
  font-weight: 600;
}

.embedded-assets :deep(.production-assets) {
  border: 0;
  border-radius: 0;
}

.embedded-assets :deep(.production-assets-heading) {
  display: none;
}

.embedded-assets :deep(.production-assets-canvas) {
  width: 100%;
  max-height: none;
  overflow: visible;
}

.stage-director {
  width: fit-content;
  min-width: 200px;
  max-width: 920px;
  overflow: visible;
}

.director-content {
  min-width: 520px;
  margin-top: 8px;
  padding: 0 16px 16px;
  user-select: text;
}

.director-content :deep(.markdown-view) {
  width: auto;
  max-width: 888px;
}

.stage-table {
  width: fit-content;
  min-width: 200px;
  max-width: 920px;
  overflow: visible;
}

.storyboard-table-content {
  min-width: 680px;
  margin-top: 8px;
  padding: 0 16px 16px;
  user-select: text;
}

.storyboard-table-content :deep(.markdown-view) {
  width: auto;
  max-width: 888px;
}

.director-content :deep(.markdown-view),
.storyboard-table-content :deep(.markdown-view) {
  color: var(--ant-color-text);
  font-family: inherit;
  font-size: 14px;
  line-height: 1.7;
}

.director-content :deep(.markdown-view > :first-child),
.storyboard-table-content :deep(.markdown-view > :first-child) {
  margin-top: 0;
}

.director-content :deep(.markdown-view h1),
.director-content :deep(.markdown-view h2),
.director-content :deep(.markdown-view h3),
.storyboard-table-content :deep(.markdown-view h1),
.storyboard-table-content :deep(.markdown-view h2),
.storyboard-table-content :deep(.markdown-view h3) {
  margin: 16px 0 8px;
  color: var(--ant-color-text);
  font-weight: 600;
  line-height: 1.45;
}

.director-content :deep(.markdown-view p),
.storyboard-table-content :deep(.markdown-view p) {
  margin: 0 0 8px;
}

.director-content :deep(.markdown-view ul),
.director-content :deep(.markdown-view ol),
.storyboard-table-content :deep(.markdown-view ul),
.storyboard-table-content :deep(.markdown-view ol) {
  margin: 0 0 10px;
  padding-left: 24px;
  color: var(--ant-color-text);
  font-size: 14px;
  line-height: 1.7;
}

.director-content :deep(.markdown-view li),
.storyboard-table-content :deep(.markdown-view li) {
  margin: 2px 0;
}

.storyboard-table-content :deep(.markdown-view table) {
  display: table;
  width: 100%;
  margin: 8px 0 12px;
  border-collapse: collapse;
  table-layout: auto;
}

.storyboard-table-content :deep(.markdown-view th),
.storyboard-table-content :deep(.markdown-view td) {
  padding: 8px 10px;
  border: 1px solid var(--ant-color-border-secondary);
  vertical-align: top;
  text-align: left;
  white-space: normal;
  overflow-wrap: anywhere;
}

.storyboard-table-content :deep(.markdown-view th) {
  background: var(--ant-color-fill-quaternary);
  font-weight: 600;
  white-space: nowrap;
}

.storyboard-table-content :deep(.markdown-view tr:nth-child(even) td) {
  background: color-mix(in srgb, var(--ant-color-fill-quaternary) 55%, transparent);
}

.stage-storyboard {
  width: 920px;
  max-width: 920px;
}
.stage-workbench {
  width: 320px;
  min-width: 280px;
  cursor: pointer;
  transition: filter 0.1s ease;
}
.stage-workbench:active { filter: brightness(0.92); }
.workbench-preview {
  position: relative;
  display: grid;
  width: calc(100% - 24px);
  overflow: hidden;
  aspect-ratio: 16 / 9;
  margin: 0 12px 12px;
  border-radius: 8px;
  background: var(--ant-color-fill-secondary);
  place-items: center;
}
.workbench-preview video { width: 100%; height: 100%; object-fit: cover; }
.workbench-play {
  position: absolute;
  display: grid;
  width: 58px;
  height: 58px;
  border-radius: 50%;
  background: rgb(0 0 0 / 42%);
  place-items: center;
  transition: transform 0.1s ease;
}
.workbench-play span {
  width: 0;
  height: 0;
  margin-left: 5px;
  border-top: 12px solid transparent;
  border-bottom: 12px solid transparent;
  border-left: 19px solid #fff;
}
.stage-workbench:hover .workbench-play { transform: scale(1.08); }
.dragHandle { cursor: grab; }
.dragHandle:active { cursor: grabbing; }

.script-body,
.stage-text,
.stage-storyboard {
  min-height: 220px;
}

.script-body {
  padding: 12px 16px 16px;
}

.script-body pre {
  max-height: none;
  overflow: visible;
}

.script-body h3 {
  margin: 0 0 8px;
  font-size: 15px;
}

pre {
  max-height: 220px;
  overflow: auto;
  margin: 0;
  padding: 14px 16px;
  color: var(--ant-color-text-secondary);
  font-family: inherit;
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
}

.stage-waiting {
  display: flex;
  min-height: 150px;
  align-items: center;
  justify-content: center;
  color: var(--ant-color-text-tertiary);
}

.storyboard-list {
  max-height: 300px;
  overflow: auto;
  padding: 10px 14px 14px;
}

.storyboard-grid {
  display: grid;
  max-height: 700px;
  overflow: auto;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  padding: 12px;
}

.storyboard-card {
  overflow: hidden;
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 7px;
  background: var(--ant-color-bg-container);
}

.storyboard-cover {
  position: relative;
  display: flex;
  height: 110px;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  color: var(--ant-color-text-tertiary);
  font-size: 12px;
  background: var(--ant-color-fill-quaternary);
}

.storyboard-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.storyboard-index {
  position: absolute;
  top: 5px;
  left: 5px;
  margin: 0;
}

.storyboard-card-body {
  padding: 7px 8px;
  font-size: 11px;
}

.storyboard-card-body p {
  display: -webkit-box;
  overflow: hidden;
  margin: 4px 0 0;
  color: var(--ant-color-text-tertiary);
  line-height: 15px;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

.storyboard-item {
  display: grid;
  grid-template-columns: 50px minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
  border-bottom: 1px solid var(--ant-color-border-secondary);
  font-size: 12px;
}

.storyboard-item span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

:deep(.vue-flow__edge-path) {
  stroke: var(--ant-color-primary);
  stroke-width: 2;
}

:deep(.vue-flow__handle) {
  width: 10px;
  height: 10px;
  border: 2px solid var(--ant-color-bg-container);
  background: var(--ant-color-primary);
}
</style>

<style>
.toonflow-workbench-modal { overflow: hidden; }
.toonflow-workbench-modal .ant-modal { width: 100vw !important; max-width: 100vw; margin: 0; padding: 0; inset-inline: 0; }
.toonflow-workbench-modal .ant-modal-content { width: 100vw; max-width: 100vw; height: 100vh; padding: 0; overflow: hidden; border-radius: 0; box-sizing: border-box; }
.toonflow-workbench-modal .ant-modal-body { width: 100%; max-width: 100vw; height: 100%; padding: 0; overflow: hidden; box-sizing: border-box; }
</style>
