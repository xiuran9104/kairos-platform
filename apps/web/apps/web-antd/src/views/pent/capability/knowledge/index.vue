<script lang="ts" setup>
import type { AiKnowledgeDocumentApi } from '#/api/ai/knowledge/document';
import type { AiKnowledgeKnowledgeApi } from '#/api/ai/knowledge/knowledge';

import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  createKnowledgeDocumentList,
  deleteKnowledgeDocument,
  getKnowledgeDocument,
  getKnowledgeDocumentPage,
  updateKnowledgeDocument,
} from '#/api/ai/knowledge/document';
import { getKnowledgePage } from '#/api/ai/knowledge/knowledge';

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const knowledgeList = ref<AiKnowledgeKnowledgeApi.Knowledge[]>([]);
const documents = ref<any[]>([]);
const categoryFilter = ref<number | undefined>(undefined);
const searchText = ref('');
let searchTimer: ReturnType<typeof setTimeout> | undefined;

// 按分类分页（与原版一致：每页 10 个分类）
const pagination = ref({ currentPage: 1, pageSize: 10 });

async function fetchKnowledgeList() {
  try {
    const res = await getKnowledgePage({ pageNo: 1, pageSize: 200 });
    knowledgeList.value = res.list || [];
  } catch {
    /* 静默 */
  }
}

async function fetchDocuments() {
  loading.value = true;
  try {
    const params: any = { pageNo: 1, pageSize: 500 };
    if (categoryFilter.value) params.knowledgeId = categoryFilter.value;
    const res = await getKnowledgeDocumentPage(params);
    documents.value = res.list || [];
  } catch {
    /* 静默 */
  } finally {
    loading.value = false;
  }
}

function refresh() {
  fetchKnowledgeList();
  fetchDocuments();
}

// ── filters / grouping ────────────────────────────────────────────────────
const filteredDocs = computed(() => {
  let list = documents.value;
  if (categoryFilter.value) {
    list = list.filter((d) => d.knowledgeId === categoryFilter.value);
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter(
      (d) =>
        (d.name || '').toLowerCase().includes(q) ||
        (d.content || '').toLowerCase().includes(q),
    );
  }
  return list;
});

interface CategoryGroup {
  id: number;
  name: string;
  docs: any[];
}

const allGroups = computed<CategoryGroup[]>(() => {
  const map = new Map<number, CategoryGroup>();
  for (const doc of filteredDocs.value) {
    if (!map.has(doc.knowledgeId)) {
      map.set(doc.knowledgeId, {
        id: doc.knowledgeId,
        name:
          knowledgeList.value.find((k) => k.id === doc.knowledgeId)?.name ||
          `知识库 #${doc.knowledgeId}`,
        docs: [],
      });
    }
    map.get(doc.knowledgeId)!.docs.push(doc);
  }
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
});

// 当前页的分类（选中单个分类或搜索时不分页，展示全部匹配结果）
const paged = computed(() => !categoryFilter.value && !searchText.value);
const totalPages = computed(() =>
  Math.max(1, Math.ceil(allGroups.value.length / pagination.value.pageSize)),
);
const pagedGroups = computed(() => {
  if (!paged.value) return allGroups.value;
  const start = (pagination.value.currentPage - 1) * pagination.value.pageSize;
  return allGroups.value.slice(start, start + pagination.value.pageSize);
});

// 统计（对齐原版：总分类数 / 当前页分类 / 当前页知识项）
const stats = computed(() => ({
  totalCategories: knowledgeList.value.length || allGroups.value.length,
  pageCategories: pagedGroups.value.length,
  pageItems: pagedGroups.value.reduce((sum, g) => sum + g.docs.length, 0),
}));

function selectCategory(value: number | undefined) {
  categoryFilter.value = value;
  pagination.value.currentPage = 1;
  fetchDocuments();
}

function handleSearchInput() {
  if (searchTimer) clearTimeout(searchTimer);
  if (!searchText.value.trim()) {
    pagination.value.currentPage = 1;
    return;
  }
  searchTimer = setTimeout(() => {
    pagination.value.currentPage = 1;
  }, 500);
}

function loadPage(page: number) {
  if (page < 1 || page > totalPages.value) return;
  pagination.value.currentPage = page;
}

// ── card helpers ───────────────────────────────────────────────────────────
// 去除 markdown 格式并截取前 150 字符（对齐原版卡片预览逻辑）
function docPreview(d: any) {
  if (!d.content) return '';
  let text: string = d.content;
  text = text.replace(/^#+\s+/gm, '');
  text = text.replace(/```[\s\S]*?```/g, '');
  text = text.replace(/`[^`]+`/g, '');
  text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
  text = text.replace(/\n+/g, ' ').replace(/\s+/g, ' ').trim();
  return text.length > 150 ? `${text.slice(0, 150)}...` : text;
}

function docTimeInfo(d: any): { label: string; time: string } {
  const fmt = (t?: string) => (t || '').slice(0, 19).replace('T', ' ');
  if (d.updateTime && fmt(d.updateTime)) return { label: '更新时间', time: fmt(d.updateTime) };
  if (d.createTime && fmt(d.createTime)) return { label: '创建时间', time: fmt(d.createTime) };
  return { label: '', time: '' };
}

// 7 天内更新视为「新」
function isRecent(d: any) {
  if (!d.updateTime) return false;
  const t = new Date(d.updateTime).getTime();
  if (Number.isNaN(t)) return false;
  return Date.now() - t < 7 * 24 * 60 * 60 * 1000;
}

// ── modal ──────────────────────────────────────────────────────────────────
const showModal = ref(false);
const editing = ref<any | null>(null);
const saving = ref(false);
const form = ref({
  knowledgeId: undefined as number | undefined,
  name: '',
  content: '',
});

function openCreate() {
  editing.value = null;
  form.value = { knowledgeId: categoryFilter.value, name: '', content: '' };
  showModal.value = true;
}

async function openEdit(d: AiKnowledgeDocumentApi.KnowledgeDocument) {
  editing.value = d;
  form.value = { knowledgeId: d.knowledgeId, name: d.name, content: '' };
  showModal.value = true;
  try {
    const detail: any = await getKnowledgeDocument(d.id);
    form.value.content = detail?.content || '';
  } catch {
    /* 详情加载失败时保留空内容 */
  }
}

async function doSave() {
  if (!form.value.knowledgeId || !form.value.name.trim() || !form.value.content.trim()) {
    message.warning('请填写所有必填字段');
    return;
  }
  saving.value = true;
  try {
    if (editing.value) {
      await updateKnowledgeDocument({
        id: editing.value.id,
        knowledgeId: form.value.knowledgeId,
        name: form.value.name.trim(),
        content: form.value.content,
      });
      message.success('更新成功');
    } else {
      await createKnowledgeDocumentList({
        knowledgeId: form.value.knowledgeId,
        list: [{ name: form.value.name.trim(), content: form.value.content }],
      });
      message.success('创建成功');
    }
    showModal.value = false;
    fetchKnowledgeList();
    fetchDocuments();
  } catch {
    message.error('保存知识项失败');
  } finally {
    saving.value = false;
  }
}

async function doDelete(d: AiKnowledgeDocumentApi.KnowledgeDocument) {
  try {
    await deleteKnowledgeDocument(d.id);
    message.success('删除成功');
    fetchKnowledgeList();
    fetchDocuments();
  } catch {
    message.error('删除知识项失败');
  }
}

onMounted(refresh);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="知识管理" sub-title="渗透测试知识库与文档">
      <template #extra>
        <a-button :loading="loading" @click="refresh">刷新</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">添加知识</a-button>
      </template>
    </a-page-header>

    <!-- 统计栏 -->
    <div
      style="
        margin-top: 16px;
        display: flex;
        gap: 32px;
        font-size: 13px;
        color: var(--ant-color-text-secondary);
      "
    >
      <span>总分类数 <strong>{{ stats.totalCategories }}</strong></span>
      <span>当前页分类 <strong>{{ stats.pageCategories }} 个</strong></span>
      <span>当前页知识项 <strong>{{ stats.pageItems }} 项</strong></span>
    </div>

    <!-- 筛选 -->
    <div style="margin-top: 12px; display: flex; gap: 16px; flex-wrap: wrap">
      <label style="display: flex; align-items: center; gap: 8px">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">分类筛选</span>
        <a-select
          :value="categoryFilter"
          placeholder="全部"
          style="width: 200px"
          allow-clear
          :options="knowledgeList.map((k) => ({ label: k.name, value: k.id }))"
          @change="selectCategory($event as number | undefined)"
        />
      </label>
      <label style="display: flex; align-items: center; gap: 8px">
        <span style="font-size: 13px; color: var(--ant-color-text-secondary)">搜索</span>
        <a-input-search
          v-model:value="searchText"
          placeholder="搜索知识..."
          style="width: 240px"
          allow-clear
          @input="handleSearchInput"
          @search="pagination.currentPage = 1"
        />
      </label>
    </div>

    <!-- 分类分组 + 知识项卡片 -->
    <a-spin :spinning="loading">
      <div v-for="group in pagedGroups" :key="group.id" style="margin-top: 16px">
        <div style="margin-bottom: 8px; display: flex; align-items: center; gap: 8px">
          <span style="font-weight: 600; font-size: 15px">{{ group.name }}</span>
          <a-tag size="small">{{ group.docs.length }} 项</a-tag>
        </div>
        <a-row :gutter="[16, 16]">
          <a-col v-for="d in group.docs" :key="d.id" :xs="24" :sm="12" :lg="8" :xl="6">
            <a-card hoverable size="small">
              <template #title>
                <span style="font-size: 13px" :title="d.name">{{ d.name }}</span>
              </template>
              <template #extra>
                <a-button size="small" type="link" title="编辑" @click="openEdit(d)">编辑</a-button>
                <a-popconfirm title="确定要删除这个知识项吗？" @confirm="doDelete(d)">
                  <a-button size="small" type="link" danger title="删除">删除</a-button>
                </a-popconfirm>
              </template>
              <div
                v-if="docPreview(d)"
                style="
                  font-size: 12px;
                  color: var(--ant-color-text-tertiary);
                  min-height: 54px;
                  word-break: break-all;
                "
              >
                {{ docPreview(d) }}
              </div>
              <div
                v-else
                style="
                  font-size: 12px;
                  color: var(--ant-color-text-quaternary);
                  min-height: 54px;
                "
              >
                （无内容摘要）
              </div>
              <div
                style="
                  font-size: 12px;
                  color: var(--ant-color-text-quaternary);
                  margin-top: 8px;
                  display: flex;
                  align-items: center;
                  gap: 8px;
                "
              >
                <span v-if="docTimeInfo(d).time" :title="docTimeInfo(d).label">
                  🕒 {{ docTimeInfo(d).time }}
                </span>
                <span v-else>🕒 —</span>
                <a-tag v-if="isRecent(d)" size="small" color="green">新</a-tag>
              </div>
            </a-card>
          </a-col>
        </a-row>
      </div>
      <a-empty
        v-if="!pagedGroups.length && !loading"
        :description="searchText ? '未找到匹配的知识项' : '暂无知识项'"
        style="margin-top: 48px"
      />
    </a-spin>

    <!-- 分页（按分类分页；选中分类或搜索时隐藏） -->
    <div
      v-if="paged && totalPages > 1"
      style="
        display: flex;
        justify-content: center;
        align-items: center;
        gap: 8px;
        padding: 20px;
        flex-wrap: wrap;
      "
    >
      <a-button
        size="small"
        :disabled="pagination.currentPage <= 1"
        @click="loadPage(pagination.currentPage - 1)"
      >
        上一页
      </a-button>
      <span style="padding: 0 12px; font-size: 13px">
        第 {{ pagination.currentPage }} 页，共 {{ totalPages }} 页（共
        {{ allGroups.length }} 个分类）
      </span>
      <a-button
        size="small"
        :disabled="pagination.currentPage >= totalPages"
        @click="loadPage(pagination.currentPage + 1)"
      >
        下一页
      </a-button>
    </div>

    <!-- 添加/编辑知识弹窗 -->
    <a-modal
      v-model:open="showModal"
      :title="editing ? '编辑知识' : '添加知识'"
      width="900px"
      :confirm-loading="saving"
      ok-text="保存"
      cancel-text="取消"
      @ok="doSave"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="分类（风险类型）" required>
          <a-select
            v-model:value="form.knowledgeId"
            placeholder="例如：SQL注入"
            :options="knowledgeList.map((k) => ({ label: k.name, value: k.id }))"
          />
        </a-form-item>
        <a-form-item label="标题" required>
          <a-input v-model:value="form.name" placeholder="知识项标题" />
        </a-form-item>
        <a-form-item label="内容（Markdown格式）" required>
          <a-textarea
            v-model:value="form.content"
            :rows="14"
            placeholder="输入知识内容，支持Markdown格式..."
            style="font-family: Monaco, Menlo, 'Ubuntu Mono', monospace; font-size: 13px"
          />
        </a-form-item>
      </a-form>
    </a-modal>
  </div>
</template>
