<script lang="ts" setup>
import { nextTick, onBeforeUnmount, ref } from 'vue';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { message } from 'ant-design-vue';
import { closeTerminalSession, createTerminalSession, readTerminalOutput, writeTerminalInput } from '#/api/pent';

type Tab={fit:FitAddon;id:string;sequence:number;terminal:Terminal;title:string};
const tabs=ref<Tab[]>([]);const activeId=ref('');const hosts=new Map<string,HTMLElement>();let polling=true;
function hostRef(id:string,element:any){if(element)hosts.set(id,element as HTMLElement);else hosts.delete(id)}
async function addTab(){try{const {id}=await createTerminalSession();const terminal=new Terminal({cursorBlink:true,fontFamily:'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',fontSize:13,theme:{background:'#080b10',foreground:'#d7dde8',cursor:'#5aa9ff',selectionBackground:'#264f78'}});const fit=new FitAddon();terminal.loadAddon(fit);const tab={fit,id,sequence:0,terminal,title:`终端 ${tabs.value.length+1}`};tabs.value.push(tab);activeId.value=id;await nextTick();terminal.open(hosts.get(id)!);fit.fit();terminal.focus();terminal.onData(data=>writeTerminalInput(id,data).catch(()=>{}));void poll(tab)}catch(e:any){message.error(e?.message||'新建终端失败')}}
async function poll(tab:Tab){while(polling&&tabs.value.some(item=>item.id===tab.id)){try{const result=await readTerminalOutput(tab.id,tab.sequence);for(const chunk of result.chunks)tab.terminal.write(chunk.data);tab.sequence=result.sequence}catch{break}await new Promise(resolve=>setTimeout(resolve,180))}}
async function selectTab(tab:Tab){activeId.value=tab.id;await nextTick();tab.fit.fit();tab.terminal.focus()}
async function closeTab(tab:Tab){await closeTerminalSession(tab.id).catch(()=>{});tab.terminal.dispose();const index=tabs.value.findIndex(item=>item.id===tab.id);tabs.value.splice(index,1);if(activeId.value===tab.id){activeId.value=tabs.value[Math.max(0,index-1)]?.id||'';if(activeId.value)await nextTick().then(()=>tabs.value.find(item=>item.id===activeId.value)?.fit.fit())}}
onBeforeUnmount(()=>{polling=false;for(const tab of tabs.value){void closeTerminalSession(tab.id);tab.terminal.dispose()}});
void addTab();
</script>

<template><div class="heading"><h2>终端</h2><p>服务器交互式终端，用于授权范围内的运维与调试。每个标签拥有独立、持续的 Bash 会话。</p></div><div class="terminal-wrapper"><div class="terminal-tabs"><button v-for="tab in tabs" :key="tab.id" class="terminal-tab" :class="{active:activeId===tab.id}" type="button" @click="selectTab(tab)"><span>{{tab.title}}</span><span class="close" @click.stop="closeTab(tab)">×</span></button><button class="new-tab" type="button" title="新终端" @click="addTab">＋</button></div><div class="terminal-panes"><div v-if="!tabs.length" class="empty-terminal"><a-button type="primary" @click="addTab">新建终端</a-button></div><div v-for="tab in tabs" v-show="activeId===tab.id" :key="tab.id" :ref="element=>hostRef(tab.id,element)" class="terminal-host"></div></div></div></template>

<style scoped>.heading{margin-bottom:22px}.heading h2{margin:0;font-size:24px}.heading p{margin:6px 0 0;color:var(--ant-color-text-secondary)}.terminal-wrapper{overflow:hidden;background:#080b10;border:1px solid #29303b;border-radius:7px}.terminal-tabs{display:flex;align-items:stretch;height:42px;background:#10151d;border-bottom:1px solid #29303b}.terminal-tab,.new-tab{color:#8e99a8;background:transparent;border:0;border-right:1px solid #29303b;cursor:pointer}.terminal-tab{display:flex;align-items:center;gap:20px;padding:0 14px}.terminal-tab.active{color:#e4e9f1;background:#080b10}.close{font-size:16px}.close:hover{color:#ff5d68}.new-tab{width:44px;font-size:18px}.terminal-panes{position:relative;height:540px;padding:10px}.terminal-host{width:100%;height:100%}.terminal-host :deep(.xterm){height:100%;padding:4px}.empty-terminal{display:grid;height:100%;place-items:center}</style>
