<script lang="ts" setup>
defineOptions({ name: 'SessionSidebar' });

defineProps<{
  sessions: any[];
  currentSessionId?: string;
  collapsed: boolean;
  loading: boolean;
}>();

const emit = defineEmits<{
  (e: 'select', session: any): void;
  (e: 'create'): void;
  (e: 'delete', session: any): void;
  (e: 'toggle-collapse'): void;
}>();
</script>

<template>
  <!-- Sidebar panel -->
  <div
    :style="{
      width: collapsed ? '0px' : '260px',
      transition: 'width 0.2s',
      overflow: 'hidden',
      background: 'var(--ant-color-bg-container)',
      borderRight: '1px solid var(--ant-color-border-secondary)',
      display: 'flex',
      flexDirection: 'column',
      flexShrink: 0,
      minHeight: 0,
    }"
  >
    <div
      style="padding:16px;border-bottom:1px solid var(--ant-color-border-secondary);display:flex;justify-content:space-between;align-items:center"
    >
      <span style="font-weight:600;font-size:15px">Sessions</span>
      <a-space>
        <a-button size="small" type="primary" aria-label="新建会话" @click="emit('create')">+</a-button>
        <a-button size="small" aria-label="收起侧栏" @click="emit('toggle-collapse')">✕</a-button>
      </a-space>
    </div>
    <div style="flex:1;min-height:0;overflow-y:auto;overscroll-behavior:contain;padding:8px">
      <div v-if="loading" style="text-align:center;padding:20px;color:#999">加载中…</div>
      <div
        v-for="s in sessions"
        :key="s.id"
        @click="emit('select', s)"
        :style="{
          padding: '10px 12px',
          cursor: 'pointer',
          borderRadius: '6px',
          marginBottom: '4px',
          background: currentSessionId === s.id ? 'var(--ant-color-primary-bg)' : 'transparent',
          border: currentSessionId === s.id ? '1px solid #91caff' : '1px solid transparent',
        }"
      >
        <div style="display:flex;justify-content:space-between;align-items:center">
          <div style="flex:1;min-width:0">
            <div
              style="font-size:13px;font-weight:500;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"
            >
              {{ s.objective || '未命名会话' }}
            </div>
            <div style="font-size:11px;color:#999;margin-top:2px">
              {{ s.created_at?.slice(0, 16) }} · {{ s.status === 'draft' ? '草稿' : s.status }}
              <a-tag
                v-if="s.run_status && s.run_status !== 'ready'"
                :color="s.run_status === 'running' ? 'green' : s.run_status === 'failed' ? 'red' : 'orange'"
                size="small"
                style="margin-left:4px;font-size:10px"
              >{{ ({ queued:'排队中', running:'运行中', cancelling:'取消中', cancelled:'已取消', completed:'已完成', failed:'失败' } as any)[s.run_status] || s.run_status }}</a-tag>
            </div>
          </div>
          <a-button
            type="link"
            size="small"
            danger
            @click.stop="emit('delete', s)"
            style="flex-shrink:0"
          >删除</a-button>
        </div>
      </div>
      <div
        v-if="!loading && sessions.length === 0"
        style="text-align:center;padding:20px;color:#999"
      >
        暂无会话
      </div>
    </div>
  </div>

  <!-- Collapsed toggle button (floats at left edge) -->
  <div v-if="collapsed" style="position:absolute;left:0;top:16px;z-index:10">
    <a-button size="small" @click="emit('toggle-collapse')">☰</a-button>
  </div>
</template>
