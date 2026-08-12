<script lang="ts" setup>
import type { PentAsset } from '#/api/pent/ops-assets';

import { computed, onMounted, reactive, ref } from 'vue';
import { message } from 'ant-design-vue';

import { getPentProjects } from '#/api/pent';
import {
  batchDeletePentAssets,
  bindPentAssetsProject,
  bulkUpdatePentAssets,
  deletePentAsset,
  getPentAssets,
  importPentAssets,
  mergePentAssets,
  updatePentAsset,
} from '#/api/pent/ops-assets';

// ── 字典 ───────────────────────────────────────────────────────────────────
const statusOptions = [
  { label: '活跃', value: 'active' },
  { label: '停用', value: 'inactive' },
];
const riskOptions = [
  { label: '严重', value: 'critical' },
  { label: '高危', value: 'high' },
  { label: '中危', value: 'medium' },
  { label: '低危', value: 'low' },
  { label: '提示', value: 'info' },
  { label: '正常', value: 'normal' },
];
const sourceOptions = [
  { label: '手动录入', value: 'manual' },
  { label: '批量导入', value: 'manual-import' },
  { label: '信息收集', value: 'infogather' },
  { label: 'FOFA', value: 'fofa' },
  { label: 'Quake', value: 'quake' },
];
const envOptions = [
  { label: '生产', value: 'production' },
  { label: '预发布', value: 'staging' },
  { label: '测试', value: 'testing' },
  { label: '开发', value: 'development' },
  { label: '其他', value: 'other' },
];
const criticalityOptions = [
  { label: '核心', value: 'critical' },
  { label: '重要', value: 'high' },
  { label: '一般', value: 'medium' },
  { label: '低', value: 'low' },
];
const scanStateOptions = [
  { label: '从未扫描', value: 'never' },
  { label: '已扫描', value: 'scanned' },
  { label: '超 7 天未扫描', value: '7' },
  { label: '超 30 天未扫描', value: '30' },
  { label: '超 90 天未扫描', value: '90' },
];
const sortOptions = [
  { label: '最近发现（新→旧）', value: 'last_seen_at:desc' },
  { label: '最近发现（旧→新）', value: 'last_seen_at:asc' },
  { label: '首次发现（新→旧）', value: 'first_seen_at:desc' },
  { label: '首次发现（旧→新）', value: 'first_seen_at:asc' },
  { label: '最近扫描（新→旧）', value: 'last_scan_at:desc' },
];
const protocolOptions = ['http', 'https', 'tcp', 'udp', 'ssh', 'rdp', 'ftp', 'mysql', 'redis'];

const riskColor: Record<string, string> = {
  critical: 'red',
  high: 'orange',
  medium: 'gold',
  low: 'blue',
  info: 'default',
  normal: 'green',
};
const riskLabel: Record<string, string> = {
  critical: '严重',
  high: '高危',
  medium: '中危',
  low: '低危',
  info: '提示',
  normal: '正常',
};
const statusColor: Record<string, string> = { active: 'green', inactive: 'default' };
const statusLabel: Record<string, string> = { active: '活跃', inactive: '停用' };
const sourceLabel: Record<string, string> = Object.fromEntries(
  sourceOptions.map((o) => [o.value, o.label]),
);
const envLabel: Record<string, string> = Object.fromEntries(
  envOptions.map((o) => [o.value, o.label]),
);
const criticalityLabel: Record<string, string> = Object.fromEntries(
  criticalityOptions.map((o) => [o.value, o.label]),
);

// ── 查询条件 ───────────────────────────────────────────────────────────────
const loading = ref(false);
const searchText = ref('');
const statusFilter = ref<string | undefined>(undefined);
const projectFilter = ref<string | undefined>(undefined);
const showAdvanced = ref(false);

const advFilter = reactive({
  risk_level: undefined as string | undefined,
  min_vulnerabilities: undefined as number | undefined,
  protocol: undefined as string | undefined,
  port: undefined as number | undefined,
  source: undefined as string | undefined,
  tag: '',
  scan_state: undefined as string | undefined,
  country: '',
  province: '',
  city: '',
  responsible_person: '',
  department: '',
  business_system: '',
  environment: undefined as string | undefined,
  criticality: undefined as string | undefined,
  first_seen_after: '',
  first_seen_before: '',
  last_seen_after: '',
  last_seen_before: '',
  sort: 'last_seen_at:desc',
});

function buildParams() {
  const params: Record<string, any> = {
    q: searchText.value || undefined,
    status: statusFilter.value,
    project_id: projectFilter.value,
    risk_level: advFilter.risk_level,
    min_vulnerabilities: advFilter.min_vulnerabilities,
    protocol: advFilter.protocol,
    port: advFilter.port,
    source: advFilter.source,
    tag: advFilter.tag || undefined,
    country: advFilter.country || undefined,
    province: advFilter.province || undefined,
    city: advFilter.city || undefined,
    responsible_person: advFilter.responsible_person || undefined,
    department: advFilter.department || undefined,
    business_system: advFilter.business_system || undefined,
    environment: advFilter.environment,
    criticality: advFilter.criticality,
    first_seen_after: advFilter.first_seen_after || undefined,
    first_seen_before: advFilter.first_seen_before || undefined,
    last_seen_after: advFilter.last_seen_after || undefined,
    last_seen_before: advFilter.last_seen_before || undefined,
    page: pagination.current,
    page_size: pagination.pageSize,
  };
  const scan = advFilter.scan_state;
  if (scan === 'never' || scan === 'scanned') params.scan_state = scan;
  else if (scan) params.scan_overdue_days = scan;
  const [sortBy, sortOrder] = (advFilter.sort || 'last_seen_at:desc').split(':');
  if (sortBy) params.sort_by = sortBy;
  if (sortOrder) params.sort_order = sortOrder;
  return params;
}

function resetFilters() {
  searchText.value = '';
  statusFilter.value = undefined;
  projectFilter.value = undefined;
  Object.assign(advFilter, {
    risk_level: undefined,
    min_vulnerabilities: undefined,
    protocol: undefined,
    port: undefined,
    source: undefined,
    tag: '',
    scan_state: undefined,
    country: '',
    province: '',
    city: '',
    responsible_person: '',
    department: '',
    business_system: '',
    environment: undefined,
    criticality: undefined,
    first_seen_after: '',
    first_seen_before: '',
    last_seen_after: '',
    last_seen_before: '',
    sort: 'last_seen_at:desc',
  });
  savedViewSelect.value = undefined;
  clearSelection();
  onSearch();
}

// ── 保存的筛选视图（localStorage，与原版一致为纯前端能力）────────────────────
const SAVED_VIEWS_KEY = 'pent.asset_saved_views';
const savedViews = ref<{ name: string; values: Record<string, any> }[]>([]);
const savedViewSelect = ref<string | undefined>(undefined);

function readSavedViews() {
  try {
    const views = JSON.parse(localStorage.getItem(SAVED_VIEWS_KEY) || '[]');
    savedViews.value = Array.isArray(views) ? views : [];
  } catch {
    savedViews.value = [];
  }
}

function currentFilterValues() {
  return {
    searchText: searchText.value,
    statusFilter: statusFilter.value,
    projectFilter: projectFilter.value,
    advFilter: { ...advFilter },
  };
}

function saveCurrentView() {
  const name = window.prompt('请输入筛选视图名称');
  if (!name || !name.trim()) return;
  const cleanName = name.trim().slice(0, 60);
  const views = savedViews.value.filter((v) => v.name !== cleanName);
  views.push({ name: cleanName, values: currentFilterValues() });
  try {
    localStorage.setItem(SAVED_VIEWS_KEY, JSON.stringify(views));
  } catch {
    message.error('保存筛选视图失败');
    return;
  }
  readSavedViews();
  savedViewSelect.value = cleanName;
  message.success('筛选视图已保存');
}

function applySavedView(name?: string) {
  if (!name) return;
  const view = savedViews.value.find((v) => v.name === name);
  if (!view) return;
  const values = view.values || {};
  searchText.value = values.searchText || '';
  statusFilter.value = values.statusFilter || undefined;
  projectFilter.value = values.projectFilter || undefined;
  Object.assign(advFilter, values.advFilter || {});
  showAdvanced.value = true;
  clearSelection();
  onSearch();
}

function deleteCurrentView() {
  const name = savedViewSelect.value;
  if (!name) return;
  if (!window.confirm(`删除筛选视图“${name}”吗？`)) return;
  const views = savedViews.value.filter((v) => v.name !== name);
  try {
    localStorage.setItem(SAVED_VIEWS_KEY, JSON.stringify(views));
  } catch {
    return;
  }
  readSavedViews();
  savedViewSelect.value = undefined;
}

// ── 项目映射 ───────────────────────────────────────────────────────────────
const projects = ref<any[]>([]);
const projectOptions = computed(() =>
  projects.value.map((p) => ({ label: p.name, value: p.id })),
);
const projectNameMap = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {};
  for (const p of projects.value) map[p.id] = p.name;
  return map;
});

async function fetchProjects() {
  try {
    projects.value = ((await getPentProjects()) as any[]) || [];
  } catch {
    projects.value = [];
  }
}

// ── 表格 ───────────────────────────────────────────────────────────────────
const columns = [
  { title: '目标', dataIndex: 'target', key: 'target' },
  { title: '服务', dataIndex: 'service', key: 'service', width: 120 },
  { title: '所属项目', dataIndex: 'project_id', key: 'project', width: 130 },
  { title: '归属', key: 'ownership', width: 130 },
  { title: '上次扫描', dataIndex: 'last_scan_at', key: 'last_scan_at', width: 160 },
  { title: '相关漏洞', key: 'vulnerabilities', width: 90 },
  { title: '风险', dataIndex: 'risk_level', key: 'risk_level', width: 90 },
  { title: '状态', dataIndex: 'status', key: 'status', width: 90 },
  { title: '操作', key: 'action', width: 160 },
];

const dataSource = ref<PentAsset[]>([]);
const pagination = reactive({ current: 1, pageSize: 20, total: 0 });
const totalPages = computed(() =>
  Math.max(1, Math.ceil(pagination.total / pagination.pageSize)),
);
const selectedRowKeys = ref<(number | string)[]>([]);
const selectedRows = ref<PentAsset[]>([]);

const rowSelection = computed(() => ({
  selectedRowKeys: selectedRowKeys.value,
  onChange: (keys: (number | string)[], rows: PentAsset[]) => {
    selectedRowKeys.value = keys;
    selectedRows.value = rows;
  },
}));

function assetTargetLabel(asset: PentAsset) {
  return asset.host || asset.domain || asset.ip || asset.target || '—';
}

function assetService(asset: PentAsset) {
  return [asset.protocol || '', asset.port ? `:${asset.port}` : ''].join('') || '—';
}

function assetRiskLabel(level?: string) {
  return riskLabel[level || ''] || (level ? level : '未评估');
}

function formatTime(t?: string) {
  if (!t) return '—';
  const ts = new Date(t).getTime();
  if (Number.isNaN(ts)) return t.slice(0, 16).replace('T', ' ');
  return new Date(ts).toLocaleString();
}

async function fetchAssets() {
  loading.value = true;
  try {
    const resp = await getPentAssets(buildParams());
    dataSource.value = resp?.assets || [];
    pagination.total = resp?.total || 0;
    pagination.current = resp?.page || pagination.current;
    if (pagination.current > totalPages.value) {
      pagination.current = totalPages.value;
      return fetchAssets();
    }
  } catch {
    message.error('加载资产失败');
    dataSource.value = [];
    pagination.total = 0;
  } finally {
    loading.value = false;
  }
}

function onSearch() {
  pagination.current = 1;
  clearSelection();
  fetchAssets();
}

function onTableChange(pag: { current?: number; pageSize?: number }) {
  pagination.current = pag.current || 1;
  pagination.pageSize = pag.pageSize || 20;
  fetchAssets();
}

// ── 目标解析（URL / 域名 / IP:端口）────────────────────────────────────────
const DEFAULT_PORT: Record<string, number> = {
  http: 80,
  https: 443,
  ssh: 22,
  ftp: 21,
  smtp: 25,
  rdp: 3389,
  mysql: 3306,
  postgresql: 5432,
  redis: 6379,
  mongodb: 27_017,
};

function parseTarget(target: string) {
  const result: Record<string, any> = { target };
  let text = target.trim();
  const protoMatch = /^([a-z][a-z0-9+.-]*):\/\//i.exec(text);
  if (protoMatch) {
    result.protocol = protoMatch[1]!.toLowerCase();
    text = text.slice(protoMatch[0].length);
  }
  text = text.split('/')[0] || text;
  const portMatch = /:(\d+)$/.exec(text);
  if (portMatch) {
    result.port = Number(portMatch[1]);
    text = text.slice(0, -portMatch[0].length);
  }
  if (text) {
    result.host = text;
    if (/^\d{1,3}(\.\d{1,3}){3}$/.test(text)) result.ip = text;
    else result.domain = text;
  }
  if (!result.protocol && result.port) {
    if (result.port === 443) result.protocol = 'https';
    else if (result.port === 80) result.protocol = 'http';
  }
  if (!result.port && result.protocol && DEFAULT_PORT[result.protocol]) {
    result.port = DEFAULT_PORT[result.protocol];
  }
  return result;
}

// ── 详情 drawer ────────────────────────────────────────────────────────────
const showDetail = ref(false);
const detailAsset = ref<PentAsset | null>(null);

function openDetail(record: PentAsset) {
  detailAsset.value = record;
  showDetail.value = true;
}

function editFromDetail() {
  const asset = detailAsset.value;
  showDetail.value = false;
  if (asset) openEdit(asset);
}

// ── 删除 ───────────────────────────────────────────────────────────────────
async function doDelete(record: PentAsset) {
  try {
    await deletePentAsset(record.id);
    message.success('已删除');
    fetchAssets();
  } catch {
    message.error('删除资产失败');
  }
}

// ── 新增 / 编辑 modal ──────────────────────────────────────────────────────
const showEdit = ref(false);
const editingId = ref<null | string>(null);
const saving = ref(false);
const form = reactive({
  target: '',
  host: '',
  ip: '',
  domain: '',
  port: undefined as number | undefined,
  protocol: '',
  server: '',
  title: '',
  status: 'active',
  project_id: undefined as string | undefined,
  country: '',
  province: '',
  city: '',
  responsible_person: '',
  department: '',
  business_system: '',
  environment: undefined as string | undefined,
  criticality: undefined as string | undefined,
  tags: [] as string[],
});

function resetForm() {
  Object.assign(form, {
    target: '',
    host: '',
    ip: '',
    domain: '',
    port: undefined,
    protocol: '',
    server: '',
    title: '',
    status: 'active',
    project_id: undefined,
    country: '',
    province: '',
    city: '',
    responsible_person: '',
    department: '',
    business_system: '',
    environment: undefined,
    criticality: undefined,
    tags: [],
  });
}

function openCreate() {
  editingId.value = null;
  resetForm();
  showEdit.value = true;
}

function openEdit(record: PentAsset) {
  editingId.value = record.id;
  resetForm();
  Object.assign(form, {
    target: record.host || record.target || '',
    host: record.host || '',
    ip: record.ip || '',
    domain: record.domain || '',
    port: record.port || undefined,
    protocol: record.protocol || '',
    server: record.server || '',
    title: record.title || '',
    status: record.status || 'active',
    project_id: record.project_id || undefined,
    country: record.country || '',
    province: record.province || '',
    city: record.city || '',
    responsible_person: record.responsible_person || '',
    department: record.department || '',
    business_system: record.business_system || '',
    environment: record.environment || undefined,
    criticality: record.criticality || undefined,
    tags: [...(record.tags || [])],
  });
  showEdit.value = true;
}

function onTargetBlur() {
  const text = form.target.trim();
  if (!text) return;
  const parsed = parseTarget(text);
  form.host = parsed.host || '';
  form.ip = parsed.ip || '';
  form.domain = parsed.domain || '';
  form.port = parsed.port || undefined;
  form.protocol = parsed.protocol || '';
}

function collectForm() {
  return {
    host: form.host.trim(),
    ip: form.ip.trim(),
    domain: form.domain.trim(),
    port: form.port ?? 0,
    protocol: form.protocol.trim(),
    server: form.server.trim(),
    title: form.title.trim(),
    status: form.status,
    project_id: form.project_id || '',
    country: form.country.trim(),
    province: form.province.trim(),
    city: form.city.trim(),
    responsible_person: form.responsible_person.trim(),
    department: form.department.trim(),
    business_system: form.business_system.trim(),
    environment: form.environment || '',
    criticality: form.criticality || '',
    tags: form.tags,
  };
}

async function doSave() {
  if (!form.target.trim()) {
    message.warning('请输入资产地址');
    return;
  }
  onTargetBlur();
  saving.value = true;
  try {
    if (editingId.value) {
      await updatePentAsset(editingId.value, collectForm());
      message.success('资产已更新');
    } else {
      const result = await importPentAssets({
        source: 'manual',
        assets: [collectForm()],
      });
      if (result && result.skipped > 0 && result.created === 0 && result.updated === 0) {
        throw new Error('资产未保存，可能已存在且没有更新权限');
      }
      message.success(
        result && result.updated > 0 && result.created === 0
          ? '资产已存在，信息已安全合并'
          : '资产已添加',
      );
    }
    showEdit.value = false;
    fetchAssets();
  } catch (error: any) {
    message.error(`保存资产失败: ${error?.message || ''}`);
  } finally {
    saving.value = false;
  }
}

// ── 批量导入 modal（CSV 文件 + 模板下载 + 预览，对齐原版结构）────────────────
const showImport = ref(false);
const importing = ref(false);
const importFileName = ref('');
const importRows = ref<
  { rowNumber: number; asset: null | Record<string, any>; projectName: string; error: string }[]
>([]);

const importPreviewColumns = [
  { title: '行号', dataIndex: 'rowNumber', key: 'rowNumber', width: 70 },
  { title: '目标', key: 'target' },
  { title: '项目', key: 'project', width: 140 },
  { title: '状态', key: 'status', width: 90 },
  { title: '校验结果', key: 'result', width: 200 },
];

const importValidCount = computed(
  () => importRows.value.filter((r) => !r.error).length,
);

const IMPORT_HEADER_ALIASES: Record<string, string[]> = {
  target: ['target', 'asset', 'assetaddress', '目标', '资产', '资产地址'],
  project: ['project', 'projectname', 'projectid', '项目', '项目名称', '项目id', '所属项目'],
  tags: ['tags', 'tag', '标签'],
  host: ['host', 'url', '完整url', '主机'],
  ip: ['ip', 'ipaddress', 'ip地址'],
  domain: ['domain', '域名'],
  port: ['port', '端口'],
  protocol: ['protocol', 'scheme', '协议'],
  title: ['title', 'pagetitle', '标题', '页面标题'],
  server: ['server', 'service', 'product', '服务', '产品', '服务指纹'],
  country: ['country', 'countryregion', '国家', '国家地区'],
  province: ['province', 'state', '省份', '州'],
  city: ['city', '城市'],
  responsible_person: ['responsibleperson', 'owner', '负责人', '责任人'],
  department: ['department', '部门'],
  business_system: ['businesssystem', 'system', '业务系统', '系统'],
  environment: ['environment', 'env', '环境'],
  criticality: ['criticality', 'importance', '重要性', '重要等级'],
  status: ['status', '状态'],
};

function normalizeImportHeader(value: string) {
  return String(value ?? '')
    .replace(/^\uFEFF/, '')
    .trim()
    .toLowerCase()
    .replace(/[\s_\-/.（）()]+/g, '');
}

function importColumnForHeader(value: string) {
  const normalized = normalizeImportHeader(value);
  return (
    Object.keys(IMPORT_HEADER_ALIASES).find((column) =>
      IMPORT_HEADER_ALIASES[column]!.includes(normalized),
    ) || ''
  );
}

function openImport() {
  importRows.value = [];
  importFileName.value = '';
  showImport.value = true;
}

function downloadImportTemplate() {
  const headers = Object.keys(IMPORT_HEADER_ALIASES);
  const csv = `\uFEFF${headers.join(',')}\r\n`;
  const link = document.createElement('a');
  link.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
  link.download = 'asset-import-template.csv';
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 0);
}

function splitCsvLine(line: string) {
  const cells: string[] = [];
  let current = '';
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]!;
    if (inQuotes) {
      if (ch === '"' && line[i + 1] === '"') {
        current += '"';
        i++;
      } else if (ch === '"') {
        inQuotes = false;
      } else {
        current += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ',') {
      cells.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  cells.push(current);
  return cells;
}

function importRecord(values: Record<string, string>, rowNumber: number) {
  const fail = (messageText: string) => ({ rowNumber, values, asset: null, projectName: '', error: messageText });
  const target = values.target || values.host || values.domain || values.ip || '';
  if (!target) return fail('缺少目标地址');
  const parsed = parseTarget(target);
  const rawPort = values.port;
  const port =
    rawPort === '' || rawPort == null ? Number(parsed.port || 0) : Number(rawPort);
  if (!Number.isInteger(port) || port < 0 || port > 65_535) return fail('端口必须在 1–65535 之间');
  const statusMap: Record<string, string> = {
    active: 'active',
    inactive: 'inactive',
    活跃: 'active',
    停用: 'inactive',
  };
  const status = statusMap[String(values.status || 'active').toLowerCase()];
  if (!status) return fail('状态仅支持 active 或 inactive');
  let projectId = '';
  let projectName = '';
  if (values.project) {
    const projectValue = String(values.project).trim().toLowerCase();
    const project = projects.value.find(
      (p) =>
        String(p.id).toLowerCase() === projectValue ||
        String(p.name).trim().toLowerCase() === projectValue,
    );
    if (!project) return fail(`项目不存在或无权访问：${values.project}`);
    projectId = project.id;
    projectName = project.name;
  }
  const tags = String(values.tags || '')
    .split(/[,，;；|]/)
    .map((t) => t.trim())
    .filter(Boolean);
  const asset: Record<string, any> = {
    host: values.host || parsed.host || '',
    ip: (values.ip || parsed.ip || '').toLowerCase(),
    domain: values.domain || parsed.domain || '',
    port,
    protocol: (values.protocol || parsed.protocol || '').toLowerCase(),
    title: values.title || '',
    server: values.server || '',
    country: values.country || '',
    province: values.province || '',
    city: values.city || '',
    responsible_person: values.responsible_person || '',
    department: values.department || '',
    business_system: values.business_system || '',
    environment: values.environment || '',
    criticality: values.criticality || '',
    status,
    tags,
    project_id: projectId,
    source: 'manual-import',
  };
  return { rowNumber, values, asset, projectName, error: '' };
}

async function handleImportFile(file: File) {
  importRows.value = [];
  importFileName.value = file.name || '';
  if (!/\.csv$/i.test(file.name || '')) {
    message.error('仅支持 .csv 文件');
    return false;
  }
  try {
    const text = await file.text();
    const lines = text.split(/\r?\n/).filter((line) => line.trim() !== '');
    if (lines.length < 2) throw new Error('文件中没有可导入的数据');
    const headers = splitCsvLine(lines[0]!).map(importColumnForHeader);
    if (!headers.some(Boolean)) throw new Error('未识别到模板字段，请使用下载的模板填写');
    const seen = new Map<string, number>();
    importRows.value = lines.slice(1).map((line, index) => {
      const cells = splitCsvLine(line);
      const values: Record<string, string> = {};
      headers.forEach((column, columnIndex) => {
        if (column && values[column] == null) values[column] = (cells[columnIndex] ?? '').trim();
      });
      const parsed = importRecord(values, index + 2);
      if (!parsed.error && parsed.asset) {
        const asset = parsed.asset;
        const key = `${String(asset.domain || asset.ip || asset.host).toLowerCase()}|${asset.port || 0}|${asset.protocol || ''}`;
        if (seen.has(key)) parsed.error = `与第 ${seen.get(key)} 行重复`;
        else seen.set(key, parsed.rowNumber);
      }
      return parsed;
    });
  } catch (error: any) {
    message.error(error?.message || '无法解析文件，请确认文件未损坏且格式正确');
  }
  return false;
}

async function doImport() {
  const validRows = importRows.value.filter((r) => !r.error && r.asset);
  if (!validRows.length) return;
  importing.value = true;
  try {
    const result = await importPentAssets({
      assets: validRows.map((r) => r.asset!),
      source: 'manual-import',
    });
    const invalid = importRows.value.length - validRows.length;
    showImport.value = false;
    fetchAssets();
    message.success(
      `导入完成：新增 ${result?.created ?? 0} 条，更新 ${result?.updated ?? 0} 条，跳过 ${(result?.skipped ?? 0) + invalid} 条`,
    );
  } catch (error: any) {
    message.error(`资产入库失败: ${error?.message || ''}`);
  } finally {
    importing.value = false;
  }
}

// ── 批量操作 ───────────────────────────────────────────────────────────────
const selectedIds = computed(() => selectedRowKeys.value.map(String));

function clearSelection() {
  selectedRowKeys.value = [];
  selectedRows.value = [];
}

// 绑定项目
const showBind = ref(false);
const bindProjectId = ref<string | undefined>(undefined);
const bindLoading = ref(false);

function openBind() {
  bindProjectId.value = undefined;
  showBind.value = true;
}

async function doBind() {
  if (!bindProjectId.value) {
    message.warning('请选择要绑定的项目');
    return;
  }
  bindLoading.value = true;
  try {
    await bindPentAssetsProject(selectedIds.value, bindProjectId.value);
    message.success(`已绑定 ${selectedIds.value.length} 个资产`);
    showBind.value = false;
    clearSelection();
    fetchAssets();
  } catch {
    message.error('绑定项目失败');
  } finally {
    bindLoading.value = false;
  }
}

// 批量编辑
const showBulkEdit = ref(false);
const bulkLoading = ref(false);
const bulkForm = reactive({
  status: undefined as string | undefined,
  responsible_person: '',
  department: '',
  business_system: '',
  environment: undefined as string | undefined,
  criticality: undefined as string | undefined,
  add_tags: [] as string[],
  remove_tags: [] as string[],
});

function openBulkEdit() {
  Object.assign(bulkForm, {
    status: undefined,
    responsible_person: '',
    department: '',
    business_system: '',
    environment: undefined,
    criticality: undefined,
    add_tags: [],
    remove_tags: [],
  });
  showBulkEdit.value = true;
}

async function doBulkEdit() {
  const patch: Record<string, string> = {};
  if (bulkForm.status) patch.status = bulkForm.status;
  if (bulkForm.responsible_person.trim()) patch.responsible_person = bulkForm.responsible_person.trim();
  if (bulkForm.department.trim()) patch.department = bulkForm.department.trim();
  if (bulkForm.business_system.trim()) patch.business_system = bulkForm.business_system.trim();
  if (bulkForm.environment) patch.environment = bulkForm.environment;
  if (bulkForm.criticality) patch.criticality = bulkForm.criticality;
  if (!Object.keys(patch).length && !bulkForm.add_tags.length && !bulkForm.remove_tags.length) {
    message.warning('请至少填写一项修改');
    return;
  }
  bulkLoading.value = true;
  try {
    await bulkUpdatePentAssets({
      asset_ids: selectedIds.value,
      patch,
      add_tags: bulkForm.add_tags.length ? bulkForm.add_tags : undefined,
      remove_tags: bulkForm.remove_tags.length ? bulkForm.remove_tags : undefined,
    });
    message.success(`已更新 ${selectedIds.value.length} 个资产`);
    showBulkEdit.value = false;
    clearSelection();
    fetchAssets();
  } catch {
    message.error('批量编辑失败');
  } finally {
    bulkLoading.value = false;
  }
}

// 合并重复
const showMerge = ref(false);
const mergePrimaryId = ref<string | undefined>(undefined);
const mergeLoading = ref(false);

function openMerge() {
  mergePrimaryId.value = selectedIds.value[0];
  showMerge.value = true;
}

async function doMerge() {
  if (!mergePrimaryId.value) {
    message.warning('请选择主资产');
    return;
  }
  mergeLoading.value = true;
  try {
    await mergePentAssets({ asset_ids: selectedIds.value, primary_id: mergePrimaryId.value });
    message.success(`已合并 ${selectedIds.value.length} 个重复资产`);
    showMerge.value = false;
    clearSelection();
    fetchAssets();
  } catch {
    message.error('合并资产失败');
  } finally {
    mergeLoading.value = false;
  }
}

// 批量删除
async function doBatchDelete() {
  try {
    await batchDeletePentAssets(selectedIds.value);
    message.success(`已删除 ${selectedIds.value.length} 个资产`);
    clearSelection();
    fetchAssets();
  } catch {
    message.error('批量删除失败');
  }
}

// 导出 CSV（纯前端，对齐原版导出能力；xlsx 组件未集成，仅支持 CSV）
function csvCell(value: any) {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function exportSelected() {
  if (!selectedRows.value.length) {
    message.warning('请先选择资产');
    return;
  }
  const rows = selectedRows.value.map((asset) => ({
    target: assetTargetLabel(asset),
    host: asset.host || '',
    ip: asset.ip || '',
    domain: asset.domain || '',
    port: asset.port ?? '',
    protocol: asset.protocol || '',
    title: asset.title || '',
    server: asset.server || '',
    project: asset.project_id ? projectNameMap.value[asset.project_id] || '' : '',
    responsible_person: asset.responsible_person || '',
    department: asset.department || '',
    business_system: asset.business_system || '',
    environment: asset.environment || '',
    criticality: asset.criticality || '',
    country: asset.country || '',
    province: asset.province || '',
    city: asset.city || '',
    source: asset.source || '',
    status: asset.status || '',
    tags: (asset.tags || []).join(','),
    risk_level: asset.risk_level || '',
    first_seen: asset.first_seen || '',
    last_seen: asset.last_seen || '',
    last_scan_at: asset.last_scan_at || '',
  }));
  const headers = Object.keys(rows[0]!);
  const csv = `\uFEFF${[
    headers.join(','),
    ...rows.map((row) =>
      headers.map((key) => csvCell((row as Record<string, any>)[key])).join(','),
    ),
  ].join('\r\n')}`;
  const stamp = new Date().toISOString().slice(0, 10);
  const link = document.createElement('a');
  link.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
  link.download = `assets-${stamp}.csv`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 0);
}

onMounted(() => {
  readSavedViews();
  fetchProjects();
  fetchAssets();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="资产库" sub-title="测绘与收集资产的统一管理">
      <template #extra>
        <a-button :loading="loading" @click="fetchAssets">刷新</a-button>
        <a-button style="margin-left: 8px" @click="openImport">批量导入</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="openCreate">新增资产</a-button>
      </template>
    </a-page-header>

    <!-- 工具栏 -->
    <a-card size="small" style="margin-top: 16px">
      <div style="display: flex; flex-wrap: wrap; gap: 8px; align-items: center">
        <a-input
          v-model:value="searchText"
          placeholder="搜索主机、IP、域名、标题、服务或标签"
          style="flex: 1; min-width: 260px"
          allow-clear
          @press-enter="onSearch"
        />
        <a-select
          v-model:value="statusFilter"
          style="width: 130px"
          :options="statusOptions"
          placeholder="全部状态"
          allow-clear
        />
        <a-select
          v-model:value="projectFilter"
          style="width: 170px"
          :options="projectOptions"
          placeholder="全部项目"
          allow-clear
          show-search
          option-filter-prop="label"
        />
        <a-button @click="showAdvanced = !showAdvanced">
          高级筛选
        </a-button>
        <a-button type="primary" @click="onSearch">搜索</a-button>
        <a-button type="link" @click="resetFilters">重置</a-button>
        <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
          {{ pagination.total }} 个资产
        </span>
      </div>

      <!-- 高级筛选面板 -->
      <template v-if="showAdvanced">
        <a-divider style="margin: 12px 0" />
        <a-form layout="vertical" size="small">
          <a-row :gutter="12">
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="风险等级" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.risk_level" allow-clear placeholder="全部" :options="riskOptions" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="最少漏洞数" style="margin-bottom: 8px">
                <a-input-number v-model:value="advFilter.min_vulnerabilities" :min="0" style="width: 100%" placeholder="如 1" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="协议" style="margin-bottom: 8px">
                <a-select
                  v-model:value="advFilter.protocol"
                  allow-clear
                  placeholder="全部"
                  :options="protocolOptions.map((p) => ({ label: p, value: p }))"
                />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="端口" style="margin-bottom: 8px">
                <a-input-number v-model:value="advFilter.port" :min="1" :max="65535" style="width: 100%" placeholder="如 443" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="来源" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.source" allow-clear placeholder="全部" :options="sourceOptions" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="标签" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.tag" placeholder="按标签筛选" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="扫描状态" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.scan_state" allow-clear placeholder="全部" :options="scanStateOptions" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="国家 / 地区" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.country" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="省份" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.province" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="城市" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.city" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="负责人" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.responsible_person" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="部门" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.department" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="业务系统" style="margin-bottom: 8px">
                <a-input v-model:value="advFilter.business_system" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="环境" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.environment" allow-clear placeholder="全部" :options="envOptions" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="重要性" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.criticality" allow-clear placeholder="全部" :options="criticalityOptions" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="首次发现（起）" style="margin-bottom: 8px">
                <a-date-picker v-model:value="advFilter.first_seen_after" value-format="YYYY-MM-DD" style="width: 100%" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="首次发现（止）" style="margin-bottom: 8px">
                <a-date-picker v-model:value="advFilter.first_seen_before" value-format="YYYY-MM-DD" style="width: 100%" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="最近发现（起）" style="margin-bottom: 8px">
                <a-date-picker v-model:value="advFilter.last_seen_after" value-format="YYYY-MM-DD" style="width: 100%" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="最近发现（止）" style="margin-bottom: 8px">
                <a-date-picker v-model:value="advFilter.last_seen_before" value-format="YYYY-MM-DD" style="width: 100%" />
              </a-form-item>
            </a-col>
            <a-col :xs="12" :md="6" :lg="4">
              <a-form-item label="排序" style="margin-bottom: 8px">
                <a-select v-model:value="advFilter.sort" :options="sortOptions" />
              </a-form-item>
            </a-col>
          </a-row>
        </a-form>
      </template>

      <a-divider style="margin: 12px 0" />
      <div style="display: flex; flex-wrap: wrap; gap: 8px; align-items: center">
        <a-select
          v-model:value="savedViewSelect"
          style="width: 220px"
          placeholder="保存的筛选视图"
          allow-clear
          :options="savedViews.map((v) => ({ label: v.name, value: v.name }))"
          @change="(name: any) => applySavedView(name)"
        />
        <a-button @click="saveCurrentView">保存当前视图</a-button>
        <a-button type="link" :disabled="!savedViewSelect" @click="deleteCurrentView">删除视图</a-button>
      </div>
    </a-card>

    <!-- 批量操作条 -->
    <a-alert v-if="selectedRowKeys.length" type="info" style="margin-top: 16px" closable @close="clearSelection">
      <template #message>
        <span style="margin-right: 12px">已选择 {{ selectedRowKeys.length }} 项</span>
        <a-button size="small" @click="openBind">绑定项目</a-button>
        <a-button size="small" style="margin-left: 8px" @click="openBulkEdit">批量编辑</a-button>
        <a-button
          size="small"
          style="margin-left: 8px"
          :disabled="selectedRowKeys.length < 2"
          :title="selectedRowKeys.length < 2 ? '请至少选择两个重复资产' : ''"
          @click="openMerge"
        >
          合并重复
        </a-button>
        <a-button size="small" style="margin-left: 8px" @click="exportSelected">导出 CSV</a-button>
        <a-popconfirm
          :title="`确定永久删除选中的 ${selectedRowKeys.length} 个资产吗？`"
          ok-text="删除"
          ok-type="danger"
          cancel-text="取消"
          @confirm="doBatchDelete"
        >
          <a-button size="small" danger style="margin-left: 8px">批量删除</a-button>
        </a-popconfirm>
      </template>
    </a-alert>

    <!-- 资产表格 -->
    <a-card size="small" style="margin-top: 16px">
      <a-table
        :columns="columns"
        :data-source="dataSource"
        :loading="loading"
        :row-selection="rowSelection"
        row-key="id"
        size="middle"
        :pagination="{
          current: pagination.current,
          pageSize: pagination.pageSize,
          total: pagination.total,
          pageSizeOptions: ['10', '20', '50', '100'],
          showSizeChanger: true,
          showTotal: (t: number, range: [number, number]) => `显示 ${range[0]}-${range[1]} / 共 ${t} 条`,
        }"
        @change="onTableChange"
      >
        <template #bodyCell="{ column, record }">
          <template v-if="column.key === 'target'">
            <a-button
              type="link"
              size="small"
              style="padding: 0; height: auto; font-weight: 500"
              :title="[record.host, record.ip, record.domain].filter(Boolean).join(' · ')"
              @click="openDetail(record)"
            >
              {{ assetTargetLabel(record) }}
            </a-button>
          </template>
          <template v-if="column.key === 'service'">
            <span :title="assetService(record)">{{ assetService(record) }}</span>
          </template>
          <template v-if="column.key === 'project'">
            <a-tag v-if="record.project_id && projectNameMap[record.project_id]" size="small">
              {{ projectNameMap[record.project_id] }}
            </a-tag>
            <span v-else style="color: var(--ant-color-text-tertiary)">—</span>
          </template>
          <template v-if="column.key === 'ownership'">
            <template v-if="record.responsible_person || record.department">
              <div
                :title="[
                  record.responsible_person ? `负责人: ${record.responsible_person}` : '',
                  record.department ? `部门: ${record.department}` : '',
                ].filter(Boolean).join(' · ')"
              >
                <div>{{ record.responsible_person || record.department }}</div>
                <div
                  v-if="record.responsible_person && record.department"
                  style="font-size: 12px; color: var(--ant-color-text-tertiary)"
                >
                  {{ record.department }}
                </div>
              </div>
            </template>
            <span v-else style="color: var(--ant-color-text-tertiary)">未分配</span>
          </template>
          <template v-if="column.key === 'last_scan_at'">
            {{ formatTime(record.last_scan_at) }}
          </template>
          <template v-if="column.key === 'vulnerabilities'">
            <span style="color: var(--ant-color-text-tertiary)">—</span>
          </template>
          <template v-if="column.key === 'risk_level'">
            <a-tag :color="riskColor[record.risk_level] || 'default'" size="small">
              {{ assetRiskLabel(record.risk_level) }}
            </a-tag>
          </template>
          <template v-if="column.key === 'status'">
            <a-tag :color="statusColor[record.status] || 'default'" size="small">
              {{ statusLabel[record.status] || record.status || '—' }}
            </a-tag>
          </template>
          <template v-if="column.key === 'action'">
            <a-button size="small" type="link" @click="openDetail(record)">详情</a-button>
            <a-button size="small" type="link" @click="openEdit(record)">编辑</a-button>
            <a-popconfirm
              title="确定删除该资产吗？"
              ok-text="删除"
              ok-type="danger"
              cancel-text="取消"
              @confirm="doDelete(record)"
            >
              <a-button size="small" type="link" danger>删除</a-button>
            </a-popconfirm>
          </template>
        </template>
        <template #emptyText>
          <a-empty description="暂无数据" />
        </template>
      </a-table>
    </a-card>

    <!-- 新增 / 编辑 modal -->
    <a-modal
      v-model:open="showEdit"
      :title="editingId ? '编辑资产' : '新增资产'"
      width="760px"
      :ok-text="editingId ? '保存' : '添加资产'"
      :confirm-loading="saving"
      @ok="doSave"
    >
      <a-form layout="vertical" :model="form">
        <a-form-item label="资产地址" required style="margin-bottom: 12px">
          <a-input
            v-model:value="form.target"
            placeholder="支持 URL / 域名 / IP:端口，如 https://example.com:8443"
            @blur="onTargetBlur"
          />
        </a-form-item>
        <a-row :gutter="16">
          <a-col :span="8">
            <a-form-item label="Host">
              <a-input v-model:value="form.host" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="IP">
              <a-input v-model:value="form.ip" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="域名">
              <a-input v-model:value="form.domain" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="端口">
              <a-input-number v-model:value="form.port" :min="1" :max="65535" style="width: 100%" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="协议">
              <a-input v-model:value="form.protocol" placeholder="如 https" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="服务指纹">
              <a-input v-model:value="form.server" placeholder="如 nginx/1.24" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="标题">
              <a-input v-model:value="form.title" placeholder="站点标题" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="状态">
              <a-select v-model:value="form.status" :options="statusOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="所属项目">
              <a-select
                v-model:value="form.project_id"
                :options="projectOptions"
                placeholder="暂不绑定"
                allow-clear
                show-search
                option-filter-prop="label"
              />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="国家 / 地区">
              <a-input v-model:value="form.country" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="省份">
              <a-input v-model:value="form.province" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="城市">
              <a-input v-model:value="form.city" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="负责人">
              <a-input v-model:value="form.responsible_person" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="部门">
              <a-input v-model:value="form.department" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="业务系统">
              <a-input v-model:value="form.business_system" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="环境">
              <a-select v-model:value="form.environment" allow-clear placeholder="选择环境" :options="envOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="8">
            <a-form-item label="重要性">
              <a-select v-model:value="form.criticality" allow-clear placeholder="选择重要性" :options="criticalityOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="24">
            <a-form-item label="标签">
              <a-select v-model:value="form.tags" mode="tags" placeholder="输入后回车创建标签" :token-separators="[',']" />
            </a-form-item>
          </a-col>
        </a-row>
      </a-form>
    </a-modal>

    <!-- 批量导入 modal -->
    <a-modal v-model:open="showImport" title="批量导入资产" width="760px" :footer="null">
      <a-alert
        type="info"
        show-icon
        message="使用 CSV 模板填写资产数据，支持拖拽上传；必填 target，或 host / ip / domain 中至少一项"
        style="margin-bottom: 12px"
      />
      <div style="display: flex; gap: 8px; margin-bottom: 12px">
        <a-button @click="downloadImportTemplate">下载 CSV 模板</a-button>
      </div>
      <a-upload-dragger
        accept=".csv"
        :show-upload-list="false"
        :before-upload="handleImportFile"
      >
        <p style="font-size: 14px; margin: 8px 0">点击或拖拽 CSV 文件到此区域</p>
        <p style="font-size: 12px; color: var(--ant-color-text-tertiary)">
          {{ importFileName || '尚未选择文件' }}
        </p>
      </a-upload-dragger>
      <template v-if="importRows.length">
        <div style="margin: 12px 0 8px; font-size: 13px">
          共 {{ importRows.length }} 行，{{ importValidCount }} 行有效，{{ importRows.length - importValidCount }} 行需修正
          <span v-if="importRows.length > 100" style="color: var(--ant-color-text-tertiary)">
            （仅展示前 100 行；提交时将处理全部 {{ importRows.length }} 行）
          </span>
        </div>
        <a-table
          :columns="importPreviewColumns"
          :data-source="importRows.slice(0, 100)"
          :row-key="(r: any) => r.rowNumber"
          size="small"
          :pagination="{ pageSize: 5 }"
          style="margin-bottom: 12px"
        >
          <template #bodyCell="{ column, record }">
            <template v-if="column.key === 'target'">
              {{ record.values.target || record.values.host || record.values.domain || record.values.ip || '-' }}
            </template>
            <template v-if="column.key === 'project'">
              {{ record.projectName || record.values.project || '-' }}
            </template>
            <template v-if="column.key === 'status'">
              {{ record.asset?.status || record.values.status || '-' }}
            </template>
            <template v-if="column.key === 'result'">
              <span v-if="record.error" style="color: var(--ant-color-error)">{{ record.error }}</span>
              <span v-else style="color: var(--ant-color-success)">通过</span>
            </template>
          </template>
        </a-table>
      </template>
      <div style="text-align: right">
        <a-button style="margin-right: 8px" @click="showImport = false">取消</a-button>
        <a-button
          type="primary"
          :loading="importing"
          :disabled="!importValidCount"
          @click="doImport"
        >
          导入 {{ importValidCount }} 条有效数据
        </a-button>
      </div>
    </a-modal>

    <!-- 绑定项目 modal -->
    <a-modal v-model:open="showBind" title="绑定项目" :confirm-loading="bindLoading" @ok="doBind">
      <p style="margin-bottom: 12px">将更新 {{ selectedRowKeys.length }} 个资产：</p>
      <a-select
        v-model:value="bindProjectId"
        :options="projectOptions"
        placeholder="请选择项目"
        style="width: 100%"
        show-search
        option-filter-prop="label"
      />
    </a-modal>

    <!-- 批量编辑 modal -->
    <a-modal v-model:open="showBulkEdit" title="批量编辑" width="640px" :confirm-loading="bulkLoading" @ok="doBulkEdit">
      <a-alert
        type="info"
        show-icon
        :message="`将修改 ${selectedRowKeys.length} 个资产；空白字段保持原值`"
        style="margin-bottom: 16px"
      />
      <a-form layout="vertical" :model="bulkForm">
        <a-row :gutter="16">
          <a-col :span="12">
            <a-form-item label="状态">
              <a-select v-model:value="bulkForm.status" allow-clear placeholder="不修改" :options="statusOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="负责人">
              <a-input v-model:value="bulkForm.responsible_person" placeholder="不修改" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="部门">
              <a-input v-model:value="bulkForm.department" placeholder="不修改" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="业务系统">
              <a-input v-model:value="bulkForm.business_system" placeholder="不修改" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="环境">
              <a-select v-model:value="bulkForm.environment" allow-clear placeholder="不修改" :options="envOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="重要性">
              <a-select v-model:value="bulkForm.criticality" allow-clear placeholder="不修改" :options="criticalityOptions" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="增加标签">
              <a-select v-model:value="bulkForm.add_tags" mode="tags" placeholder="输入后回车" :token-separators="[',']" />
            </a-form-item>
          </a-col>
          <a-col :span="12">
            <a-form-item label="删除标签">
              <a-select v-model:value="bulkForm.remove_tags" mode="tags" placeholder="输入后回车" :token-separators="[',']" />
            </a-form-item>
          </a-col>
        </a-row>
      </a-form>
    </a-modal>

    <!-- 合并重复 modal -->
    <a-modal v-model:open="showMerge" title="合并重复资产" :confirm-loading="mergeLoading" @ok="doMerge">
      <p style="margin-bottom: 12px">
        选择合并后保留的主资产，其余 {{ selectedRowKeys.length - 1 }} 项将被合并删除：
      </p>
      <a-select v-model:value="mergePrimaryId" style="width: 100%" placeholder="选择主资产">
        <a-select-option v-for="a in selectedRows" :key="a.id" :value="a.id">
          {{ assetTargetLabel(a) }}{{ a.port ? `:${a.port}` : '' }}
        </a-select-option>
      </a-select>
    </a-modal>

    <!-- 详情 drawer -->
    <a-drawer v-model:open="showDetail" title="资产详情" width="560px">
      <template v-if="detailAsset">
        <div style="margin-bottom: 16px">
          <div style="font-size: 18px; font-weight: 600; word-break: break-all">
            {{ assetTargetLabel(detailAsset) }}
          </div>
          <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-top: 4px">
            {{ [detailAsset.ip, detailAsset.domain, assetService(detailAsset) !== '—' ? assetService(detailAsset) : '']
              .filter(Boolean).join(' · ') || '—' }}
          </div>
          <div style="margin-top: 10px; display: flex; flex-wrap: wrap; gap: 6px">
            <a-tag :color="statusColor[detailAsset.status || ''] || 'default'">
              状态：{{ statusLabel[detailAsset.status || ''] || detailAsset.status || '—' }}
            </a-tag>
            <a-tag :color="riskColor[detailAsset.risk_level || ''] || 'default'">
              风险：{{ assetRiskLabel(detailAsset.risk_level) }}
            </a-tag>
            <a-tag v-if="detailAsset.source">
              来源：{{ sourceLabel[detailAsset.source] || detailAsset.source }}
            </a-tag>
            <a-tag v-if="[detailAsset.country, detailAsset.province, detailAsset.city].filter(Boolean).length">
              地区：{{ [detailAsset.country, detailAsset.province, detailAsset.city].filter(Boolean).join(' / ') }}
            </a-tag>
          </div>
        </div>
        <a-descriptions bordered size="small" :column="1">
          <a-descriptions-item label="所属项目">
            {{ detailAsset.project_id ? projectNameMap[detailAsset.project_id] || detailAsset.project_id : '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="状态">
            {{ statusLabel[detailAsset.status || ''] || detailAsset.status || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="Host">{{ detailAsset.host || '—' }}</a-descriptions-item>
          <a-descriptions-item label="IP">{{ detailAsset.ip || '—' }}</a-descriptions-item>
          <a-descriptions-item label="域名">{{ detailAsset.domain || '—' }}</a-descriptions-item>
          <a-descriptions-item label="服务">{{ assetService(detailAsset) }}</a-descriptions-item>
          <a-descriptions-item label="标题/指纹">{{ detailAsset.title || '—' }}</a-descriptions-item>
          <a-descriptions-item label="服务指纹">{{ detailAsset.server || '—' }}</a-descriptions-item>
          <a-descriptions-item label="地区">
            {{ [detailAsset.country, detailAsset.province, detailAsset.city].filter(Boolean).join(' / ') || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="负责人">{{ detailAsset.responsible_person || '—' }}</a-descriptions-item>
          <a-descriptions-item label="部门">{{ detailAsset.department || '—' }}</a-descriptions-item>
          <a-descriptions-item label="业务系统">{{ detailAsset.business_system || '—' }}</a-descriptions-item>
          <a-descriptions-item label="环境">
            {{ envLabel[detailAsset.environment || ''] || detailAsset.environment || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="重要性">
            {{ criticalityLabel[detailAsset.criticality || ''] || detailAsset.criticality || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="来源">
            {{ sourceLabel[detailAsset.source || ''] || detailAsset.source || '—' }}
          </a-descriptions-item>
          <a-descriptions-item label="标签">
            <template v-if="detailAsset.tags?.length">
              <a-tag v-for="t in detailAsset.tags" :key="t" size="small">{{ t }}</a-tag>
            </template>
            <span v-else>—</span>
          </a-descriptions-item>
          <a-descriptions-item label="首次发现">{{ formatTime(detailAsset.first_seen) }}</a-descriptions-item>
          <a-descriptions-item label="最近发现">{{ formatTime(detailAsset.last_seen) }}</a-descriptions-item>
          <a-descriptions-item label="上次扫描">{{ formatTime(detailAsset.last_scan_at) }}</a-descriptions-item>
          <a-descriptions-item label="相关漏洞">—</a-descriptions-item>
        </a-descriptions>
        <div style="margin-top: 16px; text-align: right">
          <a-button type="primary" @click="editFromDetail">编辑</a-button>
        </div>
      </template>
    </a-drawer>
  </div>
</template>
