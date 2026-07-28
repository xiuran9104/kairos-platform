<script lang="ts" setup>
import { computed, onBeforeUnmount, ref, watch } from 'vue';

import { Button, Checkbox, Empty, Image, Tag } from 'ant-design-vue';

import { assetFileUrl } from '../assets/asset-types';

const props = defineProps<{ assets: any[]; storyboards: any[] }>();
const emit = defineEmits<{
  exportImages: [ids: number[]];
  reorder: [ids: number[]];
}>();

const activeIndex = ref(0);
const elapsed = ref(0);
const playing = ref(false);
const selectedIds = ref<number[]>([]);
const orderedIds = ref<number[]>([]);
const initialIds = ref<number[]>([]);
const draggingId = ref<number>();
let timer: ReturnType<typeof setInterval> | undefined;

watch(
  () => props.storyboards.map((item) => item.id),
  (ids) => {
    if (!initialIds.value.length) initialIds.value = [...ids];
    const existing = orderedIds.value.filter((id) => ids.includes(id));
    orderedIds.value = [...existing, ...ids.filter((id) => !existing.includes(id))];
    activeIndex.value = Math.min(activeIndex.value, Math.max(0, ids.length - 1));
  },
  { immediate: true },
);

const orderedStoryboards = computed(() =>
  orderedIds.value
    .map((id) => props.storyboards.find((item) => item.id === id))
    .filter(Boolean),
);
const current = computed(() => orderedStoryboards.value[activeIndex.value]);
const currentDuration = computed(() => Number(current.value?.duration || 3));
const totalDuration = computed(() =>
  orderedStoryboards.value.reduce((sum, item) => sum + Number(item.duration || 3), 0),
);
const elapsedBefore = computed(() =>
  orderedStoryboards.value
    .slice(0, activeIndex.value)
    .reduce((sum, item) => sum + Number(item.duration || 3), 0),
);
const progress = computed(() =>
  totalDuration.value ? ((elapsedBefore.value + elapsed.value) / totalDuration.value) * 100 : 0,
);
const currentAssets = computed(() => {
  const ids = new Set((current.value?.associateAssetsIds ?? []).map(Number));
  return props.assets.filter((asset) => ids.has(Number(asset.id)));
});
const currentDescription = computed(() =>
  current.value?.description || current.value?.videoDesc || current.value?.describe || '',
);
function previewUrl(item: any) {
  return assetFileUrl(item?.imageFilePath || item?.filePath || item?.fileUrl || item?.src);
}
const allSelected = computed({
  get: () => orderedIds.value.length > 0 && orderedIds.value.every((id) => selectedIds.value.includes(id)),
  set: (checked: boolean) => { selectedIds.value = checked ? [...orderedIds.value] : []; },
});

function stop() {
  if (timer) clearInterval(timer);
  timer = undefined;
  playing.value = false;
}
function play() {
  if (playing.value || !current.value) return stop();
  if (activeIndex.value === orderedStoryboards.value.length - 1 && elapsed.value >= currentDuration.value) {
    activeIndex.value = 0;
    elapsed.value = 0;
  }
  playing.value = true;
  timer = setInterval(() => {
    elapsed.value += 0.05;
    if (elapsed.value < currentDuration.value) return;
    if (activeIndex.value >= orderedStoryboards.value.length - 1) {
      elapsed.value = currentDuration.value;
      stop();
      return;
    }
    activeIndex.value += 1;
    elapsed.value = 0;
  }, 50);
}
function togglePlay() { playing.value ? stop() : play(); }
function selectShot(index: number) { stop(); activeIndex.value = index; elapsed.value = 0; }
function seek(event: MouseEvent) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const target = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) * totalDuration.value;
  let cursor = 0;
  for (let index = 0; index < orderedStoryboards.value.length; index += 1) {
    const duration = Number(orderedStoryboards.value[index]?.duration || 3);
    if (cursor + duration >= target) {
      activeIndex.value = index;
      elapsed.value = target - cursor;
      return;
    }
    cursor += duration;
  }
}
function formatTime(seconds: number) {
  const value = Math.floor(seconds);
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
}
function toggleSelected(id: number, checked: boolean) {
  selectedIds.value = checked
    ? [...new Set([...selectedIds.value, id])]
    : selectedIds.value.filter((item) => item !== id);
}
function dropOn(targetId: number) {
  if (!draggingId.value || draggingId.value === targetId) return;
  const ids = [...orderedIds.value];
  const from = ids.indexOf(draggingId.value);
  const to = ids.indexOf(targetId);
  ids.splice(to, 0, ids.splice(from, 1)[0]!);
  orderedIds.value = ids;
  draggingId.value = undefined;
  emit('reorder', ids);
}
function restoreOrder() {
  orderedIds.value = initialIds.value.filter((id) => props.storyboards.some((item) => item.id === id));
  activeIndex.value = 0;
  elapsed.value = 0;
  emit('reorder', orderedIds.value);
}
onBeforeUnmount(stop);
</script>

<template>
  <div v-if="current" class="quick-preview">
    <section class="preview-main">
      <div class="visual-column">
        <div class="hero-image">
          <img
            v-if="current.filePath || current.src"
            :src="previewUrl(current)"
            :alt="currentDescription || `分镜 ${activeIndex + 1}`"
            class="hero-image-content"
          />
          <Empty v-else :image="Empty.PRESENTED_IMAGE_SIMPLE" description="暂无图片" />
        </div>
        <div class="player-controls">
          <div class="control-buttons">
            <Button shape="circle" :disabled="activeIndex === 0" aria-label="上一个分镜" @click="selectShot(activeIndex - 1)">｜◀</Button>
            <Button shape="circle" type="primary" :aria-label="playing ? '暂停' : '播放'" @click="togglePlay">{{ playing ? 'Ⅱ' : '▶' }}</Button>
            <Button shape="circle" :disabled="activeIndex === orderedStoryboards.length - 1" aria-label="下一个分镜" @click="selectShot(activeIndex + 1)">▶｜</Button>
          </div>
          <div class="progress-row">
            <span>{{ formatTime(elapsedBefore + elapsed) }}</span>
            <div class="progress-track" @mousedown="seek">
              <i
                v-for="(storyboard, index) in orderedStoryboards"
                :key="storyboard.id"
                class="progress-segment"
                :class="{ active: Number(index) === activeIndex, completed: Number(index) < activeIndex }"
                :style="{ left: `${orderedStoryboards.slice(0, Number(index)).reduce((sum, item) => sum + Number(item.duration || 3), 0) / totalDuration * 100}%`, width: `${Number(storyboard.duration || 3) / totalDuration * 100}%` }"
                @click.stop="selectShot(Number(index))"
              />
              <b :style="{ width: `${progress}%` }" />
              <em :style="{ left: `${progress}%` }" />
            </div>
            <span>{{ formatTime(totalDuration) }}</span>
          </div>
        </div>
      </div>

      <aside class="info-panel">
        <section><h4><i />分镜描述</h4><p>【序号 {{ activeIndex + 1 }}】{{ currentDescription || '暂无描述' }}</p></section>
        <section><h4><i />时长</h4><p>{{ current.duration || 3 }} 秒</p></section>
        <section>
          <h4><i />涉及资产</h4>
          <div v-if="currentAssets.length" class="asset-list">
            <article v-for="asset in currentAssets" :key="asset.id">
              <Image v-if="previewUrl(asset)" :src="previewUrl(asset)" :width="76" :height="76" />
              <div v-else class="asset-image-empty">暂无图片</div>
              <Tag>{{ asset.name }}（{{ asset.type === 'role' ? '角色' : asset.type === 'tool' ? '道具' : '场景' }}）</Tag>
            </article>
          </div>
          <Tag v-else>暂无出场人物</Tag>
        </section>
        <section class="prompt-info"><h4><i />图片提示词</h4><p><b>场景描述：</b>{{ currentDescription || '暂无描述' }}</p><p><b>提示词：</b>{{ current.prompt || '暂无提示词' }}</p></section>
      </aside>
    </section>

    <section class="shot-list-area">
      <header>
        <div><Checkbox v-model:checked="allSelected">全选</Checkbox><Button type="text" @click="restoreOrder">↶ 还原排序</Button></div>
        <Button type="text" @click="emit('exportImages', selectedIds)">⇩ 导出图片</Button>
      </header>
      <div class="shot-list">
        <article
          v-for="(storyboard, index) in orderedStoryboards"
          :key="storyboard.id"
          draggable="true"
          class="shot-item"
          :class="{ active: Number(index) === activeIndex }"
          @click="selectShot(Number(index))"
          @dragstart="draggingId = storyboard.id"
          @dragover.prevent
          @drop="dropOn(storyboard.id)"
        >
          <Checkbox :checked="selectedIds.includes(storyboard.id)" @click.stop @change="toggleSelected(storyboard.id, $event.target.checked)" />
          <div><img v-if="storyboard.filePath || storyboard.src" :src="previewUrl(storyboard)" /><span v-else>暂无图片</span><Tag>#{{ storyboard.id }}</Tag></div>
        </article>
      </div>
    </section>
  </div>
  <Empty v-else class="preview-empty" :image="Empty.PRESENTED_IMAGE_SIMPLE" description="暂无分镜图" />
</template>

<style scoped>
.quick-preview { display: grid; width: 100%; height: 100%; min-height: 0; overflow: hidden; grid-template-rows: minmax(0, 1fr) 132px; background: var(--ant-color-bg-container); }
.preview-main { display: grid; min-width: 0; min-height: 0; overflow: hidden; grid-template-columns: minmax(0, 72%) minmax(280px, 28%); }
.visual-column { display: grid; min-width: 0; min-height: 0; overflow: hidden; padding: 12px 18px 8px; align-content: start; grid-template-rows: auto auto; }
.hero-image { position: relative; display: block; width: 100%; max-width: none; height: auto; max-height: none; min-height: 0; overflow: hidden; aspect-ratio: 16 / 9; border-radius: 8px; background: #111827; }
.hero-image :deep(.ant-image), .hero-image :deep(img) { width: 100%; height: 100%; object-fit: contain; object-position: left center; }
.hero-image-content { position: absolute; inset: 0; display: block; width: 100% !important; height: 100% !important; max-width: 100%; max-height: 100%; object-fit: contain !important; object-position: center !important; }
.player-controls { width: 100%; max-width: none; padding-top: 8px; }
.control-buttons { display: flex; justify-content: center; gap: 12px; }
.progress-row { display: grid; grid-template-columns: 42px 1fr 42px; align-items: center; gap: 8px; margin-top: 10px; color: var(--ant-color-text-tertiary); font-size: 11px; }
.progress-track { position: relative; height: 8px; border-radius: 4px; background: var(--ant-color-fill-secondary); cursor: pointer; }
.progress-segment { position: absolute; height: 100%; border-right: 1px solid var(--ant-color-bg-container); }
.progress-segment.active { background: var(--ant-color-primary-bg-hover); }
.progress-segment.completed { background: var(--ant-color-primary-border); }
.progress-track b { position: absolute; height: 2px; top: 3px; left: 0; background: var(--ant-color-primary); }
.progress-track em { position: absolute; top: -3px; width: 14px; height: 14px; margin-left: -7px; border: 3px solid #fff; border-radius: 50%; background: var(--ant-color-primary); box-shadow: 0 1px 4px rgb(0 0 0 / 25%); }
.info-panel { display: block; min-width: 0; min-height: 0; overflow-y: auto; padding: 18px 20px; border-left: 1px solid var(--ant-color-border-secondary); background: var(--ant-color-bg-layout); }
.info-panel section { margin-bottom: 22px; }
.info-panel h4 { display: flex; align-items: center; gap: 7px; margin: 0 0 9px; font-size: 14px; }
.info-panel h4 i { width: 4px; height: 16px; border-radius: 2px; background: var(--ant-color-primary); }
.info-panel p { max-width: 880px; margin: 4px 0; color: var(--ant-color-text-secondary); font-size: 12px; line-height: 1.65; overflow-wrap: anywhere; }
.prompt-info p + p { margin-top: 9px; }
.asset-list { display: flex; flex-wrap: wrap; gap: 10px; }
.asset-list article { display: grid; max-width: 120px; gap: 5px; }
.asset-list :deep(img) { border-radius: 7px; object-fit: contain; background: var(--ant-color-fill-secondary); }
.asset-list :deep(.ant-tag) { overflow: hidden; margin: 0; text-overflow: ellipsis; white-space: nowrap; }
.asset-image-empty { display: grid; width: 76px; height: 76px; border-radius: 7px; color: var(--ant-color-text-tertiary); background: var(--ant-color-fill-secondary); font-size: 11px; place-items: center; }
.shot-list-area { height: 132px; overflow: hidden; padding: 6px 14px 10px; border-top: 1px solid var(--ant-color-border-secondary); background: var(--ant-color-bg-container); }
.shot-list-area header { display: flex; height: 28px; align-items: center; justify-content: space-between; }
.shot-list-area header > div { display: flex; align-items: center; gap: 8px; }
.shot-list { display: flex; height: 88px; align-items: stretch; overflow-x: auto; overflow-y: hidden; gap: 10px; padding: 4px 2px 8px; }
.shot-item { position: relative; width: 132px; flex: 0 0 132px; padding: 5px; border: 2px solid transparent; border-radius: 7px; background: var(--ant-color-fill-tertiary); cursor: grab; }
.shot-item.active { border-color: var(--ant-color-primary); }
.shot-item > :deep(.ant-checkbox-wrapper) { position: absolute; z-index: 2; top: 9px; left: 9px; }
.shot-item > div { width: 100%; height: auto; aspect-ratio: 16 / 9; overflow: hidden; border-radius: 4px; background: #111827; place-items: center; }
.shot-item img { width: 100%; height: 100%; max-height: 100%; object-fit: contain; object-position: center; }
.shot-item :deep(.ant-tag) { position: absolute; right: 4px; bottom: 4px; margin: 0; color: #fff; background: rgb(0 0 0 / 62%); }
.preview-empty { margin-top: 180px; }
.quick-preview, .preview-main, .visual-column, .info-panel, .shot-list-area { min-width: 0; max-width: 100%; box-sizing: border-box; }
@media (max-width: 1000px) { .preview-main { grid-template-columns: 1fr; overflow-y: auto; }.info-panel { border-top: 1px solid var(--ant-color-border-secondary); border-left: 0; }.quick-preview { grid-template-rows: minmax(0, 1fr) 178px; } }
@media (max-width: 900px) { .quick-preview { height: auto; overflow: visible; grid-template-rows: auto auto; }.preview-main { grid-template-columns: 1fr; overflow: visible; }.visual-column { min-height: 420px; }.hero-image, .player-controls { width: min(100%, 420px); }.hero-image { height: auto; max-height: 236px; aspect-ratio: 16 / 9; }.info-panel { min-height: 320px; overflow: visible; border-top: 1px solid var(--ant-color-border-secondary); border-left: 0; }.shot-list-area { height: 132px; } }
</style>
