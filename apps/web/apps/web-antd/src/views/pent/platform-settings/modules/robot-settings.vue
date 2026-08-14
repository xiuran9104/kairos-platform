<script lang="ts" setup>
import type { RobotChannelConfig } from '#/api/pent';
import { onMounted, ref } from 'vue';
import { message } from 'ant-design-vue';
import { getRobotSettings, updateRobotSettings } from '#/api/pent';
const channels=ref<RobotChannelConfig[]>([]);const loading=ref(false);const saving=ref(false);
const meta:Record<string,[string,string]>= {wechat:['微信 / iLink','扫码绑定个人微信'],wecom:['企业微信','企业内部应用回调机器人'],dingtalk:['钉钉','Stream 长连接机器人'],lark:['飞书 (Lark)','企业应用单聊与群聊'],telegram:['Telegram','Bot API 长轮询'],slack:['Slack','Socket Mode 连接'],discord:['Discord','Gateway WebSocket'],qq:['QQ 机器人','QQ 开放平台 WebSocket']};
async function load(){loading.value=true;try{channels.value=(await getRobotSettings()).map(v=>({...v,secret:''}))}catch(e:any){message.error(e?.message||'加载失败')}finally{loading.value=false}}
async function save(){saving.value=true;try{channels.value=(await updateRobotSettings(channels.value)).map(v=>({...v,secret:''}));message.success('机器人设置已保存')}catch(e:any){message.error(e?.message||'保存失败')}finally{saving.value=false}}
onMounted(load);
</script>
<template><a-spin :spinning="loading"><div class="heading"><div><h2>机器人设置</h2><p>配置企业微信、钉钉、飞书等机器人，在移动端直接与平台对话。</p></div><a-button type="primary" :loading="saving" @click="save">保存更改</a-button></div><div class="grid"><section v-for="item in channels" :key="item.id" class="card"><div class="card-head"><div><h3>{{ meta[item.id]?.[0] }}</h3><p>{{ meta[item.id]?.[1] }}</p></div><a-switch v-model:checked="item.enabled" /></div><a-form layout="vertical"><a-form-item label="App ID / Bot ID"><a-input v-model:value="item.app_id" /></a-form-item><a-form-item label="Secret / Token"><a-input-password v-model:value="item.secret" autocomplete="new-password" :placeholder="item.configured?'已安全保存；留空不修改':'输入凭据'" /></a-form-item></a-form><a-tag :color="item.configured?'green':'default'">{{ item.configured?'已配置':'未配置' }}</a-tag></section></div></a-spin></template>
<style scoped>.heading,.card-head{display:flex;justify-content:space-between;align-items:flex-start;gap:20px}.heading{margin-bottom:24px}.heading h2{margin:0;font-size:24px}.heading p,.card-head p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}.card{padding:18px;border:1px solid var(--ant-color-border-secondary);border-radius:7px}.card-head{margin-bottom:14px}.card-head h3{margin:0;font-size:16px}@media(max-width:800px){.grid{grid-template-columns:1fr}}</style>
