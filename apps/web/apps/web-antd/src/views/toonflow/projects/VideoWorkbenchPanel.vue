<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue';

import { AiModelTypeEnum } from '@vben/constants';

import {
  Button,
  Empty,
  Input,
  InputNumber,
  Modal,
  Select,
  Slider,
  Switch,
  Tag,
  Tooltip,
} from 'ant-design-vue';

import { assetFileUrl } from '../assets/asset-types';
import { getModelSimpleList } from '#/api/ai/model/model';
import StoryboardQuickPreview from './StoryboardQuickPreview.vue';

const props = defineProps<{
  assets: any[];
  storyboards: any[];
  tracks: any[];
  videoMode?: string;
  videoModel?: number;
  videoRatio?: string;
}>();

const emit = defineEmits<{
  cancelVideo: [video: any];
  deleteVideo: [video: any];
  generatePrompt: [track: any];
  generateVideo: [track: any];
  openTrack: [trackId: number];
  retryVideo: [video: any, track: any];
  reorderStoryboards: [ids: number[]];
  savePrompt: [track: any];
  selectVideo: [track: any, video: any];
  exportStoryboardImages: [ids: number[]];
  exportVideo: [];
  close: [];
}>();

const activeTrackId = ref<number>();
const activeTab = ref('preview');
const previewVideo = ref<any>();
const addReferenceOpen = ref(false);
const videoModelOptions = ref<Array<{ label: string; value: number }>>([]);
const activeEditorIndex = ref(0);
const playingSequence = ref(false);
const editorVolume = ref(100);
const editorPlayer = ref<HTMLVideoElement>();

const activeTrack = computed(() =>
  props.tracks.find((track) => track.id === activeTrackId.value) ?? props.tracks[0],
);
const selectedClips = computed(() =>
  props.tracks
    .map((track, index) => {
      const videos = track.videoList ?? [];
      const video =
        videos.find((item: any) => item.id === track.selectVideoId) ??
        videos.find((item: any) => ['生成成功', '已完成'].includes(item.state));
      return video?.src
        ? { ...video, src: assetFileUrl(video.src), duration: track.duration || 5, index, trackId: track.id }
        : undefined;
    })
    .filter(Boolean),
);
const totalDuration = computed(() =>
  selectedClips.value.reduce((sum, clip: any) => sum + Number(clip.duration || 0), 0),
);
const activeEditorClip = computed(() => selectedClips.value[activeEditorIndex.value]);
const activePreviewVideo = computed(() => {
  const videos = activeTrack.value?.videoList ?? [];
  return videos.find((video: any) => video.id === activeTrack.value?.selectVideoId && video.src)
    ?? videos.find((video: any) => video.src);
});
const referenceAssets = computed(() => props.assets.filter((asset) => previewUrl(asset)));

function generation(track: any) {
  syncAssetReferences(track);
  track.generation ??= {
    audio: false,
    duration: track.duration || 5,
    mode: props.videoMode || 'text',
    model: props.videoModel,
    resolution: '1080p',
  };
  return track.generation;
}

function syncAssetReferences(track: any) {
  const medias = track.medias ?? (track.medias = []);
  const storyboardIds = new Set(
    medias.filter((media: any) => media.sources === 'storyboard').map((media: any) => Number(media.id)),
  );
  const assetIds = new Set(
    props.storyboards
      .filter((storyboard) => storyboardIds.has(Number(storyboard.id)))
      .flatMap((storyboard) => storyboard.associateAssetsIds ?? [])
      .map(Number),
  );
  for (const asset of props.assets.filter((item) => assetIds.has(Number(item.id)))) addReferenceToTrack(track, asset);
}

function activateTrack(track: any) {
  activeTrackId.value = track.id;
}

function mediaName(media: any, index: number) {
  if (media.name) return media.name;
  if (media.sources === 'storyboard') return `分镜 ${media.index ?? media.id ?? index + 1}`;
  return media.fileType === 'audio' ? `音频 ${index + 1}` : `参考素材 ${index + 1}`;
}

function mediaUrl(media: any) {
  return assetFileUrl(media?.src || media?.imageFilePath || media?.filePath || media?.fileUrl);
}

function previewUrl(item: any) {
  return assetFileUrl(item?.imageFilePath || item?.filePath || item?.fileUrl || item?.src);
}

function addReferenceToTrack(track: any, asset: any) {
  const medias = track.medias ?? (track.medias = []);
  if (medias.some((media: any) => media.sources === 'assets' && Number(media.id) === Number(asset.id))) return;
  medias.push({ id: asset.id, name: asset.name, src: previewUrl(asset), fileType: 'image', sources: 'assets' });
}

function addReference(asset: any) {
  if (activeTrack.value) addReferenceToTrack(activeTrack.value, asset);
}

function stateColor(state?: string) {
  if (state === '生成成功' || state === '已完成') return 'green';
  if (state === '生成中') return 'processing';
  if (state === '生成失败') return 'red';
  return 'default';
}

function playSequence() {
  if (!selectedClips.value.length) return;
  activeEditorIndex.value = 0;
  playingSequence.value = true;
}

function handleEditorEnded() {
  if (!playingSequence.value) return;
  if (activeEditorIndex.value < selectedClips.value.length - 1) activeEditorIndex.value += 1;
  else playingSequence.value = false;
}

function syncEditorVolume() {
  if (editorPlayer.value) editorPlayer.value.volume = editorVolume.value / 100;
}

watch(selectedClips, (clips) => {
  activeEditorIndex.value = Math.min(activeEditorIndex.value, Math.max(0, clips.length - 1));
});
watch(editorVolume, (volume) => {
  if (editorPlayer.value) editorPlayer.value.volume = volume / 100;
});

onMounted(async () => {
  try {
    const models = await getModelSimpleList(AiModelTypeEnum.VIDEO);
    videoModelOptions.value = models.map((model) => ({ label: model.name || model.model, value: model.id }));
  } catch {
    if (props.videoModel) videoModelOptions.value = [{ label: `项目模型 #${props.videoModel}`, value: props.videoModel }];
  }
});
</script>

<template>
  <div class="toonflow-workbench-shell">
    <header class="workbench-topbar">
      <Button class="close-workbench" type="text" aria-label="关闭视频工作台" @click="emit('close')">×</Button>
      <nav class="workbench-tabs" aria-label="视频工作台功能">
        <Tooltip title="分镜图预览">
          <button class="workbench-tab" :class="{ 'workbench-tab--active': activeTab === 'preview' }" type="button" @click="activeTab = 'preview'"><span>▣</span><b>分镜图预览</b></button>
        </Tooltip>
        <Tooltip title="轨道生成">
          <button class="workbench-tab" :class="{ 'workbench-tab--active': activeTab === 'generate' }" type="button" @click="activeTab = 'generate'"><span>▶</span><b>轨道生成</b></button>
        </Tooltip>
        <Tooltip title="剪辑台">
          <button class="workbench-tab" :class="{ 'workbench-tab--active': activeTab === 'editor' }" type="button" @click="activeTab = 'editor'"><span>✂</span><b>剪辑台</b></button>
        </Tooltip>
      </nav>
    </header>

    <div class="workbench-content">
      <StoryboardQuickPreview
        v-if="activeTab === 'preview'"
        :assets="assets"
        :storyboards="storyboards"
        @export-images="emit('exportStoryboardImages', $event)"
        @reorder="emit('reorderStoryboards', $event)"
      />

      <div v-else-if="activeTab === 'generate'" class="generation-page">
        <template v-if="activeTrack">
          <section class="generation-main">
            <div class="generation-player" :class="{ 'has-video': activePreviewVideo?.src }">
              <video v-if="activePreviewVideo?.src" :src="assetFileUrl(activePreviewVideo.src)" controls preload="metadata" />
              <div v-else class="player-placeholder"><span>▶</span><p>生成视频后在这里预览</p></div>
            </div>

            <aside class="generation-settings">
              <section class="setting-block prompt-block">
                <div class="section-heading"><div><b>视频提示词</b><span>描述镜头运动与画面变化</span></div><Button size="small" @click="emit('generatePrompt', activeTrack)">AI 生成</Button></div>
                <Input.TextArea v-model:value="activeTrack.prompt" :auto-size="{ minRows: 10, maxRows: 16 }" placeholder="输入视频提示词…" />
              </section>

              <section class="setting-block">
                <div class="section-heading"><div><b>参考素材</b><span>资产、分镜与首尾帧</span></div><Button size="small" @click="addReferenceOpen = true">＋ 添加图片</Button></div>
                <div v-if="activeTrack.medias?.length" class="media-strip">
                  <article v-for="(media, index) in activeTrack.medias" :key="`${media.sources}-${media.id}-${index}`" class="media-card">
                    <div class="media-preview">
                      <img v-if="media.fileType === 'image' && mediaUrl(media)" :src="mediaUrl(media)" :alt="mediaName(media, Number(index))" class="media-image" />
                      <div v-else-if="media.fileType === 'audio'" class="audio-preview">♪</div>
                      <div v-else class="empty-media">暂无图片</div>
                      <span class="media-order">{{ Number(index) + 1 }}</span><span class="media-source">{{ media.sources === 'storyboard' ? '分镜' : '资产' }}</span>
                    </div>
                  </article>
                </div>
                <Empty v-else :image="Empty.PRESENTED_IMAGE_SIMPLE" description="暂无参考素材" />
              </section>

              <section class="setting-block parameter-block">
                <div class="section-heading"><div><b>生成参数</b><span>{{ videoRatio || '16:9' }}</span></div></div>
                <div class="parameter-grid">
                  <label><span>视频模型</span><Select v-model:value="generation(activeTrack).model" :options="videoModelOptions" placeholder="选择视频模型" size="small" /></label>
                  <label><span>生成模式</span><Select v-model:value="generation(activeTrack).mode" :options="[{label:'纯文本',value:'text'},{label:'单图首帧',value:'singleImage'},{label:'首尾帧',value:'startEndRequired'},{label:'尾帧可选',value:'endFrameOptional'},{label:'首帧可选',value:'startFrameOptional'}]" size="small" /></label>
                  <label><span>分辨率</span><Select v-model:value="generation(activeTrack).resolution" :options="[{label:'720p',value:'720p'},{label:'1080p',value:'1080p'}]" size="small" /></label>
                  <label><span>时长（秒）</span><InputNumber v-model:value="generation(activeTrack).duration" :min="1" :max="30" size="small" /></label>
                </div>
                <div class="generate-actions"><label class="audio-setting"><span>生成音频</span><Switch v-model:checked="generation(activeTrack).audio" /></label><Button type="primary" :disabled="!activeTrack.prompt?.trim()" :loading="activeTrack.state === '生成中'" @click="emit('generateVideo', activeTrack)">生成视频</Button></div>
              </section>

              <section class="history-section setting-block">
                <div class="section-heading"><div><b>历史版本</b><span>选择一个结果作为当前轨道视频</span></div><Tag :color="stateColor(activeTrack.state)">{{ activeTrack.state || '未生成' }}</Tag></div>
                <div v-if="activeTrack.videoList?.length" class="video-grid">
                  <article v-for="video in activeTrack.videoList" :key="video.id" class="video-card">
                    <button class="video-preview" type="button" @click="previewVideo = video"><video v-if="video.src" :src="assetFileUrl(video.src)" muted /><div v-else class="video-placeholder">{{ video.state }}</div><Tag class="video-state" :color="stateColor(video.state)">{{ video.state }}</Tag></button>
                    <div class="video-actions"><Button v-if="['生成成功','已完成'].includes(video.state)" size="small" @click="emit('selectVideo', activeTrack, video)">{{ activeTrack.selectVideoId === video.id ? '已选中' : '选中' }}</Button><Button v-if="video.state === '生成中'" size="small" @click="emit('cancelVideo', video)">取消</Button><Button v-if="['生成失败','已取消'].includes(video.state)" size="small" @click="emit('retryVideo', video, activeTrack)">重试</Button><Button danger size="small" type="text" @click="emit('deleteVideo', video)">删除</Button></div>
                  </article>
                </div>
                <Empty v-else :image="Empty.PRESENTED_IMAGE_SIMPLE" description="暂无历史版本" />
              </section>
            </aside>
          </section>

          <section class="track-filmstrip setting-block">
            <div class="section-heading"><div><b>视频轨道</b><span>横向切换轨道</span></div><Button size="small" @click="emit('openTrack', activeTrack.id)">调整分镜</Button></div>
            <div class="track-strip"><button v-for="(track,index) in tracks" :key="track.id" :class="{active:activeTrack.id===track.id}" @click="activateTrack(track)"><img v-if="mediaUrl(track.medias?.[0])" :src="mediaUrl(track.medias[0])" /><span>轨道 {{ index + 1 }} · {{ track.duration || 5 }}s</span></button></div>
          </section>
        </template>
        <Empty v-else :image="Empty.PRESENTED_IMAGE_SIMPLE" description="分镜生成后进入视频工作台" />
      </div>

      <div v-else class="editor-pane">
        <section class="editor-workspace">
          <aside class="editor-media-library">
            <header><b>视频素材</b><Tag>{{ selectedClips.length }}</Tag></header>
            <button v-for="(clip,index) in selectedClips" :key="clip.trackId" :class="{ active: activeEditorIndex === index }" @click="activeEditorIndex = index; playingSequence = false">
              <video :src="clip.src" muted preload="metadata" /><span>轨道 {{ clip.index + 1 }}.mp4<small>{{ clip.duration }}s</small></span>
            </button>
            <Empty v-if="!selectedClips.length" :image="Empty.PRESENTED_IMAGE_SIMPLE" description="暂无视频素材" />
          </aside>

          <main class="editor-stage">
            <div class="editor-canvas">
              <video v-if="activeEditorClip?.src" :key="activeEditorClip.trackId" ref="editorPlayer" :src="activeEditorClip.src" :autoplay="playingSequence" controls preload="metadata" @ended="handleEditorEnded" @loadedmetadata="syncEditorVolume" />
              <div v-else class="editor-empty"><span class="editor-play">▶</span><p>请先在轨道生成中选中视频</p><Button type="primary" @click="activeTab = 'generate'">进入轨道生成</Button></div>
            </div>
            <div class="editor-transport"><Button shape="circle" aria-label="上一个片段" :disabled="activeEditorIndex === 0" @click="activeEditorIndex -= 1">◀</Button><Button shape="circle" type="primary" aria-label="播放序列" :disabled="!selectedClips.length" @click="playSequence">▶</Button><Button shape="circle" aria-label="下一个片段" :disabled="activeEditorIndex >= selectedClips.length - 1" @click="activeEditorIndex += 1">▶</Button><span>{{ activeEditorIndex + 1 }} / {{ selectedClips.length || 0 }}</span></div>
          </main>

          <aside class="editor-properties">
            <header><b>视频属性</b></header>
            <label><span>画面比例</span><b>{{ videoRatio || '16:9' }}</b></label>
            <label><span>分辨率</span><b>1080p</b></label>
            <label><span>当前片段</span><b>{{ activeEditorClip?.duration || 0 }}s</b></label>
            <label><span>成片时长</span><b>{{ totalDuration }}s</b></label>
            <div class="volume-control"><span>音量 {{ editorVolume }}%</span><Slider v-model:value="editorVolume" :min="0" :max="100" /></div>
            <Button type="primary" :disabled="!selectedClips.length" @click="emit('exportVideo')">合并导出视频</Button>
          </aside>
        </section>

        <section class="timeline-panel editor-timeline">
          <div class="timeline-toolbar"><b>主轨道（视频）</b><span>{{ selectedClips.length }} 个片段 · {{ totalDuration }} 秒</span></div>
          <div class="timeline-ruler"><span v-for="tick in Math.max(2, Math.ceil(totalDuration / 5) + 1)" :key="tick">{{ (tick - 1) * 5 }}s</span></div>
          <div v-if="selectedClips.length" class="clip-track">
            <button v-for="(clip,index) in selectedClips" :key="clip.trackId" class="timeline-clip" :class="{ active: activeEditorIndex === index }" :style="{ flexGrow: clip.duration }" type="button" @click="activeEditorIndex = index; playingSequence = false"><video :src="clip.src" muted /><span>{{ index + 1 }}. 轨道 {{ clip.index + 1 }} · {{ clip.duration }}s</span></button>
          </div>
          <div v-else class="empty-track">暂无可拼接视频片段</div>
        </section>
      </div>
    </div>

    <Modal v-model:open="previewVideo" width="76vw" :footer="null" title="视频预览" destroy-on-close>
      <video v-if="previewVideo?.src" :src="assetFileUrl(previewVideo.src)" class="preview-player" controls autoplay />
    </Modal>
    <Modal v-model:open="addReferenceOpen" title="添加项目资产" width="760px" :footer="null">
      <div class="asset-picker">
        <button v-for="asset in referenceAssets" :key="asset.id" type="button" @click="addReference(asset)">
          <img :src="previewUrl(asset)" :alt="asset.name" /><span>{{ asset.name }}</span><b>＋</b>
        </button>
      </div>
    </Modal>
  </div>
</template>

<style scoped>
.toonflow-workbench-shell { display: grid; width: 100%; height: 100%; min-width: 0; overflow: hidden; grid-template-rows: 68px minmax(0, 1fr); background: var(--ant-color-bg-container); }
.workbench-topbar { position: relative; display: flex; align-items: center; justify-content: center; border-bottom: 1px solid var(--ant-color-border-secondary); background: var(--ant-color-bg-container); }
.close-workbench { position: absolute; z-index: 2; top: 14px; left: 18px; width: 40px; height: 40px; padding: 0; font-size: 30px; line-height: 36px; }
.workbench-tabs { display: flex; height: 100%; align-items: stretch; gap: 12px; }.workbench-tabs button { position: relative; display: flex; min-width: 116px; align-items: center; justify-content: center; gap: 8px; padding: 0 18px; border: 0; color: var(--ant-color-text-secondary); background: transparent; cursor: pointer; }.workbench-tabs button::after { position: absolute; right: 14px; bottom: 0; left: 14px; height: 3px; border-radius: 3px 3px 0 0; background: transparent; content: ''; }.workbench-tabs button:hover { color: var(--ant-color-primary); background: var(--ant-color-fill-quaternary); }.workbench-tabs span { font-size: 20px; }.workbench-tabs b { font-size: 13px; font-weight: 500; }
.workbench-tabs .workbench-tab--active { color: var(--ant-color-primary); background: rgb(22 119 255 / 12%); }
.workbench-tabs .workbench-tab--active::after { background: var(--ant-color-primary); }
.workbench-tabs .workbench-tab--active span, .workbench-tabs .workbench-tab--active b { color: var(--ant-color-primary); }
.workbench-tabs .workbench-tab--active b { font-weight: 700; }
.workbench-content { min-width: 0; min-height: 0; overflow-x: hidden; overflow-y: auto; background: var(--ant-color-bg-layout); }
.editor-pane { width: 100%; height: 100%; overflow-y: auto; padding: 20px 28px; background: var(--ant-color-bg-layout); }
.editor-empty { display: grid; gap: 10px; text-align: center; place-items: center; }.editor-empty p { margin: 0; color: var(--ant-color-text-tertiary); }
.editor-play { display: grid; width: 58px; height: 58px; border-radius: 50%; background: rgb(255 255 255 / 12%); place-items: center; }
.editor-workspace { display: grid; min-height: 480px; overflow: hidden; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px 8px 0 0; grid-template-columns: 210px minmax(0, 1fr) 240px; background: var(--ant-color-bg-container); }
.editor-media-library, .editor-properties { min-width: 0; padding: 12px; background: var(--ant-color-bg-container); }.editor-media-library { overflow-y: auto; border-right: 1px solid var(--ant-color-border-secondary); }.editor-properties { border-left: 1px solid var(--ant-color-border-secondary); }.editor-media-library header, .editor-properties header { display: flex; height: 34px; align-items: center; justify-content: space-between; margin-bottom: 10px; }
.editor-media-library > button { display: grid; width: 100%; min-width: 0; align-items: center; gap: 8px; margin-bottom: 8px; padding: 5px; border: 1px solid transparent; border-radius: 6px; text-align: left; background: var(--ant-color-fill-tertiary); cursor: pointer; grid-template-columns: 68px minmax(0, 1fr); }.editor-media-library > button.active { border-color: var(--ant-color-primary); background: var(--ant-color-primary-bg); }.editor-media-library video { width: 68px; height: 48px; object-fit: cover; background: #000; }.editor-media-library button span { overflow: hidden; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }.editor-media-library small { display: block; color: var(--ant-color-text-tertiary); }
.editor-stage { display: grid; min-width: 0; padding: 18px; background: #eef1f5; grid-template-rows: minmax(0, 1fr) 48px; }.editor-canvas { display: grid; width: 100%; min-height: 0; align-self: center; overflow: hidden; aspect-ratio: 16 / 9; background: #000; place-items: center; }.editor-canvas video { display: block; width: 100%; height: 100%; object-fit: contain; }.editor-transport { display: flex; align-items: center; justify-content: center; gap: 12px; color: var(--ant-color-text-secondary); }.editor-transport span { margin-left: 8px; font-size: 12px; }
.editor-properties label { display: flex; align-items: center; justify-content: space-between; padding: 10px 0; border-bottom: 1px solid var(--ant-color-border-secondary); }.editor-properties label span, .volume-control > span { color: var(--ant-color-text-secondary); font-size: 12px; }.volume-control { margin: 18px 0; }.editor-properties > .ant-btn { width: 100%; }
.editor-timeline { border-top: 0; border-radius: 0 0 8px 8px; }.timeline-clip.active { border-color: #fff; box-shadow: 0 0 0 2px var(--ant-color-primary); }
.timeline-panel { padding: 14px; border: 1px solid var(--ant-color-border-secondary); border-radius: 9px; background: var(--ant-color-bg-container); }.timeline-toolbar, .timeline-ruler { display: flex; align-items: center; justify-content: space-between; }.timeline-toolbar span { color: var(--ant-color-text-tertiary); font-size: 12px; }.timeline-ruler { margin: 12px 0 5px; padding-left: 96px; color: var(--ant-color-text-tertiary); font-size: 10px; }.clip-track { display: flex; min-height: 74px; gap: 2px; padding: 6px 6px 6px 96px; border-radius: 5px; background: var(--ant-color-fill-tertiary); }.timeline-clip { position: relative; min-width: 90px; overflow: hidden; padding: 0; border: 2px solid var(--ant-color-primary); border-radius: 4px; color: #fff; background: #111827; cursor: pointer; }.timeline-clip video { width: 100%; height: 100%; object-fit: cover; opacity: 0.7; }.timeline-clip span { position: absolute; bottom: 3px; left: 5px; font-size: 10px; text-shadow: 0 1px 2px #000; }.empty-track { display: grid; height: 74px; margin-left: 96px; color: var(--ant-color-text-tertiary); background: var(--ant-color-fill-tertiary); place-items: center; }
.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px; }.section-heading > div { display: flex; align-items: baseline; gap: 10px; }.section-heading span { color: var(--ant-color-text-tertiary); font-size: 12px; }
.media-strip { display: flex; gap: 10px; overflow-x: auto; padding-bottom: 4px; }
.media-card { width: 132px; min-width: 0; flex: 0 0 132px; }.media-preview { position: relative; display: grid; width: 100%; height: auto; overflow: hidden; aspect-ratio: 16 / 9; border-radius: 7px; background: var(--ant-color-fill-secondary); place-items: center; }
.media-preview .media-image { display: block; width: 100%; height: 100%; max-width: 100%; max-height: 100%; object-fit: scale-down !important; object-position: center; }
.media-order, .media-source { position: absolute; top: 6px; padding: 1px 6px; border-radius: 10px; color: #fff; font-size: 10px; background: rgb(0 0 0 / 60%); }.media-order { left: 6px; }.media-source { right: 6px; }
.audio-preview, .empty-media { display: grid; height: 100%; color: var(--ant-color-text-tertiary); place-items: center; }.audio-preview { font-size: 28px; }
.video-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(230px, 1fr)); gap: 12px; }
.video-card { overflow: hidden; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; }
.video-preview { position: relative; display: grid; width: 100%; overflow: hidden; aspect-ratio: 16 / 9; padding: 0; border: 0; color: #fff; background: #111827; cursor: pointer; place-items: center; }.video-preview video { width: 100%; height: 100%; object-fit: cover; }
.video-placeholder { display: grid; gap: 8px; color: #cbd5e1; font-size: 12px; place-items: center; }.video-state { position: absolute; top: 8px; left: 8px; }
.video-actions { display: flex; align-items: center; gap: 12px; justify-content: flex-end; padding: 8px; }
.preview-player { display: block; width: 100%; max-height: 72vh; background: #000; }
.generation-page { display: grid; width: 100%; max-width: 100%; min-width: 0; overflow-x: hidden; gap: 0; padding: 0; box-sizing: border-box; grid-template-rows: auto auto auto; }
.generation-main { display: grid; width: 100%; max-width: 100%; min-width: 0; align-items: stretch; overflow: hidden; gap: 0; grid-template-columns: minmax(0, 56%) minmax(0, 44%); }
.generation-player { position: relative; display: grid; width: calc(100% - 40px); max-width: 100%; min-width: 0; height: auto; min-height: 0; align-self: start; overflow: hidden; aspect-ratio: 16 / 9; margin: 20px; border-radius: 8px; color: #fff; background: #070b12; box-sizing: border-box; place-items: center; }
.generation-player.has-video { display: grid; height: auto; min-height: 0; padding: 6px; }
.generation-player video { position: absolute; inset: 6px; display: block; width: calc(100% - 12px) !important; height: calc(100% - 12px) !important; max-width: calc(100% - 12px); max-height: calc(100% - 12px); object-fit: contain !important; object-position: center !important; background: #000; }
.player-placeholder { display: grid; gap: 10px; color: var(--ant-color-text-tertiary); text-align: center; place-items: center; }.player-placeholder span { display: grid; width: 64px; height: 64px; border-radius: 50%; background: rgb(255 255 255 / 10%); font-size: 24px; place-items: center; }.player-placeholder p { margin: 0; }
.generation-settings { display: grid; width: 100%; max-width: 100%; min-width: 0; overflow: hidden; gap: 0; padding: 20px; border-left: 1px solid var(--ant-color-border-secondary); background: var(--ant-color-bg-layout); box-sizing: border-box; }.setting-block { min-width: 0; padding: 14px; overflow: hidden; border: 1px solid var(--ant-color-border-secondary); border-radius: 0; background: var(--ant-color-bg-container); box-sizing: border-box; }.setting-block + .setting-block { border-top: 0; }.generation-settings > .setting-block:first-child { border-radius: 8px 8px 0 0; }.generation-settings > .setting-block:last-child { border-radius: 0 0 8px 8px; }.setting-block .section-heading { margin-bottom: 10px; }
.prompt-block :deep(textarea) { min-height: 240px !important; resize: vertical; }
.parameter-grid { display: grid; margin-bottom: 14px; gap: 10px; grid-template-columns: repeat(4, minmax(0, 1fr)); }.parameter-grid label { display: grid; min-width: 0; gap: 5px; }.parameter-grid label > span { color: var(--ant-color-text-secondary); font-size: 12px; }.parameter-grid :deep(.ant-select), .parameter-grid :deep(.ant-input-number) { width: 100%; }
.parameter-grid :deep(.ant-select-selector), .parameter-grid :deep(.ant-select-selection-item) { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.generate-actions { display: flex; align-items: center; justify-content: space-between; gap: 16px; }.generate-actions .audio-setting { display: flex; align-items: center; gap: 9px; color: var(--ant-color-text-secondary); font-size: 12px; }
.generation-page > .history-section, .generation-page > .track-filmstrip { border-right: 0; border-left: 0; border-radius: 0; }
.generation-page > .history-section { border-bottom: 0; }
.track-strip { display: flex; gap: 10px; overflow-x: auto; padding-bottom: 7px; }.track-strip button { position: relative; display: grid; width: 164px; flex: 0 0 164px; overflow: hidden; padding: 5px; border: 2px solid transparent; border-radius: 8px; color: var(--ant-color-text-secondary); background: var(--ant-color-fill-tertiary); cursor: pointer; gap: 5px; }.track-strip button.active { border-color: var(--ant-color-primary); }.track-strip img { width: 100%; height: 92px; border-radius: 5px; object-fit: contain; background: #111827; }.track-strip span { overflow: hidden; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
.asset-picker { display: grid; max-height: 65vh; overflow-y: auto; gap: 12px; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); }.asset-picker button { position: relative; display: grid; overflow: hidden; padding: 6px; border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; background: var(--ant-color-bg-container); cursor: pointer; gap: 6px; }.asset-picker button:hover { border-color: var(--ant-color-primary); }.asset-picker img { width: 100%; height: 112px; object-fit: contain; background: var(--ant-color-fill-secondary); }.asset-picker span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.asset-picker b { position: absolute; top: 10px; right: 10px; display: grid; width: 24px; height: 24px; border-radius: 50%; color: #fff; background: var(--ant-color-primary); place-items: center; }
@media (max-width: 900px) { .generation-main { grid-template-columns: 1fr; }.generation-player { width: calc(100% - 32px); max-width: none; height: auto; min-height: 260px; aspect-ratio: 16 / 9; margin: 16px; }.generation-settings { padding: 16px; border-top: 1px solid var(--ant-color-border-secondary); border-left: 0; }.parameter-grid { grid-template-columns: 1fr; } }
.toonflow-workbench-shell,
.workbench-content,
.editor-pane {
  min-width: 0;
  max-width: 100vw;
  box-sizing: border-box;
}
</style>
