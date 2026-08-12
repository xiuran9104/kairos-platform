<script lang="ts" setup>
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { decideHitlApproval, getHitlApprovals, getHitlSettings, updateHitlSettings } from '#/api/pent';

const loading = ref(false);
const saving = ref(false);
const settings = ref<any>({ hitl_enabled: false, hitl_mode: 'approval', hitl_reviewer: 'human', hitl_tool_whitelist: [], approval_ttl_seconds: 1800 });
const approvals = ref<any[]>([]);
const whitelistText = ref('');

async function refresh() {
  loading.value = true;
  try {
    const [policy, pending] = await Promise.all([getHitlSettings(), getHitlApprovals({ status: 'pending' })]);
    settings.value = policy;
    whitelistText.value = (policy.hitl_tool_whitelist || []).join('\n');
    approvals.value = pending || [];
  } catch (error: any) { message.error(error?.message || '加载 AI 围栏失败'); }
  finally { loading.value = false; }
}

async function save() {
  saving.value = true;
  try {
    settings.value.hitl_tool_whitelist = whitelistText.value.split(/[\n,]/).map((v) => v.trim()).filter(Boolean);
    await updateHitlSettings(settings.value);
    message.success('AI 围栏设置已保存');
    await refresh();
  } catch (error: any) { message.error(error?.message || '保存失败'); }
  finally { saving.value = false; }
}

async function decide(record: any, decision: 'approve' | 'reject') {
  try {
    await decideHitlApproval(record.id, { decision, comment: decision === 'approve' ? '人工批准' : '人工拒绝' });
    message.success(decision === 'approve' ? '已批准' : '已拒绝');
    await refresh();
  } catch (error: any) { message.error(error?.message || '审批失败'); }
}

onMounted(refresh);
</script>

<template>
  <div class="hitl-page">
    <a-page-header title="AI 围栏" sub-title="DesRed 风格的工具白名单、人工审批与审计 Agent">
      <template #extra><a-button :loading="loading" @click="refresh">刷新</a-button></template>
    </a-page-header>
    <a-spin :spinning="loading">
      <a-card title="围栏策略" class="mb-4">
        <template #extra><a-button type="primary" :loading="saving" @click="save">保存</a-button></template>
        <a-form layout="vertical">
          <a-row :gutter="20">
            <a-col :span="6"><a-form-item label="启用"><a-switch v-model:checked="settings.hitl_enabled" /></a-form-item></a-col>
            <a-col :span="6"><a-form-item label="模式"><a-select v-model:value="settings.hitl_mode" :options="[{label:'审批',value:'approval'},{label:'审阅并可修改参数',value:'review_edit'}]" /></a-form-item></a-col>
            <a-col :span="6"><a-form-item label="审批者"><a-select v-model:value="settings.hitl_reviewer" :options="[{label:'人工',value:'human'},{label:'审计 Agent',value:'audit_agent'}]" /></a-form-item></a-col>
            <a-col :span="6"><a-form-item label="审批超时（秒）"><a-input-number v-model:value="settings.approval_ttl_seconds" :min="60" :max="86400" class="w-full" /></a-form-item></a-col>
          </a-row>
          <a-form-item label="全局工具白名单（每行一个，大小写不敏感）"><a-textarea v-model:value="whitelistText" :rows="5" /></a-form-item>
          <template v-if="settings.hitl_reviewer === 'audit_agent'">
            <a-form-item label="审计模型 ID（留空则使用会话模型）"><a-input-number v-model:value="settings.hitl_audit_model_id" class="w-full" /></a-form-item>
            <a-form-item label="审计系统提示词"><a-textarea v-model:value="settings.hitl_audit_system_prompt" :rows="4" placeholder="留空使用安全默认提示词" /></a-form-item>
            <a-form-item label="审计用户提示词"><a-textarea v-model:value="settings.hitl_audit_user_prompt" :rows="4" placeholder="支持 {{tool_name}}、{{arguments}}、{{mode}}" /></a-form-item>
          </template>
        </a-form>
      </a-card>
      <a-card title="待人工审批">
        <a-table :data-source="approvals" row-key="id" :pagination="false" :columns="[{title:'工具',dataIndex:'tool_name'},{title:'模式',dataIndex:'hitl_mode'},{title:'参数',dataIndex:'original_arguments',ellipsis:true},{title:'过期时间',dataIndex:'expires_at'},{title:'操作',key:'action',width:160}]">
          <template #bodyCell="{ column, record }"><a-space v-if="column.key === 'action'"><a-button type="primary" size="small" @click="decide(record,'approve')">批准</a-button><a-button danger size="small" @click="decide(record,'reject')">拒绝</a-button></a-space></template>
        </a-table>
      </a-card>
    </a-spin>
  </div>
</template>

<style scoped>.hitl-page{min-height:100%;padding:16px;background:var(--ant-color-bg-layout)}</style>
