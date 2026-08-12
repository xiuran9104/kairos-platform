<script lang="ts" setup>
import type {
  AssetImportItem,
  InfocollectSearchResult,
} from '#/api/pent/ops-infocollect';

import { computed, onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';

import {
  getInfocollectProviders,
  importAssetsForInfocollect,
  searchInfocollect,
} from '#/api/pent/ops-infocollect';

// ── 数据源配置（对齐原版 INFO_COLLECT_PROVIDERS）─────────────────────────────
interface SyntaxGuide {
  summary: string;
  docsUrl: string;
  sections: Array<[string, string[]]>;
}
interface ProviderConfig {
  label: string;
  placeholder: string;
  nlPlaceholder: string;
  hint: string;
  parseHint: string;
  maxSize: number;
  sizeHint: string;
  fullOption: null | { label: string; hint: string };
  fields: string;
  presets: Array<[string, string]>;
  fieldPresets: Array<[string, string]>;
  syntaxGuide: SyntaxGuide;
}

const PROVIDER_CONFIGS: Record<string, ProviderConfig> = {
  fofa: {
    label: 'FOFA',
    placeholder: '例如：app="Apache" && country="CN"',
    nlPlaceholder: '例如：找美国 Missouri 的 Apache 站点，标题包含 Home',
    hint: '查询语法参考 FOFA 文档，支持 && / || / () 等。',
    parseHint: '解析后会弹窗展示 FOFA 语法（可编辑），确认无误后再填入查询框并执行查询。',
    maxSize: 10_000,
    sizeHint: 'FOFA 返回数量上限与账号权限相关，前端最多允许 10000。',
    fullOption: {
      label: '完整模式',
      hint: '向 FOFA 传 full=true，返回更完整/更实时的数据，可能消耗更多额度。',
    },
    fields: 'host,ip,port,domain,title,protocol,country,province,city,server',
    presets: [
      ['Apache + 中国', 'app="Apache" && country="CN"'],
      ['登录页 + 中国', 'title="登录" && country="CN"'],
      ['指定域名', 'domain="example.com"'],
      ['指定 IP', 'ip="1.1.1.1"'],
    ],
    fieldPresets: [
      ['最小字段', 'host,ip,port,domain'],
      ['Web 常用', 'host,title,ip,port,domain,protocol,server,icp,country,province,city'],
      [
        '情报增强',
        'host,ip,port,domain,title,protocol,country,province,city,server,as_number,as_organization,icp,header,banner',
      ],
    ],
    syntaxGuide: {
      summary: 'FOFA 使用 field="value" 精确匹配，支持 &&、||、! 和括号组合；字符串建议用双引号包裹。',
      docsUrl: 'https://en.fofa.info/api',
      sections: [
        [
          '常用字段',
          [
            'app="Apache"',
            'title="后台管理"',
            'body="Powered by"',
            'domain="example.com"',
            'host="https://example.com"',
            'ip="1.1.1.1"',
            'port="443"',
            'country="CN"',
            'city="Hangzhou"',
            'server="nginx"',
          ],
        ],
        [
          '组合写法',
          [
            'app="nginx" && country="CN"',
            'title="login" || title="登录"',
            '(app="Apache" || app="nginx") && port="443"',
            'domain="example.com" && !title="404"',
          ],
        ],
        [
          '场景示例',
          [
            'cert="example.com" && port="443"',
            'header="JSESSIONID" && country="CN"',
            'icon_hash="-247388890"',
            'fid="sZyXkR9e" && domain="example.com"',
          ],
        ],
      ],
    },
  },
  zoomeye: {
    label: 'ZoomEye',
    placeholder: '例如：app="Apache" && country="CN"',
    nlPlaceholder: '例如：找中国的 SSH 服务，排除蜜罐',
    hint: 'ZoomEye 支持 app/title/domain/ip/port/country/city 等语法。',
    parseHint: '解析后会弹窗展示 ZoomEye 语法（可编辑），确认无误后再填入查询框并执行查询。',
    maxSize: 10_000,
    sizeHint: 'ZoomEye pagesize 最高支持到 10000，实际额度以账号为准。',
    fullOption: null,
    fields: 'ip,port,domain,hostname,title,service,app,country,city',
    presets: [
      ['Apache + 中国', 'app="Apache" && country="CN"'],
      ['SSH 服务', 'service="ssh"'],
      ['指定域名', 'domain="example.com"'],
      ['指定 IP', 'ip="1.1.1.1"'],
    ],
    fieldPresets: [
      ['最小字段', 'ip,port,domain,hostname'],
      ['Web 常用', 'ip,port,domain,hostname,title,service,app,country,city'],
      ['情报增强', 'ip,port,domain,hostname,title,service,app,country,city,org,isp,ssl'],
    ],
    syntaxGuide: {
      summary: 'ZoomEye 支持字段检索、引号短语、AND/OR/NOT 与括号组合；字段名以官方控制台实际支持为准。',
      docsUrl: 'https://www.zoomeye.ai/help',
      sections: [
        [
          '常用字段',
          [
            'app="Apache"',
            'service="ssh"',
            'title="登录"',
            'domain="example.com"',
            'hostname="example.com"',
            'ip="1.1.1.1"',
            'port=443',
            'country="CN"',
            'city="Beijing"',
            'org="Tencent"',
          ],
        ],
        [
          '组合写法',
          [
            'app="nginx" AND country="CN"',
            'service="http" AND (title="login" OR title="登录")',
            'domain="example.com" AND NOT app="cloudflare"',
            'port=443 AND country="US"',
          ],
        ],
        [
          '场景示例',
          [
            'ssl.cert.fingerprint="SHA256值"',
            'iconhash="-247388890"',
            'service="rdp" AND country="CN"',
            'app="Elasticsearch" AND port=9200',
          ],
        ],
      ],
    },
  },
  quake: {
    label: 'Quake',
    placeholder: '例如：service.name:"http" AND country_cn:"中国"',
    nlPlaceholder: '例如：找中国的 HTTP 服务，标题包含登录',
    hint: 'Quake 使用 DSL 语法，常见字段如 service.name、domain、ip、port、country_cn。',
    parseHint: '解析后会弹窗展示 Quake DSL（可编辑），确认无误后再填入查询框并执行查询。',
    maxSize: 10_000,
    sizeHint: 'Quake size 会消耗积分，建议按需控制返回数量。',
    fullOption: {
      label: '最新数据',
      hint: '向 Quake 传 latest=true，优先查询最新数据。',
    },
    fields: 'ip,port,domain,service.name,service.http.title,location.country_cn,location.province_cn,location.city_cn',
    presets: [
      ['HTTP + 中国', 'service.name:"http" AND country_cn:"中国"'],
      ['443 端口', 'port:443'],
      ['指定域名', 'domain:"example.com"'],
      ['指定 IP', 'ip:"1.1.1.1"'],
    ],
    fieldPresets: [
      ['最小字段', 'ip,port,domain'],
      ['Web 常用', 'ip,port,domain,service.name,service.http.title,location.country_cn,location.city_cn'],
      [
        '情报增强',
        'ip,port,domain,service.name,service.http.title,service.http.server,location.country_cn,location.province_cn,location.city_cn,asn',
      ],
    ],
    syntaxGuide: {
      summary: 'Quake 使用 Lucene/DSL 风格查询，常见形式是 field:"value"，逻辑运算符通常使用 AND、OR、NOT。',
      docsUrl: 'https://quake.360.net/quake/#/help',
      sections: [
        [
          '常用字段',
          [
            'service.name:"http"',
            'service.http.title:"登录"',
            'service.http.server:"nginx"',
            'domain:"example.com"',
            'ip:"1.1.1.1"',
            'port:443',
            'country_cn:"中国"',
            'province_cn:"浙江"',
            'city_cn:"杭州"',
          ],
        ],
        [
          '组合写法',
          [
            'service.name:"http" AND country_cn:"中国"',
            '(service.name:"http" OR service.name:"https") AND port:443',
            'domain:"example.com" AND NOT service.http.title:"404"',
            'service.http.title:"login" AND port:443',
          ],
        ],
        [
          '场景示例',
          [
            'service.http.favicon.hash:"-247388890"',
            'service.http.response.header:"JSESSIONID"',
            'service.name:"ssh" AND country_cn:"中国"',
            'service.http.title:"Dashboard" AND NOT ip:"127.0.0.1"',
          ],
        ],
      ],
    },
  },
  shodan: {
    label: 'Shodan',
    placeholder: '例如：product:nginx country:CN',
    nlPlaceholder: '例如：找中国的 nginx 资产，端口 443',
    hint: 'Shodan 使用 filter:value 语法，常见字段如 product、port、country、org。',
    parseHint: '解析后会弹窗展示 Shodan filter 语法（可编辑），确认无误后再填入查询框并执行查询。',
    maxSize: 1000,
    sizeHint: 'Shodan 官方每页 100 条；后端会自动翻页聚合，单次最多 1000 条以控制额度消耗。',
    fullOption: null,
    fields: 'ip_str,port,hostnames,domains,org,isp,location.country_name,location.city,product,transport',
    presets: [
      ['Nginx + 中国', 'product:nginx country:CN'],
      ['SSH 服务', 'port:22'],
      ['证书域名', 'ssl.cert.subject.cn:example.com'],
      ['Amazon 443', 'org:"Amazon" port:443'],
    ],
    fieldPresets: [
      ['最小字段', 'ip_str,port,hostnames,domains'],
      ['Web 常用', 'ip_str,port,hostnames,domains,product,org,location.country_name,location.city'],
      [
        '情报增强',
        'ip_str,port,hostnames,domains,org,isp,asn,location.country_name,location.city,product,transport,ssl.cert.subject.cn',
      ],
    ],
    syntaxGuide: {
      summary: 'Shodan 默认搜索 banner data；精确条件使用 filter:value，值含空格时用双引号，多个过滤器并列表示收窄结果。',
      docsUrl: 'https://help.shodan.io/the-basics/search-query-fundamentals',
      sections: [
        [
          '常用过滤器',
          [
            'product:nginx',
            'port:443',
            'country:CN',
            'city:Shanghai',
            'org:"Amazon"',
            'asn:AS15169',
            'hostname:example.com',
            'ssl.cert.subject.cn:example.com',
            'http.title:"Dashboard"',
          ],
        ],
        [
          '组合写法',
          [
            'product:nginx country:CN',
            'apache port:443 country:DE',
            'org:"Amazon" port:443',
            'ssl.cert.subject.cn:example.com port:443',
          ],
        ],
        [
          '场景示例',
          [
            'http.title:"login" country:CN',
            'ssl:true port:443 hostname:example.com',
            'vuln:CVE-2021-41773',
            'has_screenshot:true product:nginx',
          ],
        ],
      ],
    },
  },
};

const FALLBACK_PROVIDER: ProviderConfig = PROVIDER_CONFIGS.fofa!;

// ── 表单状态（localStorage 持久化，对齐原版行为）────────────────────────────
const FORM_STORAGE_KEY = 'info-collect-fofa-form';
const HIDDEN_FIELDS_STORAGE_KEY = 'info-collect-fofa-hidden-fields';

const activeTab = ref('online');
const source = ref('fofa');
const nlQuery = ref('');
const syntaxQuery = ref('');
const returnConfig = ref({ size: 100, page: 1, full: false });
const fieldsText = ref(FALLBACK_PROVIDER.fields);
const syntaxGuideExpanded = ref(false);

const providerCfg = computed<ProviderConfig>(
  () => PROVIDER_CONFIGS[source.value] || FALLBACK_PROVIDER,
);

function saveFormToStorage() {
  try {
    localStorage.setItem(
      FORM_STORAGE_KEY,
      JSON.stringify({
        provider: source.value,
        query: syntaxQuery.value,
        size: returnConfig.value.size,
        page: returnConfig.value.page,
        fields: fieldsText.value,
        full: returnConfig.value.full,
      }),
    );
  } catch {
    /* 静默 */
  }
}

function loadFormFromStorage() {
  try {
    const raw = localStorage.getItem(FORM_STORAGE_KEY);
    if (!raw) return;
    const data = JSON.parse(raw);
    if (!data || typeof data !== 'object') return;
    if (typeof data.provider === 'string') source.value = data.provider;
    if (typeof data.query === 'string') syntaxQuery.value = data.query;
    if (typeof data.size === 'number') returnConfig.value.size = data.size;
    if (typeof data.page === 'number') returnConfig.value.page = data.page;
    if (typeof data.fields === 'string' && data.fields.trim()) fieldsText.value = data.fields;
    if (typeof data.full === 'boolean') returnConfig.value.full = data.full;
  } catch {
    /* 静默 */
  }
}

function loadHiddenFields(): Set<string> {
  try {
    const arr = JSON.parse(localStorage.getItem(HIDDEN_FIELDS_STORAGE_KEY) || '[]');
    return new Set(Array.isArray(arr) ? arr.filter((x) => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

// ── 数据源列表（API）────────────────────────────────────────────────────────
const providers = ref<Array<{ id: string; name: string; configured: boolean }>>([]);

const providerOptions = computed(() => {
  const known = Object.keys(PROVIDER_CONFIGS);
  const fromApi = providers.value.map((p) => ({
    label: p.configured ? p.name : `${p.name}（未配置）`,
    value: p.id,
    disabled: !p.configured,
  }));
  const missing = known
    .filter((id) => !fromApi.some((o) => o.value === id))
    .map((id) => ({ label: PROVIDER_CONFIGS[id]!.label, value: id, disabled: false }));
  return fromApi.length ? [...fromApi, ...missing] : missing;
});

async function fetchProviders() {
  try {
    providers.value = (await getInfocollectProviders()) || [];
    if (providers.value.length && !providers.value.some((p) => p.id === source.value)) {
      source.value = providers.value[0]?.id ?? 'fofa';
      onProviderChange();
    }
  } catch {
    /* 静默：保留本地内置数据源 */
  }
}

function onProviderChange() {
  syntaxGuideExpanded.value = false;
  fieldsText.value = providerCfg.value.fields;
  if (!providerCfg.value.fullOption) returnConfig.value.full = false;
  if (returnConfig.value.size > providerCfg.value.maxSize) {
    returnConfig.value.size = providerCfg.value.maxSize;
  }
  saveFormToStorage();
}

// ── 查询示例 / 字段模板 / 语法指南 ──────────────────────────────────────────
function applyQueryPreset(preset: string) {
  syntaxQuery.value = preset.trim();
  saveFormToStorage();
}

function applyFieldsPreset(preset: string) {
  fieldsText.value = preset.trim();
  saveFormToStorage();
}

function toggleSyntaxGuide() {
  syntaxGuideExpanded.value = !syntaxGuideExpanded.value;
}

function aiParse() {
  message.info('AI 解析暂未接入');
}

function resetForm() {
  syntaxQuery.value = '';
  nlQuery.value = '';
  returnConfig.value = { size: 100, page: 1, full: false };
  fieldsText.value = providerCfg.value.fields;
  searchResult.value = null;
  searched.value = false;
  selectedRowKeys.value = [];
  saveFormToStorage();
}

// ── 查询与结果 ─────────────────────────────────────────────────────────────
const searching = ref(false);
const searched = ref(false);
const searchResult = ref<InfocollectSearchResult | null>(null);

const resultFields = computed(() => searchResult.value?.fields || []);
const hiddenFields = ref<Set<string>>(loadHiddenFields());
const visibleFields = computed(() => resultFields.value.filter((f) => !hiddenFields.value.has(f)));
const resultRows = computed(() => searchResult.value?.results || []);

const resultsMeta = computed(() => {
  const r = searchResult.value;
  if (!searched.value || !r) return '-';
  return `${providerCfg.value.label} · 共 ${r.total} 条 · 本页 ${resultRows.value.length} 条 · page=${r.page} · size=${r.size}`;
});

async function submitSearch() {
  const query = syntaxQuery.value.trim();
  if (!query) {
    message.warning('请输入查询语法');
    return;
  }
  const maxSize = providerCfg.value.maxSize || 10_000;
  if (returnConfig.value.size > maxSize) {
    returnConfig.value.size = maxSize;
    message.info(`${providerCfg.value.label} 单次最多返回 ${maxSize} 条，已自动调整。`);
  }
  saveFormToStorage();
  searching.value = true;
  try {
    const fields = fieldsText.value
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    searchResult.value = await searchInfocollect({
      provider: source.value,
      query,
      size: returnConfig.value.size,
      page: returnConfig.value.page,
      fields: returnConfig.value.full ? undefined : fields,
      full: returnConfig.value.full,
    });
    searched.value = true;
    selectedRowKeys.value = [];
    // 修剪隐藏字段：只保留当前 fields 中存在的
    const allowed = new Set(resultFields.value);
    hiddenFields.value = new Set([...hiddenFields.value].filter((f) => allowed.has(f)));
    persistHiddenFields();
  } catch (error: any) {
    message.error(error?.message || '查询失败，请检查数据源配置与查询语法');
    searchResult.value = null;
    searched.value = true;
  } finally {
    searching.value = false;
  }
}

// ── 表格列 ─────────────────────────────────────────────────────────────────
const resultColumns = computed(() => [
  ...visibleFields.value.map((f) => ({
    title: f,
    dataIndex: f,
    key: f,
    ellipsis: true,
  })),
  { title: '操作', key: '__actions', width: 110, fixed: 'right' as const },
]);

function persistHiddenFields() {
  try {
    localStorage.setItem(HIDDEN_FIELDS_STORAGE_KEY, JSON.stringify([...hiddenFields.value]));
  } catch {
    /* 静默 */
  }
}

function toggleColumn(field: string, visible: boolean) {
  const next = new Set(hiddenFields.value);
  if (visible) next.delete(field);
  else next.add(field);
  hiddenFields.value = next;
  persistHiddenFields();
}

function showAllColumns() {
  hiddenFields.value = new Set();
  persistHiddenFields();
}

function hideAllColumns() {
  const keep = ['host', 'ip', 'domain'].find((x) => resultFields.value.includes(x));
  hiddenFields.value = new Set(resultFields.value.filter((f) => f !== keep));
  persistHiddenFields();
}

// ── 行选择 / 目标推断 ───────────────────────────────────────────────────────
const selectedRowKeys = ref<number[]>([]);
const importing = ref(false);

const rowSelection = computed(() => ({
  selectedRowKeys: selectedRowKeys.value,
  onChange: (keys: any[]) => {
    selectedRowKeys.value = keys as number[];
  },
}));

function inferTarget(row: Record<string, any>): string {
  const host = row.host == null ? '' : String(row.host).trim();
  if (host) return host;
  const domain = row.domain == null ? '' : String(row.domain).trim();
  const ip = row.ip == null ? '' : String(row.ip).trim();
  const port = row.port == null ? '' : String(row.port).trim();
  const protocol = row.protocol == null ? '' : String(row.protocol).trim().toLowerCase();
  const base = domain || ip;
  if (!base) return '';
  if (port) {
    const p = Number.parseInt(port, 10);
    if (p === 80 || p === 443) return `${p === 443 ? 'https' : 'http'}://${base}:${p}`;
    if (protocol === 'http' || protocol === 'https') return `${protocol}://${base}:${port}`;
    return `${base}:${port}`;
  }
  return base;
}

function rowToAsset(row: Record<string, any>): AssetImportItem | null {
  const target = inferTarget(row);
  if (!target) return null;
  return {
    target,
    host: row.host ?? undefined,
    ip: row.ip ?? undefined,
    domain: row.domain ?? undefined,
    port: row.port ?? undefined,
    protocol: row.protocol ?? undefined,
    title: row.title ?? undefined,
    server: row.server ?? undefined,
    country: row.country ?? undefined,
    province: row.province ?? undefined,
    city: row.city ?? undefined,
  };
}

async function importRows(indexes: number[]) {
  const assets = indexes
    .map((i) => resultRows.value[i])
    .filter((row): row is Record<string, any> => !!row)
    .map((row) => rowToAsset(row))
    .filter((a): a is AssetImportItem => !!a);
  if (!assets.length) {
    message.warning('所选行缺少 host/ip/domain，无法入库');
    return;
  }
  importing.value = true;
  try {
    const res = await importAssetsForInfocollect({
      assets,
      source: `infogather-${source.value}`,
    });
    message.success(`入库完成：新增 ${res.created}，更新 ${res.updated}，跳过 ${res.skipped}`);
  } catch {
    message.error('资产入库失败');
  } finally {
    importing.value = false;
  }
}

function importSelected() {
  if (!selectedRowKeys.value.length) {
    message.warning('请先勾选需要入库的行');
    return;
  }
  importRows(selectedRowKeys.value);
}

async function copyTarget(row: Record<string, any>) {
  const target = inferTarget(row);
  if (!target) {
    message.warning('没有可复制的目标');
    return;
  }
  try {
    await navigator.clipboard.writeText(target);
    message.success('已复制目标');
  } catch {
    message.warning(`复制失败，请手动复制：${target}`);
  }
}

function isHttpLink(text: string): boolean {
  return text.startsWith('http://') || text.startsWith('https://');
}

// ── 单元格详情弹窗 ──────────────────────────────────────────────────────────
const cellModal = ref({ open: false, field: '', text: '' });

function openCellDetail(field: string, text: string) {
  if (!text) return;
  cellModal.value = { open: true, field, text };
}

async function copyCellText() {
  try {
    await navigator.clipboard.writeText(cellModal.value.text);
    message.success('已复制');
  } catch {
    message.error('复制失败');
  }
}

// ── 导出（纯前端，对齐原版 exportFofaResults）───────────────────────────────
function downloadBlob(content: string, filename: string, mime: string) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.append(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

function csvEscape(value: any): string {
  if (value === null || value === undefined) return '""';
  return `"${String(value).replaceAll('"', '""')}"`;
}

function exportResults(format: 'csv' | 'json') {
  const r = searchResult.value;
  if (!r || !resultRows.value.length) {
    message.warning('暂无可导出的结果');
    return;
  }
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  const ts = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  const provider = r.provider || source.value;

  if (format === 'json') {
    downloadBlob(
      JSON.stringify(
        {
          provider,
          query: syntaxQuery.value,
          total: r.total,
          page: r.page,
          size: r.size,
          fields: r.fields,
          results: r.results,
        },
        null,
        2,
      ),
      `${provider}_results_${ts}.json`,
      'application/json;charset=utf-8',
    );
    return;
  }

  const header = visibleFields.value;
  const lines = [
    header.map(csvEscape).join(','),
    ...resultRows.value.map((row) => header.map((f) => csvEscape(row?.[f])).join(',')),
  ];
  downloadBlob(`\uFEFF${lines.join('\n')}`, `${provider}_results_${ts}.csv`, 'text/csv;charset=utf-8');
}

// ── 被动收集 ───────────────────────────────────────────────────────────────
const aiTarget = ref('');
const aiMode = ref('full');
const aiModeOptions = [
  { label: '全流程（股权→备案→DNS→供应链→泄露→测绘）', value: 'full' },
  { label: '股权穿透（小蓝本→子公司域名）', value: 'owner' },
  { label: '备案反查（同主体域名）', value: 'icp' },
  { label: '供应链反查（FOFA 证书）', value: 'supply' },
  { label: '在线测绘（FOFA/Quake/ZoomEye）', value: 'online' },
];

function aiCollect() {
  message.info('暂未接入');
}

// ── JSON 结果导入（对齐原版 previewPassiveJson / importPassiveAssets）───────
const PASSIVE_FIELDS = [
  'host',
  'ip',
  'port',
  'domain',
  'protocol',
  'title',
  'server',
  'country',
  'province',
  'city',
  'source',
];
const jsonInput = ref('');
const jsonRows = ref<Array<Record<string, any>>>([]);
const jsonImporting = ref(false);

const jsonColumns = [
  { title: 'host', dataIndex: 'host', key: 'host' },
  { title: 'ip', dataIndex: 'ip', key: 'ip' },
  { title: 'port', dataIndex: 'port', key: 'port' },
  { title: 'domain', dataIndex: 'domain', key: 'domain' },
  { title: 'protocol', dataIndex: 'protocol', key: 'protocol' },
  { title: 'title', dataIndex: 'title', key: 'title' },
  { title: 'source', dataIndex: 'source', key: 'source' },
];

function previewJson() {
  const raw = jsonInput.value.trim();
  if (!raw) {
    message.warning('请先粘贴 JSON');
    jsonRows.value = [];
    return;
  }
  let rows: any[] = [];
  try {
    const parsed = JSON.parse(raw);
    rows = Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    // 回退：逐行 JSON
    rows = [];
    for (const line of raw.split(/\r?\n/)) {
      const t = line.trim();
      if (!t) continue;
      try {
        rows.push(JSON.parse(t));
      } catch {
        /* 跳过坏行 */
      }
    }
  }
  const cleaned = rows
    .filter((r) => r && typeof r === 'object')
    .map((r) => {
      const asset: Record<string, any> = {};
      for (const k of PASSIVE_FIELDS) {
        if (r[k] !== undefined && r[k] !== null && String(r[k]).trim() !== '') {
          asset[k] = k === 'port' ? Number(r[k]) : String(r[k]).trim();
        }
      }
      if (!asset.source) asset.source = 'infogather';
      return asset;
    })
    .filter((a) => a.host || a.ip || a.domain);
  jsonRows.value = cleaned;
  if (!cleaned.length) {
    message.error('没有可入库的有效资产（host/ip/domain 至少一个）');
    return;
  }
  message.success(`解析出 ${cleaned.length} 条资产`);
}

async function importJson() {
  if (!jsonRows.value.length) {
    message.warning('请先粘贴 JSON 并点击「解析预览」');
    return;
  }
  const assets = jsonRows.value
    .map((row) => ({ ...row, target: inferTarget(row) }))
    .filter((a) => a.target);
  jsonImporting.value = true;
  try {
    const res = await importAssetsForInfocollect({
      assets,
      source: jsonRows.value[0]?.source || 'infogather',
    });
    message.success(`导入完成：新增 ${res.created} 条，更新 ${res.updated} 条，跳过 ${res.skipped} 条`);
    jsonRows.value = [];
    jsonInput.value = '';
  } catch {
    message.error('资产导入失败');
  } finally {
    jsonImporting.value = false;
  }
}

onMounted(() => {
  loadFormFromStorage();
  fetchProviders();
});
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-page-header title="信息收集" sub-title="在线测绘 + 被动收集">
      <template #extra>
        <a-button @click="resetForm">重置</a-button>
        <a-button type="primary" style="margin-left: 8px" :loading="searching" @click="submitSearch">
          确定
        </a-button>
      </template>
    </a-page-header>

    <a-tabs v-model:active-key="activeTab" style="margin-top: 8px">
      <!-- ── 在线测绘 ── -->
      <a-tab-pane key="online" tab="在线测绘">
        <a-row :gutter="[16, 16]">
          <!-- 01 数据源 -->
          <a-col :xs="24" :lg="8">
            <a-card size="small" style="height: 100%">
              <template #title>
                <a-tag color="blue">01</a-tag><span style="font-size: 13px">数据源</span>
              </template>
              <a-form-item label="数据源" style="margin-bottom: 8px">
                <a-select
                  v-model:value="source"
                  :options="providerOptions"
                  style="width: 100%"
                  @change="onProviderChange"
                />
              </a-form-item>
              <div style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                支持 FOFA、ZoomEye、Quake、Shodan；API Key 可在系统设置或环境变量中配置。
              </div>
            </a-card>
          </a-col>

          <!-- 02 查询方式 -->
          <a-col :xs="24" :lg="16">
            <a-card size="small" style="height: 100%">
              <template #title>
                <a-tag color="blue">02</a-tag><span style="font-size: 13px">查询方式</span>
              </template>

              <!-- 自然语言 -->
              <div style="margin-bottom: 4px; font-size: 13px">
                自然语言（可选，AI 解析为 {{ providerCfg.label }} 语法）
              </div>
              <div style="display: flex; gap: 8px; align-items: flex-start">
                <a-textarea
                  v-model:value="nlQuery"
                  :rows="1"
                  :placeholder="providerCfg.nlPlaceholder"
                  style="flex: 1"
                />
                <a-button @click="aiParse">AI 解析</a-button>
              </div>
              <div style="margin: 4px 0 12px; font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ providerCfg.parseHint }}
              </div>

              <!-- 查询语法 -->
              <div style="margin-bottom: 4px; font-size: 13px">
                {{ providerCfg.label }} 查询语法（可编辑，可直接查询）
              </div>
              <div style="display: flex; gap: 8px; align-items: flex-start">
                <a-textarea
                  v-model:value="syntaxQuery"
                  :rows="1"
                  :placeholder="providerCfg.placeholder"
                  style="flex: 1"
                  @change="saveFormToStorage"
                />
                <a-button type="primary" :loading="searching" @click="submitSearch">查询</a-button>
              </div>
              <div style="margin: 4px 0 8px; font-size: 12px; color: var(--ant-color-text-tertiary)">
                {{ providerCfg.hint }}
              </div>
              <div style="display: flex; flex-wrap: wrap; gap: 4px">
                <a-tag
                  v-for="[label, q] in providerCfg.presets"
                  :key="label"
                  style="cursor: pointer"
                  :title="q"
                  @click="applyQueryPreset(q)"
                >
                  {{ label }}
                </a-tag>
              </div>

              <!-- 语法指南 -->
              <div
                style="
                  margin-top: 8px;
                  padding: 8px 12px;
                  border: 1px solid var(--ant-color-border-secondary);
                  border-radius: 6px;
                  font-size: 12px;
                "
              >
                <div style="display: flex; justify-content: space-between; gap: 8px; align-items: center">
                  <span style="color: var(--ant-color-text-secondary)">
                    {{ providerCfg.syntaxGuide.summary }}
                  </span>
                  <span style="flex-shrink: 0">
                    <a :href="providerCfg.syntaxGuide.docsUrl" target="_blank" rel="noopener noreferrer">
                      官方文档
                    </a>
                    <a-button size="small" type="link" style="padding: 0 4px" @click="toggleSyntaxGuide">
                      {{ syntaxGuideExpanded ? '收起示例' : '展开示例' }}
                    </a-button>
                  </span>
                </div>
                <div v-if="syntaxGuideExpanded" style="margin-top: 8px">
                  <div
                    v-for="[sectionTitle, examples] in providerCfg.syntaxGuide.sections"
                    :key="sectionTitle"
                    style="margin-bottom: 8px"
                  >
                    <div style="font-weight: 600; margin-bottom: 4px">{{ sectionTitle }}</div>
                    <div style="display: flex; flex-wrap: wrap; gap: 4px">
                      <a-tag
                        v-for="example in examples"
                        :key="example"
                        style="cursor: pointer"
                        title="填入查询框"
                        @click="applyQueryPreset(example)"
                      >
                        {{ example }}
                      </a-tag>
                    </div>
                  </div>
                </div>
              </div>
            </a-card>
          </a-col>

          <!-- 03 返回配置 -->
          <a-col :span="24">
            <a-card size="small">
              <template #title>
                <a-tag color="blue">03</a-tag><span style="font-size: 13px">返回配置</span>
              </template>
              <a-form layout="inline">
                <a-form-item label="返回数量">
                  <a-input-number
                    v-model:value="returnConfig.size"
                    :min="1"
                    :max="providerCfg.maxSize"
                    @change="saveFormToStorage"
                  />
                </a-form-item>
                <a-form-item label="页码">
                  <a-input-number
                    v-model:value="returnConfig.page"
                    :min="1"
                    @change="saveFormToStorage"
                  />
                </a-form-item>
                <a-form-item v-if="providerCfg.fullOption" :label="undefined">
                  <a-checkbox v-model:checked="returnConfig.full" @change="saveFormToStorage">
                    {{ providerCfg.fullOption.label }}
                  </a-checkbox>
                </a-form-item>
                <a-form-item>
                  <span style="font-size: 12px; color: var(--ant-color-text-tertiary)">
                    {{ providerCfg.fullOption && returnConfig.full ? providerCfg.fullOption.hint : providerCfg.sizeHint }}
                  </span>
                </a-form-item>
              </a-form>
            </a-card>
          </a-col>

          <!-- 04 返回字段 -->
          <a-col :span="24">
            <a-card size="small">
              <template #title>
                <a-tag color="blue">04</a-tag><span style="font-size: 13px">返回字段名（逗号分隔）</span>
              </template>
              <a-input v-model:value="fieldsText" @change="saveFormToStorage" />
              <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-top: 8px">
                <a-tag
                  v-for="[label, fields] in providerCfg.fieldPresets"
                  :key="label"
                  style="cursor: pointer"
                  :title="fields"
                  @click="applyFieldsPreset(fields)"
                >
                  {{ label }}
                </a-tag>
              </div>
            </a-card>
          </a-col>
        </a-row>

        <!-- 查询结果 -->
        <a-card size="small" style="margin-top: 16px">
          <template #title>
            <span style="font-size: 13px">查询结果</span>
            <span style="margin-left: 12px; font-size: 12px; font-weight: normal; color: var(--ant-color-text-tertiary)">
              {{ resultsMeta }}
            </span>
          </template>
          <template #extra>
            <span style="margin-right: 8px; font-size: 12px; color: var(--ant-color-text-tertiary)">
              已选择 {{ selectedRowKeys.length }} 条
            </span>
            <a-popover trigger="click" placement="bottomRight">
              <template #title>
                <div style="display: flex; justify-content: space-between; align-items: center; gap: 8px">
                  <span>显示字段</span>
                  <span>
                    <a-button size="small" type="link" style="padding: 0 4px" @click="showAllColumns">全选</a-button>
                    <a-button size="small" type="link" style="padding: 0 4px" @click="hideAllColumns">全不选</a-button>
                  </span>
                </div>
              </template>
              <template #content>
                <div style="display: flex; flex-direction: column; gap: 4px; max-height: 320px; overflow: auto">
                  <a-checkbox
                    v-for="f in resultFields"
                    :key="f"
                    :checked="!hiddenFields.has(f)"
                    @change="(e: any) => toggleColumn(f, e.target.checked)"
                  >
                    {{ f }}
                  </a-checkbox>
                  <a-empty v-if="!resultFields.length" description="暂无字段" :image-style="{ height: '40px' }" />
                </div>
              </template>
              <a-button size="small">列</a-button>
            </a-popover>
            <a-button size="small" style="margin-left: 8px" title="导出当前结果为 CSV（UTF-8，兼容中文）" @click="exportResults('csv')">
              导出 CSV
            </a-button>
            <a-button size="small" style="margin-left: 8px" title="导出当前结果为 JSON" @click="exportResults('json')">
              导出 JSON
            </a-button>
            <a-button
              size="small"
              type="primary"
              style="margin-left: 8px"
              :disabled="!selectedRowKeys.length"
              :loading="importing"
              @click="importSelected"
            >
              入库所选
            </a-button>
          </template>

          <a-table
            :columns="resultColumns"
            :data-source="resultRows"
            :loading="searching"
            :row-selection="rowSelection"
            :row-key="(_: any, index: number) => index"
            :pagination="false"
            size="small"
            :scroll="{ x: true }"
          >
            <template #bodyCell="{ column, text, record, index }">
              <template v-if="column.key === '__actions'">
                <a-button size="small" type="link" style="padding: 0 4px" title="复制目标" @click="copyTarget(record)">
                  复制
                </a-button>
                <a-button
                  size="small"
                  type="link"
                  style="padding: 0 4px"
                  title="入库该行"
                  :loading="importing"
                  @click="importRows([index])"
                >
                  入库
                </a-button>
              </template>
              <template v-else-if="column.dataIndex === 'host' && text && isHttpLink(String(text))">
                <a :href="text" target="_blank" rel="noopener noreferrer" @click.stop>{{ text }}</a>
              </template>
              <template v-else>
                <span
                  :title="text === null || text === undefined ? '' : String(text)"
                  :style="text ? 'cursor: pointer' : ''"
                  @click="openCellDetail(String(column.dataIndex), text === null || text === undefined ? '' : String(text))"
                >
                  {{ text === null || text === undefined || text === '' ? '-' : text }}
                </span>
              </template>
            </template>
            <template #emptyText>
              <a-empty description="暂无数据" />
            </template>
          </a-table>
        </a-card>
      </a-tab-pane>

      <!-- ── 被动收集 ── -->
      <a-tab-pane key="passive" tab="被动收集">
        <a-card size="small">
          <template #title>
            <a-tag color="blue">P</a-tag><span style="font-size: 13px">被动信息收集（infogather）</span>
          </template>
          <div style="margin-bottom: 12px; font-size: 12px; color: var(--ant-color-text-tertiary)">
            工具在对话中直接调用：<b>icp-scan</b>（ICP备案反查）、<b>dns-enum</b>（批量DNS解析）、
            <b>supply-chain-collect</b>（FOFA供应链反查）、<b>xiaolanben-search</b>（小蓝本股权穿透）、
            <b>sg-code-search</b>（代码泄露搜索）、<b>quake-tool</b>（Quake数据质检）。
            工具输出后把结果 JSON 粘贴到下方，解析预览后一键导入资产库（source=infogather-*）。
          </div>

          <!-- AI 一键收集 -->
          <a-card size="small" style="margin-bottom: 16px; background: var(--ant-color-fill-quaternary)">
            <template #title>
              <a-tag color="purple">AI</a-tag>
              <span style="font-size: 13px">AI 一键收集（对话引擎，角色已锁定：信息收集，只收集不利用）</span>
            </template>
            <a-form layout="inline" style="row-gap: 8px">
              <a-form-item label="目标（公司名 / 域名）" style="min-width: 320px">
                <a-input v-model:value="aiTarget" placeholder="百胜中国 / kfc.com.cn" style="width: 240px" />
              </a-form-item>
              <a-form-item label="收集模式">
                <a-select v-model:value="aiMode" :options="aiModeOptions" style="width: 320px" />
              </a-form-item>
              <a-form-item>
                <a-button type="primary" @click="aiCollect">开始收集</a-button>
              </a-form-item>
            </a-form>
            <div style="margin-top: 8px; font-size: 12px; color: var(--ant-color-text-tertiary)">
              点击后自动：锁定「信息收集」角色 → 新建会话 → 切到对话页填入任务书并发送。AI 会按模式
              依次调用收集工具，边收边写入资产库；完成后回「资产库」查看结果（按来源筛 icp/dns/fofa/infogather）。
            </div>
          </a-card>

          <!-- JSON 导入 -->
          <a-form-item label="收集结果 JSON（数组或逐行 JSON；字段支持 host/ip/port/domain/protocol/title/server/country/province/city/source）">
            <a-textarea
              v-model:value="jsonInput"
              :rows="8"
              placeholder='[{"host":"1.2.3.4:8080","ip":"1.2.3.4","port":8080,"domain":"example.com","protocol":"http","title":"后台管理","server":"nginx","source":"icp"}]'
            />
          </a-form-item>
          <div>
            <a-button @click="previewJson">解析预览</a-button>
            <a-button type="primary" style="margin-left: 8px" :loading="jsonImporting" @click="importJson">
              导入资产库
            </a-button>
          </div>
          <template v-if="jsonRows.length">
            <a-table
              :columns="jsonColumns"
              :data-source="jsonRows.slice(0, 50)"
              :row-key="(_: any, index: number) => index"
              :pagination="false"
              size="small"
              style="margin-top: 12px"
            >
              <template #bodyCell="{ text }">
                <span>{{ text === null || text === undefined || text === '' ? '-' : text }}</span>
              </template>
            </a-table>
            <div v-if="jsonRows.length > 50" style="margin-top: 8px; font-size: 12px; color: var(--ant-color-text-tertiary)">
              … 共 {{ jsonRows.length }} 条（仅预览前 50 条）
            </div>
          </template>
        </a-card>
      </a-tab-pane>
    </a-tabs>

    <!-- 单元格详情弹窗 -->
    <a-modal
      v-model:open="cellModal.open"
      :title="cellModal.field || '字段'"
      width="640px"
      :footer="null"
    >
      <div style="font-size: 12px; color: var(--ant-color-text-tertiary); margin-bottom: 8px">
        共 {{ [...cellModal.text].length }} 个字符
      </div>
      <pre
        style="
          background: var(--ant-color-bg-layout);
          padding: 12px;
          border-radius: 4px;
          max-height: 400px;
          overflow: auto;
          font-size: 12px;
          white-space: pre-wrap;
          word-break: break-all;
        "
      >{{ cellModal.text }}</pre>
      <div style="margin-top: 12px; text-align: right">
        <a-button @click="copyCellText">复制</a-button>
        <a-button type="primary" style="margin-left: 8px" @click="cellModal.open = false">关闭</a-button>
      </div>
    </a-modal>
  </div>
</template>
