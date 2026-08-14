<script lang="ts" setup>
import { computed, ref, watch } from 'vue';
import { message } from 'ant-design-vue';

import { getPentPlatformSection, updatePentPlatformSection } from '#/api/pent';

type Field = { extra?: string; key: string; label: string; max?: number; min?: number; options?: Array<{ label: string; value: string }>; suffix?: string; type: 'boolean' | 'number' | 'select' | 'text' | 'textarea' };
const props = defineProps<{ description: string; sectionKey: string; title: string }>();
const loading = ref(false);
const saving = ref(false);
const values = ref<Record<string, any>>({});

const schemas: Record<string, Field[]> = {
  assets: [
    { key: 'auto_discovery', label: '自动发现资产', type: 'boolean', extra: '按计划执行资产发现并同步到资产库。' },
    { key: 'discovery_interval_minutes', label: '发现周期', type: 'number', min: 5, max: 10080, suffix: '分钟' },
    { key: 'alive_check_enabled', label: '资产存活检测', type: 'boolean' },
    { key: 'asset_retention_days', label: '离线资产保留时间', type: 'number', min: 1, max: 3650, suffix: '天' },
    { key: 'duplicate_strategy', label: '重复资产处理', type: 'select', options: [{ label: '自动合并', value: 'merge' }, { label: '保留最新', value: 'latest' }, { label: '允许重复', value: 'keep' }] },
    { key: 'risk_sync_enabled', label: '同步风险与漏洞', type: 'boolean' },
  ],
  knowledge: [
    { key: 'enabled', label: '启用知识检索功能', type: 'boolean' },
    { key: 'embedding_model_config_id', label: '嵌入模型配置 ID', type: 'number', min: 0, extra: '0 表示使用 AI 大模型模块中的默认嵌入模型。' },
    { key: 'retrieval_top_k', label: 'Top-K 结果数量', type: 'number', min: 1, max: 20 }, { key: 'similarity_threshold', label: '相似度阈值', type: 'number', min: 0, max: 1 },
    { key: 'sub_index_filter', label: '子索引过滤', type: 'text' }, { key: 'multi_query_max_queries', label: 'MultiQuery 改写上限', type: 'number', min: 1, max: 8 },
    { key: 'rerank_provider', label: '精排提供商', type: 'select', options: [{label:'自动推断',value:'auto'},{label:'DashScope',value:'dashscope'},{label:'Cohere 兼容',value:'cohere'}] }, { key: 'rerank_model', label: '精排模型', type: 'text' },
    { key: 'prefetch_top_k', label: '向量预取候选数', type: 'number', min: 0, max: 200 }, { key: 'max_context_chars', label: '返回内容最大字符数', type: 'number', min: 0, max: 1000000 },
    { key: 'max_context_tokens', label: '返回内容最大 Token 数', type: 'number', min: 0, max: 1000000 }, { key: 'chunk_strategy', label: '分块策略', type: 'select', options: [{label:'Markdown 标题 + 递归',value:'markdown_then_recursive'},{label:'仅递归切分',value:'recursive'}] },
    { key: 'request_timeout_seconds', label: '嵌入 HTTP 超时', type: 'number', min: 0, max: 600, suffix: '秒' }, { key: 'batch_size', label: '嵌入批大小', type: 'number', min: 1, max: 256 },
    { key: 'prefer_source_file', label: '优先读取磁盘源文件', type: 'boolean' }, { key: 'sub_indexes', label: 'Hermes 子索引', type: 'text' },
    { key: 'chunk_size', label: '分块大小', type: 'number', min: 128, max: 4096, suffix: 'Token' }, { key: 'chunk_overlap', label: '分块重叠', type: 'number', min: 0, max: 512, suffix: 'Token' },
    { key: 'max_chunks_per_item', label: '单知识项最大块数', type: 'number', min: 0, max: 1000 }, { key: 'max_rpm', label: '每分钟最大请求数', type: 'number', min: 0, max: 1000 },
    { key: 'rate_limit_delay_ms', label: '请求间隔延迟', type: 'number', min: 0, max: 10000, suffix: '毫秒' }, { key: 'max_retries', label: '最大重试次数', type: 'number', min: 0, max: 10 },
    { key: 'retry_delay_ms', label: '重试间隔', type: 'number', min: 0, max: 10000, suffix: '毫秒' },
  ],
  c2: [
    { key: 'enabled', label: '启用 C2 调度', type: 'boolean' }, { key: 'heartbeat_seconds', label: '节点心跳周期', type: 'number', min: 5, max: 300, suffix: '秒' },
    { key: 'offline_timeout_seconds', label: '节点离线判定', type: 'number', min: 30, max: 3600, suffix: '秒' }, { key: 'max_concurrent_jobs', label: '最大并发任务', type: 'number', min: 1, max: 200 },
    { key: 'tls_required', label: '强制 TLS 通信', type: 'boolean' }, { key: 'allow_unregistered_nodes', label: '允许未注册节点接入', type: 'boolean' },
    { key: 'command_timeout_seconds', label: '命令执行超时', type: 'number', min: 10, max: 86400, suffix: '秒' },
  ],
  robots: [
    { key: 'enabled', label: '启用机器人协作', type: 'boolean' }, { key: 'max_active_agents', label: '最大活跃 Agent', type: 'number', min: 1, max: 100 },
    { key: 'default_role', label: '默认角色', type: 'select', options: [{ label: '渗透测试员', value: 'penetration_tester' }, { label: '安全分析师', value: 'security_analyst' }, { label: '任务协调员', value: 'coordinator' }] },
    { key: 'auto_assign_tasks', label: '自动分配任务', type: 'boolean' }, { key: 'idle_timeout_minutes', label: '空闲回收时间', type: 'number', min: 1, max: 1440, suffix: '分钟' },
    { key: 'memory_enabled', label: '启用长期记忆', type: 'boolean' }, { key: 'collaboration_enabled', label: '允许 Agent 间协作', type: 'boolean' },
  ],
  terminal: [
    { key: 'session_timeout_minutes', label: '会话超时时间', type: 'number', min: 5, max: 1440, suffix: '分钟' }, { key: 'max_sessions_per_user', label: '单用户最大会话', type: 'number', min: 1, max: 50 },
    { key: 'command_audit_enabled', label: '记录终端命令', type: 'boolean' }, { key: 'dangerous_command_confirmation', label: '危险命令二次确认', type: 'boolean' },
    { key: 'file_transfer_enabled', label: '允许文件传输', type: 'boolean' }, { key: 'max_upload_mb', label: '单文件上传限制', type: 'number', min: 1, max: 10240, suffix: 'MB' },
    { key: 'default_shell', label: '默认 Shell', type: 'select', options: [{ label: 'Bash', value: 'bash' }, { label: 'PowerShell', value: 'powershell' }, { label: 'Zsh', value: 'zsh' }] },
  ],
};
const fields = computed(() => schemas[props.sectionKey] || []);

async function load() { loading.value = true; try { values.value = await getPentPlatformSection(props.sectionKey); } catch (error: any) { message.error(error?.message || '加载设置失败'); } finally { loading.value = false; } }
async function save() { saving.value = true; try { values.value = await updatePentPlatformSection(props.sectionKey, values.value); message.success(`${props.title}已保存`); } catch (error: any) { message.error(error?.message || '保存失败'); } finally { saving.value = false; } }
watch(() => props.sectionKey, load, { immediate: true });
</script>

<template>
  <a-spin :spinning="loading">
    <div class="section-heading"><div><h2>{{ title }}</h2><p>{{ description }}</p></div><a-button type="primary" :loading="saving" @click="save">保存更改</a-button></div>
    <section class="settings-block"><a-form layout="vertical"><a-row :gutter="24">
      <a-col v-for="field in fields" :key="field.key" :xs="24" :md="field.type === 'textarea' ? 24 : 12">
        <a-form-item :label="field.label" :extra="field.extra">
          <div v-if="field.type === 'boolean'" class="switch-control"><a-switch v-model:checked="values[field.key]" /><span>{{ values[field.key] ? '已启用' : '已停用' }}</span></div>
          <a-input-number v-else-if="field.type === 'number'" v-model:value="values[field.key]" :min="field.min" :max="field.max" :addon-after="field.suffix" />
          <a-select v-else-if="field.type === 'select'" v-model:value="values[field.key]" :options="field.options" />
          <a-textarea v-else-if="field.type === 'textarea'" v-model:value="values[field.key]" :rows="6" />
          <a-input v-else v-model:value="values[field.key]" />
        </a-form-item>
      </a-col>
    </a-row></a-form></section>
  </a-spin>
</template>

<style scoped>
.section-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:20px;margin-bottom:28px}.section-heading h2{margin:0;font-size:24px}.section-heading p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.settings-block{padding-top:22px;border-top:1px solid var(--ant-color-border-secondary)}.settings-block :deep(.ant-input-number){width:100%}.switch-control{display:flex;align-items:center;gap:10px;min-height:32px;color:var(--ant-color-text-secondary)}
</style>
