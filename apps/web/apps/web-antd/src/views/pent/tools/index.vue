<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { getPentTools, getPentSkills, getPentSkill, createPentSkill, updatePentSkillStatus, type PentSkill } from '#/api/pent';

// ── Tool registry ────────────────────────────────────────────────────────
const tools = ref<any[]>([]);
const toolLoading = ref(false);
const searchText = ref('');
const nsFilter = ref('all');

async function fetchTools() {
  toolLoading.value = true;
  try { tools.value = (await getPentTools()) as any[]; } catch { /* */ } finally { toolLoading.value = false; }
}

const nsList = computed(() => {
  const ns = [...new Set(tools.value.map((t: any) => t.namespace).filter(Boolean))];
  return [{ label: '全部', value: 'all' }, ...ns.map((n: any) => ({ label: n, value: n }))];
});

const filtered = computed(() => {
  let list = tools.value;
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter((t: any) => (t.name || '').toLowerCase().includes(q) || (t.description || '').toLowerCase().includes(q));
  }
  if (nsFilter.value !== 'all') list = list.filter((t: any) => t.namespace === nsFilter.value);
  return list;
});

// ── Skill management ─────────────────────────────────────────────────────
const skills = ref<PentSkill[]>([]);
const skillLoading = ref(false);
const skillFilter = ref('all');
const skillSearch = ref('');
const showSkillCreate = ref(false);
const skillForm = ref({ name: '', title: '', content: '', description: '', category: 'general', risk_level: 'low' });
const skillCreating = ref(false);
const skillDetail = ref<PentSkill | null>(null);
const showSkillDetail = ref(false);

async function fetchSkills() {
  skillLoading.value = true;
  try {
    const params: any = {};
    if (skillFilter.value !== 'all') params.status = skillFilter.value;
    if (skillSearch.value) params.query = skillSearch.value;
    skills.value = (await getPentSkills(params)) as PentSkill[];
  } catch { /* */ } finally { skillLoading.value = false; }
}

const filteredSkills = computed(() => skills.value);

async function doCreateSkill() {
  if (!skillForm.value.name || !skillForm.value.title || !skillForm.value.content) return;
  skillCreating.value = true;
  try {
    await createPentSkill(skillForm.value);
    showSkillCreate.value = false;
    skillForm.value = { name: '', title: '', content: '', description: '', category: 'general', risk_level: 'low' };
    await fetchSkills();
  } catch { /* */ } finally { skillCreating.value = false; }
}

async function doTransition(id: string, to: string) {
  try {
    await updatePentSkillStatus(id, to);
    await fetchSkills();
  } catch { /* */ }
}

async function openDetail(skill: PentSkill) {
  try {
    skillDetail.value = (await getPentSkill(skill.id)) as PentSkill;
  } catch {
    skillDetail.value = skill;
  }
  showSkillDetail.value = true;
}

const statusColor: Record<string, string> = { draft: 'default', reviewed: 'blue', published: 'green', disabled: 'red' };
const statusLabel: Record<string, string> = { draft: '草稿', reviewed: '已审核', published: '已发布', disabled: '已停用' };

const canTransition = (current: string, target: string): boolean => {
  const allowed: Record<string, string[]> = {
    draft: ['reviewed'],
    reviewed: ['published', 'draft'],
    published: ['disabled'],
    disabled: ['draft'],
  };
  return (allowed[current] || []).includes(target);
};

onMounted(() => { fetchTools(); fetchSkills(); });
</script>

<template>
  <div style="padding: 24px; min-height: 100vh">
    <a-tabs>
      <!-- ── Tab 1: Tool registry ──────────────────────────────────────── -->
      <a-tab-pane key="tools" tab="工具注册表">
        <a-page-header sub-title="安全工具目录 · 63+ 可调用工具">
          <template #extra>
            <a-input-search v-model:value="searchText" placeholder="搜索..." style="width:200px" allow-clear />
            <a-select v-model:value="nsFilter" style="width:120px;margin-left:8px" :options="nsList" />
            <a-button @click="fetchTools" :loading="toolLoading" style="margin-left:8px">刷新</a-button>
          </template>
        </a-page-header>

        <a-spin :spinning="toolLoading">
          <a-row :gutter="[16,16]">
            <a-col :xs="24" :sm="12" :md="8" :lg="6" v-for="t in filtered" :key="t.tool_id + '-' + t.version">
              <a-card hoverable size="small">
                <template #title><span style="font-size:13px">{{ t.name }}</span></template>
                <template #extra>
                  <a-tag :color="t.risk_level==='high'?'red':t.risk_level==='medium'?'orange':'green'" size="small">{{ t.risk_level || 'low' }}</a-tag>
                </template>
                <div style="font-size:12px;color:var(--ant-color-text-tertiary);margin-bottom:8px;min-height:32px">{{ t.description }}</div>
                <div style="display:flex;flex-wrap:wrap;gap:4px">
                  <a-tag size="small" color="blue">{{ t.namespace }}</a-tag>
                  <a-tag v-for="cap in (t.capabilities||[]).slice(0,3)" :key="cap" size="small">{{ cap }}</a-tag>
                </div>
              </a-card>
            </a-col>
          </a-row>
          <a-empty v-if="!filtered.length && !toolLoading" description="暂无工具" />
        </a-spin>
      </a-tab-pane>

      <!-- ── Tab 2: Skill management ─────────────────────────────────── -->
      <a-tab-pane key="skills" tab="Skill 管理">
        <a-page-header sub-title="Agent 操作规程 · draft → reviewed → published → disabled">
          <template #extra>
            <a-input-search v-model:value="skillSearch" placeholder="搜索 Skill..." style="width:200px" allow-clear @search="fetchSkills" />
            <a-select v-model:value="skillFilter" style="width:120px;margin-left:8px"
              :options="[{label:'全部状态',value:'all'},{label:'草稿',value:'draft'},{label:'已审核',value:'reviewed'},{label:'已发布',value:'published'},{label:'已停用',value:'disabled'}]"
              @change="fetchSkills" />
            <a-button @click="fetchSkills" :loading="skillLoading" style="margin-left:8px">刷新</a-button>
            <a-button type="primary" style="margin-left:8px" @click="showSkillCreate = true">新建 Skill</a-button>
          </template>
        </a-page-header>

        <a-spin :spinning="skillLoading">
          <a-table :dataSource="filteredSkills" :pagination="false" rowKey="id" size="middle">
            <a-table-column title="名称" dataIndex="name" :width="140">
              <template #default="{ record }">
                <a @click="openDetail(record)">{{ record.name }}</a>
              </template>
            </a-table-column>
            <a-table-column title="标题" dataIndex="title" :width="180" />
            <a-table-column title="分类" dataIndex="category" :width="100">
              <template #default="{ record }">
                <a-tag size="small">{{ record.category }}</a-tag>
              </template>
            </a-table-column>
            <a-table-column title="风险" dataIndex="risk_level" :width="70">
              <template #default="{ record }">
                <a-tag :color="record.risk_level==='high'?'red':'green'" size="small">{{ record.risk_level }}</a-tag>
              </template>
            </a-table-column>
            <a-table-column title="状态" dataIndex="status" :width="90">
              <template #default="{ record }">
                <a-tag :color="statusColor[record.status]">{{ statusLabel[record.status] || record.status }}</a-tag>
              </template>
            </a-table-column>
            <a-table-column title="版本" dataIndex="version" :width="90" />
            <a-table-column title="修订" dataIndex="revision" :width="60" />
            <a-table-column title="更新时间" dataIndex="updated_at" :width="170">
              <template #default="{ record }">{{ record.updated_at?.slice(0, 16).replace('T', ' ') }}</template>
            </a-table-column>
            <a-table-column title="操作" :width="200">
              <template #default="{ record }">
                <a-button size="small" type="link" @click="openDetail(record)">详情</a-button>
                <template v-if="canTransition(record.status, 'reviewed')">
                  <a-button size="small" type="link" @click="doTransition(record.id, 'reviewed')">审核通过</a-button>
                </template>
                <template v-if="canTransition(record.status, 'published')">
                  <a-button size="small" type="link" style="color:green" @click="doTransition(record.id, 'published')">发布</a-button>
                </template>
                <template v-if="canTransition(record.status, 'disabled')">
                  <a-button size="small" type="link" danger @click="doTransition(record.id, 'disabled')">停用</a-button>
                </template>
                <template v-if="canTransition(record.status, 'draft')">
                  <a-button size="small" type="link" @click="doTransition(record.id, 'draft')">退回草稿</a-button>
                </template>
              </template>
            </a-table-column>
          </a-table>
          <a-empty v-if="!filteredSkills.length && !skillLoading" description="暂无 Skill" />
        </a-spin>

        <!-- Skill detail modal -->
        <a-modal v-model:open="showSkillDetail" :title="skillDetail?.title" width="720px" :footer="null">
          <template v-if="skillDetail">
            <a-descriptions bordered size="small" :column="2">
              <a-descriptions-item label="名称">{{ skillDetail.name }}</a-descriptions-item>
              <a-descriptions-item label="状态">
                <a-tag :color="statusColor[skillDetail.status]">{{ statusLabel[skillDetail.status] }}</a-tag>
              </a-descriptions-item>
              <a-descriptions-item label="分类">{{ skillDetail.category }}</a-descriptions-item>
              <a-descriptions-item label="风险等级">{{ skillDetail.risk_level }}</a-descriptions-item>
              <a-descriptions-item label="版本">{{ skillDetail.version }} (r{{ skillDetail.revision }})</a-descriptions-item>
              <a-descriptions-item label="更新时间">{{ skillDetail.updated_at?.slice(0, 16).replace('T', ' ') }}</a-descriptions-item>
            </a-descriptions>
            <div style="margin-top:12px"><strong>描述：</strong>{{ skillDetail.description || '—' }}</div>
            <div style="margin-top:12px">
              <strong>内容：</strong>
              <pre style="background:var(--ant-color-bg-layout);padding:12px;border-radius:4px;max-height:360px;overflow:auto;margin-top:4px;font-size:12px;white-space:pre-wrap">{{ skillDetail.content || '(点击详情查看)' }}</pre>
            </div>
          </template>
        </a-modal>

        <!-- Create skill modal -->
        <a-modal v-model:open="showSkillCreate" title="新建 Skill" width="600px" @ok="doCreateSkill" :confirmLoading="skillCreating">
          <a-form layout="vertical" :model="skillForm">
            <a-form-item label="名称" required><a-input v-model:value="skillForm.name" placeholder="小写字母+数字+中划线，如 web-recon-v2" /></a-form-item>
            <a-form-item label="标题" required><a-input v-model:value="skillForm.title" placeholder="如：Web 侦察规程" /></a-form-item>
            <a-form-item label="分类"><a-input v-model:value="skillForm.category" placeholder="recon / validation / reporting / general" /></a-form-item>
            <a-form-item label="风险等级">
              <a-select v-model:value="skillForm.risk_level">
                <a-select-option value="low">低</a-select-option>
                <a-select-option value="medium">中</a-select-option>
                <a-select-option value="high">高</a-select-option>
              </a-select>
            </a-form-item>
            <a-form-item label="描述"><a-textarea v-model:value="skillForm.description" placeholder="简要说明此 Skill 的用途" :rows="2" /></a-form-item>
            <a-form-item label="内容" required><a-textarea v-model:value="skillForm.content" placeholder="Markdown 格式的完整操作规程..." :rows="10" /></a-form-item>
          </a-form>
        </a-modal>
      </a-tab-pane>
    </a-tabs>
  </div>
</template>
