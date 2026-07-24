<script lang="ts" setup>
import { useRouter } from 'vue-router';

defineProps<{ assets: any[]; loading: boolean }>();
const router = useRouter();
</script>

<template>
  <a-card title="项目风险概览" :bordered="false" size="small" :loading="loading">
    <a-empty v-if="!assets.length" description="暂无项目数据" :image="false" />
    <a-list v-else :data-source="assets.slice(0, 6)" size="small">
      <template #renderItem="{ item, index }">
        <a-list-item>
          <template #actions>
            <a-button type="link" size="small" @click="router.push(`/pent/assets/${item.id}`)">
              查看
            </a-button>
          </template>
          <a-list-item-meta>
            <template #title>
              <a-space>
                <a-tag :color="index === 0 ? 'red' : index < 3 ? 'orange' : 'default'">
                  #{{ index + 1 }}
                </a-tag>
                <span>{{ item.name || item.id?.slice(0, 16) }}</span>
              </a-space>
            </template>
            <template #description>
              <span class="text-xs text-gray-400">创建于 {{ item.created_at }}</span>
            </template>
          </a-list-item-meta>
        </a-list-item>
      </template>
    </a-list>
  </a-card>
</template>
