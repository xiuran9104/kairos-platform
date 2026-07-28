<script setup lang="ts">
import type { Edge, Node } from '@vue-flow/core';

import { ref } from 'vue';

import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { Handle, Position, VueFlow } from '@vue-flow/core';
import { MiniMap } from '@vue-flow/minimap';
import { Button, Card, Image, Input, Select, Space, Tag } from 'ant-design-vue';

interface AssetOption {
  id: number;
  image: string;
  label: string;
  type: string;
}

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';
import '@vue-flow/minimap/dist/style.css';

const props = defineProps<{
  edges: Edge[];
  assetOptions?: AssetOption[];
  loadingNodeId?: string;
  nodes: Node[];
}>();

const emit = defineEmits<{
  add: [type: 'generated' | 'prompt' | 'upload'];
  agent: [nodeId: string];
  connect: [connection: any];
  generate: [nodeId: string];
  remove: [nodeId: string];
  save: [];
  selectAsset: [nodeId: string, assetId: number];
  'update:edges': [edges: Edge[]];
  'update:nodes': [nodes: Node[]];
  upload: [nodeId: string, file: File];
}>();

const uploadInput = ref<HTMLInputElement>();
const uploadNodeId = ref('');
const targetOptions = [
  { label: '分镜画面', value: 'storyboard' },
  { label: '角色', value: 'role' },
  { label: '服装', value: 'costume' },
  { label: '场景', value: 'scene' },
  { label: '道具', value: 'tool' },
];

function chooseUpload(nodeId: string) {
  uploadNodeId.value = nodeId;
  uploadInput.value?.click();
}

function onUpload(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (file && uploadNodeId.value) emit('upload', uploadNodeId.value, file);
}
</script>

<template>
  <div class="image-editor">
    <Space class="toolbar" wrap>
      <Button @click="emit('add', 'upload')">添加图片</Button>
      <Button @click="emit('add', 'prompt')">添加指令</Button>
      <Button type="primary" @click="emit('add', 'generated')">添加 AI 编辑</Button>
      <Button @click="emit('save')">保存工作流</Button>
      <span class="hint">连接上游图片或生成结果到 AI 编辑节点，结果会自动作为参考图继续迭代</span>
    </Space>
    <input ref="uploadInput" accept="image/*" class="hidden" type="file" @change="onUpload" />
    <VueFlow
      :edges="props.edges"
      :nodes="props.nodes"
      class="flow"
      fit-view-on-init
      :min-zoom="0.2"
      :max-zoom="2"
      @connect="emit('connect', $event)"
      @edges-change="emit('update:edges', props.edges)"
      @nodes-change="emit('update:nodes', props.nodes)"
    >
      <Background pattern-color="var(--ant-color-fill-secondary)" :gap="20" />
      <Controls />
      <MiniMap pannable zoomable />

      <template #node-upload="{ id, data }">
        <Card class="flow-node" size="small" :title="data.assetName ? `资产参考 · ${data.assetName}` : '图片输入'">
          <template #extra><Button danger size="small" type="link" @click="emit('remove', id)">删除</Button></template>
          <Image v-if="data.image" :src="data.image" :alt="data.assetName || '参考图'" class="preview-image" />
          <div v-else class="empty">选择资产图、分镜图或本地图片</div>
          <Select
            v-if="data.assetSlot !== undefined"
            :value="data.assetId"
            :options="(assetOptions || []).map((asset) => ({ label: asset.label, value: asset.id }))"
            class="asset-select"
            option-filter-prop="label"
            show-search
            @change="emit('selectAsset', id, Number($event))"
          />
          <Tag v-if="data.assetType">{{ data.assetType === 'role' ? '衍生人物' : data.assetType === 'scene' ? '场景' : '道具' }}</Tag>
          <Button v-if="data.assetSlot === undefined" block size="small" @click="chooseUpload(id)">更换图片</Button>
          <Handle type="source" :position="Position.Right" />
        </Card>
      </template>

      <template #node-prompt="{ id, data }">
        <Card class="flow-node" size="small" title="编辑指令">
          <template #extra><Button danger size="small" type="link" @click="emit('remove', id)">删除</Button></template>
          <Input.TextArea
            v-model:value="data.prompt"
            :rows="6"
            placeholder="只描述需要改变的内容，例如：保持人物和构图，将白衣改为黑色夜行衣"
          />
          <Handle type="target" :position="Position.Left" />
          <Handle type="source" :position="Position.Right" />
        </Card>
      </template>

      <template #node-generated="{ id, data }">
        <Card class="flow-node generated" size="small" title="AI 图片编辑">
          <template #extra><Button danger size="small" type="link" @click="emit('remove', id)">删除</Button></template>
          <img v-if="data.generatedImage" :src="data.generatedImage" class="preview" />
          <div v-else class="empty">连接参考图和编辑指令后执行</div>
          <Select v-model:value="data.targetType" class="field" :options="targetOptions" />
          <Input.TextArea v-model:value="data.prompt" :rows="3" placeholder="当前节点补充编辑要求（可选）" />
          <Space class="actions">
            <Button size="small" @click="emit('agent', id)">让生产 Agent 规划</Button>
            <Button :loading="loadingNodeId === id" size="small" type="primary" @click="emit('generate', id)">执行此节点</Button>
          </Space>
          <Handle type="target" :position="Position.Left" />
          <Handle type="source" :position="Position.Right" />
        </Card>
      </template>
    </VueFlow>
  </div>
</template>

<style scoped>
.image-editor { height: 72vh; min-height: 560px; }
.toolbar { height: 44px; }
.hint { color: var(--ant-color-text-tertiary); font-size: 12px; }
.flow { height: calc(100% - 44px); border: 1px solid var(--ant-color-border-secondary); border-radius: 8px; background: var(--ant-color-fill-quaternary); }
.flow-node { width: 290px; box-shadow: 0 5px 18px rgb(0 0 0 / 8%); }
.flow-node.generated { width: 320px; }
.preview, .empty { width: 100%; height: 150px; margin-bottom: 10px; border-radius: 6px; object-fit: contain; background: var(--ant-color-bg-layout); }
.preview-image { display: block; width: 100%; margin-bottom: 10px; }
.preview-image :deep(.ant-image-img) { width: 100%; height: 150px; border-radius: 6px; object-fit: contain; background: var(--ant-color-bg-layout); }
.asset-select { width: 100%; margin-bottom: 8px; }
.empty { display: flex; align-items: center; justify-content: center; padding: 16px; color: var(--ant-color-text-tertiary); text-align: center; }
.field { width: 100%; margin-bottom: 8px; }
.actions { margin-top: 10px; }
.hidden { display: none; }
</style>
