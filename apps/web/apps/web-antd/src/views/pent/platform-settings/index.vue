<script lang="ts" setup>
import { computed, ref } from 'vue';

import BasicSettings from './modules/basic-settings.vue';
import C2Settings from './modules/c2-settings.vue';
import HitlSettings from './modules/hitl-settings.vue';
import InfoCollectSettings from './modules/info-collect-settings.vue';
import KnowledgeSettings from './modules/knowledge-settings.vue';
import RobotSettings from './modules/robot-settings.vue';
import SectionSettings from './modules/section-settings.vue';
import TerminalSettings from './modules/terminal-settings.vue';

type Section = {
  description: string;
  key: string;
  label: string;
};

const activeKey = ref('basic');
const sections: Section[] = [
  { key: 'basic', label: '基本设置', description: '平台默认模型、Agent 与运行策略' },
  { key: 'hitl', label: 'AI 围栏', description: '高风险工具审批、白名单和审核策略' },
  { key: 'assets', label: '资产管理', description: '配置资产发现、存活检测、去重和风险同步策略' },
  { key: 'knowledge', label: '知识库', description: '配置知识索引、分块、召回和引用策略' },
  { key: 'c2', label: 'C2', description: '配置节点连接、心跳、并发和通信安全' },
  { key: 'robots', label: '机器人设置', description: '配置 Agent 数量、角色、记忆和协作行为' },
  { key: 'terminal', label: '终端', description: '配置终端会话、文件传输和命令安全控制' },
];
const activeSection = computed(() => sections.find((item) => item.key === activeKey.value)!);
</script>

<template>
  <div class="settings-page">
    <header class="page-header"><h1>系统设置</h1><p>管理平台级配置与安全智能运行策略</p></header>
    <div class="settings-shell">
      <aside class="settings-sidebar" aria-label="系统设置分类">
        <button v-for="section in sections" :key="section.key" class="nav-item" :class="{ active: activeKey === section.key }" type="button" @click="activeKey = section.key">{{ section.label }}</button>
      </aside>
      <main class="settings-content">
        <BasicSettings v-if="activeKey === 'basic'" />
        <HitlSettings v-else-if="activeKey === 'hitl'" />
        <InfoCollectSettings v-else-if="activeKey === 'assets'" />
        <KnowledgeSettings v-else-if="activeKey === 'knowledge'" />
        <C2Settings v-else-if="activeKey === 'c2'" />
        <RobotSettings v-else-if="activeKey === 'robots'" />
        <TerminalSettings v-else-if="activeKey === 'terminal'" />
        <SectionSettings v-else :section-key="activeSection.key" :title="activeSection.label" :description="activeSection.description" />
      </main>
    </div>
  </div>
</template>

<style scoped>
.settings-page{min-height:100%;padding:24px;background:var(--ant-color-bg-layout)}.page-header{margin:0 0 20px}.page-header h1{margin:0;font-size:24px;font-weight:650;line-height:1.35}.page-header p{margin:5px 0 0;color:var(--ant-color-text-secondary)}.settings-shell{display:flex;min-height:calc(100vh - 170px);overflow:hidden;background:var(--ant-color-bg-container);border:1px solid var(--ant-color-border-secondary);border-radius:8px}.settings-sidebar{flex:0 0 224px;padding:18px 0;background:var(--ant-color-fill-quaternary);border-right:1px solid var(--ant-color-border-secondary)}.nav-item{display:block;width:100%;height:46px;padding:0 28px;color:var(--ant-color-text-secondary);font-size:14px;text-align:left;background:transparent;border:0;border-left:3px solid transparent;cursor:pointer;transition:.16s ease}.nav-item:hover{color:var(--ant-color-text);background:var(--ant-color-fill-tertiary)}.nav-item.active{color:var(--ant-color-primary);font-weight:600;background:color-mix(in srgb,var(--ant-color-primary) 10%,transparent);border-left-color:var(--ant-color-primary)}.settings-content{flex:1;min-width:0;padding:28px 34px 40px;overflow:auto}@media(max-width:800px){.settings-page{padding:14px}.settings-shell{display:block}.settings-sidebar{display:flex;overflow:auto;width:100%;padding:0;border-right:0;border-bottom:1px solid var(--ant-color-border-secondary)}.nav-item{flex:0 0 auto;width:auto;padding:0 18px;border-bottom:2px solid transparent;border-left:0}.nav-item.active{border-bottom-color:var(--ant-color-primary);border-left:0}.settings-content{padding:22px 18px 32px}}
</style>
