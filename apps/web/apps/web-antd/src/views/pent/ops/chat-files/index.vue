<script lang="ts" setup>
import type { PentFileEntry } from '#/api/pent/ops-files';

import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import { downloadFileFromBlobPart } from '@vben/utils';

import {
  deletePentFile,
  downloadPentFile,
  getPentFileContent,
  getPentFiles,
  mkdirPentFile,
  renamePentFile,
  savePentFileContent,
  uploadPentFile,
} from '#/api/pent/ops-files';

// ── state ──────────────────────────────────────────────────────────────────
const loading = ref(false);
const currentPath = ref('');
const entries = ref<PentFileEntry[]>([]);
const total = ref(0);
const page = ref(1);
const pageSize = ref(20);

// 筛选项（对齐原版：会话 ID / 项目 ID / 文件名 / 来源 / 分组方式）。
// 后端暂不支持会话、项目、来源过滤，分组固定为「按文件夹（路径浏览）」，
// 对应控件保留结构但禁用。
const filterConversation = ref('');
const filterProject = ref('');
const filterName = ref('');
const filterSource = ref('all');
const groupBy = ref('folder');

const sourceOptions = [
  { label: '全部', value: 'all' },
  { label: '对话附件', value: 'upload' },
  { label: '工具输出', value: 'reduction' },
  { label: '工作目录', value: 'workspace' },
  { label: '会话产物', value: 'conversation_artifact' },
];

const groupOptions = [{ label: '按文件夹（路径浏览）', value: 'folder' }];

const isSearching = computed(() => !!filterName.value.trim());

async function fetchFiles() {
  loading.value = true;
  try {
    const res = await getPentFiles({
      path: isSearching.value ? '' : currentPath.value,
      search: filterName.value.trim() || undefined,
      page: page.value,
      page_size: isSearching.value ? pageSize.value : 1000,
    });
    entries.value = res?.entries || [];
    total.value = res?.total || 0;
  } catch {
    message.error('加载文件列表失败');
  } finally {
    loading.value = false;
  }
}

function onSearch() {
  page.value = 1;
  fetchFiles();
}

function clearSearch() {
  filterName.value = '';
  page.value = 1;
  fetchFiles();
}

function onPageChange(p: number, ps: number) {
  page.value = p;
  pageSize.value = ps;
  fetchFiles();
}

// ── 路径浏览（面包屑）──────────────────────────────────────────────────────
const breadcrumbs = computed(() => {
  const segs = currentPath.value.split('/').filter(Boolean);
  const crumbs: { label: string; path: string }[] = [{ label: '文件', path: '' }];
  segs.forEach((seg, i) => {
    crumbs.push({ label: seg, path: segs.slice(0, i + 1).join('/') });
  });
  return crumbs;
});

function goPath(path: string) {
  currentPath.value = path;
  filterName.value = '';
  page.value = 1;
  fetchFiles();
}

function goUp() {
  const segs = currentPath.value.split('/').filter(Boolean);
  segs.pop();
  goPath(segs.join('/'));
}

// ── 上传 ───────────────────────────────────────────────────────────────────
const uploading = ref(false);

async function customUpload(options: any) {
  const file = options.file as File;
  uploading.value = true;
  try {
    await uploadPentFile(file, currentPath.value);
    message.success(`已上传 ${file.name}`);
    fetchFiles();
  } catch {
    message.error('上传失败');
  } finally {
    uploading.value = false;
  }
}

// ── 新建文件夹 ─────────────────────────────────────────────────────────────
const showMkdir = ref(false);
const mkdirName = ref('');
const mkdirLoading = ref(false);

function openMkdir() {
  mkdirName.value = '';
  showMkdir.value = true;
}

async function doMkdir() {
  const name = mkdirName.value.trim();
  if (!name || name.includes('/') || name.includes('\\') || name === '.' || name === '..') {
    message.warning('名称无效：不能为空，且不能包含 /、\\ 或 . / ..');
    return;
  }
  mkdirLoading.value = true;
  try {
    await mkdirPentFile(currentPath.value, name);
    message.success('文件夹已创建');
    showMkdir.value = false;
    fetchFiles();
  } catch {
    message.error('创建失败');
  } finally {
    mkdirLoading.value = false;
  }
}

// ── 行操作 ─────────────────────────────────────────────────────────────────
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.left = '-9999px';
      document.body.append(ta);
      ta.select();
      const ok = document.execCommand('copy');
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

async function copyPath(row: PentFileEntry) {
  const ok = await copyText(row.path);
  if (ok) {
    message.success(row.is_dir ? '目录路径已复制，可粘贴到对话中' : '路径已复制，可粘贴到对话中引用');
  } else {
    message.error('复制失败');
  }
}

async function doDownload(row: PentFileEntry) {
  try {
    const blob = await downloadPentFile(row.path);
    downloadFileFromBlobPart({ fileName: row.name, source: blob });
  } catch {
    message.error('下载失败');
  }
}

const showRename = ref(false);
const renameTarget = ref<PentFileEntry | null>(null);
const renameValue = ref('');
const renameLoading = ref(false);

function openRename(row: PentFileEntry) {
  renameTarget.value = row;
  renameValue.value = row.name;
  showRename.value = true;
}

async function doRename() {
  const row = renameTarget.value;
  if (!row) return;
  if (!renameValue.value.trim()) {
    message.warning('请输入新名称');
    return;
  }
  renameLoading.value = true;
  try {
    await renamePentFile(row.path, renameValue.value.trim());
    message.success('已重命名');
    showRename.value = false;
    fetchFiles();
  } catch {
    message.error('重命名失败');
  } finally {
    renameLoading.value = false;
  }
}

const showEdit = ref(false);
const editTarget = ref<PentFileEntry | null>(null);
const editContent = ref('');
const editLoading = ref(false);
const editSaving = ref(false);

const MAX_EDIT_SIZE = 256 * 1024;

const BINARY_EXT = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'tif', 'tiff', 'heic', 'heif', 'svgz',
  'pdf', 'zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'zst',
  'mp3', 'm4a', 'wav', 'ogg', 'flac', 'aac',
  'mp4', 'avi', 'mkv', 'mov', 'wmv', 'webm', 'm4v',
  'exe', 'dll', 'so', 'dylib', 'bin', 'app', 'dmg', 'pkg',
  'woff', 'woff2', 'ttf', 'otf', 'eot',
  'sqlite', 'db', 'sqlite3',
  'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods',
  'class', 'jar', 'war', 'apk', 'ipa',
  'iso', 'img',
]);

function isBinaryByName(name: string) {
  const i = name ? name.lastIndexOf('.') : -1;
  if (i < 0 || i === name.length - 1) return false;
  return BINARY_EXT.has(name.slice(i + 1).toLowerCase());
}

function canEdit(row: PentFileEntry) {
  return !row.is_dir && !isBinaryByName(row.name) && (row.size || 0) <= MAX_EDIT_SIZE;
}

async function openEdit(row: PentFileEntry) {
  editTarget.value = row;
  editContent.value = '';
  showEdit.value = true;
  editLoading.value = true;
  try {
    const res = await getPentFileContent(row.path);
    editContent.value = res?.content || '';
  } catch {
    message.error('读取文件内容失败');
    showEdit.value = false;
  } finally {
    editLoading.value = false;
  }
}

async function doSaveEdit() {
  const row = editTarget.value;
  if (!row) return;
  editSaving.value = true;
  try {
    await savePentFileContent(row.path, editContent.value);
    message.success('已保存');
    showEdit.value = false;
    fetchFiles();
  } catch {
    message.error('保存失败');
  } finally {
    editSaving.value = false;
  }
}

async function doDelete(row: PentFileEntry) {
  try {
    await deletePentFile(row.path);
    message.success('已删除');
    fetchFiles();
  } catch {
    message.error('删除失败');
  }
}

// ── 表格 ───────────────────────────────────────────────────────────────────
// 搜索（平铺）模式列对齐原版：日期 / 会话 / 子路径 / 文件名 / 大小 / 修改时间 / 操作
const searchColumns = [
  { title: '日期', key: 'date', width: 110 },
  { title: '会话', key: 'conversation', width: 140 },
  { title: '子路径', key: 'sub_path' },
  { title: '文件名', dataIndex: 'name', key: 'name' },
  { title: '大小', dataIndex: 'size', key: 'size', width: 110 },
  { title: '修改时间', dataIndex: 'modified_at', key: 'modified_at', width: 170 },
  { title: '操作', key: 'action', width: 180 },
];

// 路径浏览（按文件夹）模式列对齐原版紧凑表：文件名 / 大小 / 修改时间 / 操作
const browseColumns = [
  { title: '文件名', dataIndex: 'name', key: 'name' },
  { title: '大小', dataIndex: 'size', key: 'size', width: 110 },
  { title: '修改时间', dataIndex: 'modified_at', key: 'modified_at', width: 170 },
  { title: '操作', key: 'action', width: 180 },
];

const columns = computed(() => (isSearching.value ? searchColumns : browseColumns));

/** 平铺模式下由完整路径拆出子路径（目录部分） */
function subPathOf(path: string) {
  const i = path.lastIndexOf('/');
  return i > 0 ? path.slice(0, i) : '';
}

function formatSize(size: number) {
  if (!size) return '0 B';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

function formatTime(t?: string) {
  return t ? t.slice(0, 16).replace('T', ' ') : '—';
}

function onRowClick(row: PentFileEntry) {
  if (!isSearching.value && row.is_dir) goPath(row.path);
}

onMounted(fetchFiles);
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="文件管理">
      <template #extra>
        <a-tooltip title="后端暂不支持导出">
          <a-button disabled>导出当前结果</a-button>
        </a-tooltip>
        <a-upload :show-upload-list="false" :custom-request="customUpload" multiple>
          <a-button type="primary" style="margin-left: 8px" :loading="uploading">上传文件</a-button>
        </a-upload>
        <a-button style="margin-left: 8px" :loading="loading" @click="fetchFiles">刷新</a-button>
      </template>
    </a-page-header>

    <a-typography-paragraph type="secondary" style="margin-top: 8px">
      管理在对话中上传的文件。可将文件拖拽到下方列表区域，或点击「上传文件」选择文件（支持多选）。需要让
      AI 引用某文件时，在列表中点击「复制路径」，到对话里粘贴即可（路径为服务器上的绝对路径，与对话附件保存位置一致）。
    </a-typography-paragraph>

    <!-- 筛选区（对齐原版：会话 ID / 项目 ID / 文件名 / 来源 / 分组方式 / 搜索） -->
    <a-card size="small" style="margin-top: 8px">
      <div style="display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-end">
        <div>
          <div style="margin-bottom: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">会话 ID</div>
          <a-tooltip title="后端暂不支持按会话过滤">
            <a-input v-model:value="filterConversation" placeholder="留空表示全部" style="width: 180px" disabled />
          </a-tooltip>
        </div>
        <div>
          <div style="margin-bottom: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">项目 ID</div>
          <a-tooltip title="后端暂不支持按项目过滤">
            <a-input v-model:value="filterProject" placeholder="留空表示全部" style="width: 180px" disabled />
          </a-tooltip>
        </div>
        <div>
          <div style="margin-bottom: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">文件名</div>
          <a-input
            v-model:value="filterName"
            placeholder="筛选文件名"
            style="width: 240px"
            allow-clear
            @press-enter="onSearch"
            @clear="clearSearch"
          />
        </div>
        <div>
          <div style="margin-bottom: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">来源</div>
          <a-tooltip title="后端暂不支持按来源过滤">
            <a-select v-model:value="filterSource" :options="sourceOptions" style="width: 150px" disabled />
          </a-tooltip>
        </div>
        <div>
          <div style="margin-bottom: 4px; font-size: 12px; color: var(--ant-color-text-secondary)">分组方式</div>
          <a-select v-model:value="groupBy" :options="groupOptions" style="width: 190px" disabled />
        </div>
        <a-button type="primary" @click="onSearch">搜索</a-button>
      </div>
    </a-card>

    <!-- 列表区 -->
    <a-card size="small" style="margin-top: 16px">
      <!-- 路径浏览工具栏（对齐原版「按文件夹」模式：面包屑 + 新建文件夹 + 上级） -->
      <div
        v-if="!isSearching"
        style="display: flex; flex-wrap: wrap; gap: 12px; align-items: center; justify-content: space-between; margin-bottom: 12px"
      >
        <a-breadcrumb>
          <a-breadcrumb-item v-for="(crumb, i) in breadcrumbs" :key="crumb.path">
            <a v-if="i < breadcrumbs.length - 1" @click="goPath(crumb.path)">{{ crumb.label }}</a>
            <span v-else>{{ crumb.label }}</span>
          </a-breadcrumb-item>
        </a-breadcrumb>
        <div>
          <a-button size="small" @click="openMkdir">新建文件夹</a-button>
          <a-button size="small" style="margin-left: 8px" :disabled="!currentPath" @click="goUp">上级</a-button>
        </div>
      </div>

      <a-table
        :columns="columns"
        :data-source="entries"
        :loading="loading"
        row-key="path"
        size="middle"
        :custom-row="(row: PentFileEntry) => ({ onClick: () => onRowClick(row), style: !isSearching && row.is_dir ? 'cursor: pointer' : '' })"
        :pagination="
          isSearching
            ? {
                current: page,
                pageSize,
                total,
                showSizeChanger: true,
                showTotal: (t: number) => `共 ${t} 个文件`,
              }
            : false
        "
        @change="(p: any) => onPageChange(p.current, p.pageSize)"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'date' || column.key === 'conversation'">
            <span style="color: var(--ant-color-text-secondary)">—</span>
          </template>
          <template v-else-if="column.key === 'sub_path'">
            <span v-if="subPathOf(record.path)" style="font-size: 12px; color: var(--ant-color-text-secondary)">
              {{ subPathOf(record.path) }}
            </span>
            <span v-else style="color: var(--ant-color-text-secondary)">（根目录）</span>
          </template>
          <template v-else-if="column.key === 'name'">
            <span style="display: inline-flex; align-items: center; gap: 6px">
              <span style="font-size: 16px">{{ record.is_dir ? '📁' : '📄' }}</span>
              <a v-if="record.is_dir && !isSearching" @click.stop="goPath(record.path)">{{ record.name }}</a>
              <span v-else>{{ record.name }}</span>
            </span>
          </template>
          <template v-else-if="column.key === 'size'">
            {{ record.is_dir ? '—' : formatSize(record.size) }}
          </template>
          <template v-else-if="column.key === 'modified_at'">
            {{ formatTime(record.modified_at) }}
          </template>
          <template v-else-if="column.key === 'action'">
            <div @click.stop>
              <a-tooltip title="复制服务器上的绝对路径，可粘贴到对话中引用">
                <a-button size="small" type="link" @click="copyPath(record)">复制路径</a-button>
              </a-tooltip>
              <a-button v-if="!record.is_dir" size="small" type="link" @click="doDownload(record)">下载</a-button>
              <a-dropdown :trigger="['click']">
                <a-button size="small" type="link">更多</a-button>
                <template #overlay>
                  <a-menu>
                    <a-menu-item v-if="canEdit(record)" key="edit" @click="openEdit(record)">编辑</a-menu-item>
                    <a-menu-item
                      v-else-if="!record.is_dir"
                      key="edit-disabled"
                      disabled
                      title="图片、压缩包等二进制或过大文件无法在此以文本方式编辑，请使用「下载」。"
                    >
                      不可编辑
                    </a-menu-item>
                    <a-menu-item key="rename" @click="openRename(record)">重命名</a-menu-item>
                    <a-menu-item key="delete" danger>
                      <a-popconfirm
                        :title="`确认删除${record.is_dir ? '该文件夹及其中的全部文件' : `「${record.name}」`}？此操作不可恢复。`"
                        ok-text="删除"
                        cancel-text="取消"
                        @confirm="doDelete(record)"
                      >
                        <span @click.stop>删除</span>
                      </a-popconfirm>
                    </a-menu-item>
                  </a-menu>
                </template>
              </a-dropdown>
            </div>
          </template>
        </template>
        <template #emptyText>
          <a-empty :description="isSearching ? '暂无文件' : '此文件夹为空'" />
        </template>
      </a-table>
    </a-card>

    <!-- 新建文件夹 modal -->
    <a-modal
      v-model:open="showMkdir"
      title="新建文件夹"
      width="420px"
      :confirm-loading="mkdirLoading"
      @ok="doMkdir"
    >
      <a-form layout="vertical">
        <a-form-item label="位置">
          <a-input :value="currentPath || '（根目录）'" disabled />
        </a-form-item>
        <a-form-item label="文件夹名称">
          <a-input v-model:value="mkdirName" placeholder="仅名称，不含 /" @press-enter="doMkdir" />
        </a-form-item>
      </a-form>
    </a-modal>

    <!-- 重命名 modal -->
    <a-modal
      v-model:open="showRename"
      title="重命名"
      width="420px"
      :confirm-loading="renameLoading"
      @ok="doRename"
    >
      <a-input v-model:value="renameValue" placeholder="新名称" @press-enter="doRename" />
    </a-modal>

    <!-- 编辑文件 modal -->
    <a-modal
      v-model:open="showEdit"
      :title="`编辑 ${editTarget?.name || ''}`"
      width="760px"
      :confirm-loading="editSaving"
      @ok="doSaveEdit"
    >
      <a-spin :spinning="editLoading">
        <a-textarea v-model:value="editContent" :rows="16" style="font-family: monospace; font-size: 12px" />
      </a-spin>
    </a-modal>
  </div>
</template>
