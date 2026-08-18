<script lang="ts" setup>
import type { VxeTableGridOptions } from '#/adapter/vxe-table';
import type { AiModelModelApi } from '#/api/ai/model/model';

import { onMounted } from 'vue';

import { DocAlert, Page, useVbenModal } from '@vben/common-ui';

import { message } from 'ant-design-vue';

import { ACTION_ICON, TableAction, useVbenVxeGrid } from '#/adapter/vxe-table';
import {
  deleteModel,
  discoverModels,
  getModelPage,
  getModelPlatformCapabilities,
  testModel,
} from '#/api/ai/model/model';
import { $t } from '#/locales';

import { useGridColumns, useGridFormSchema } from './data';
import Form from './modules/form.vue';
import { applyGatewayCapabilities } from './platforms';
import { applyDiscoveredModels } from './platforms';

const [FormModal, formModalApi] = useVbenModal({
  connectedComponent: Form,
  destroyOnClose: true,
});

/** 刷新表格 */
function handleRefresh() {
  gridApi.query();
}

/** 创建模型配置 */
async function loadCapabilities() {
  const capabilities = await getModelPlatformCapabilities();
  applyGatewayCapabilities(capabilities.platforms);
  for (const platform of new Set(
    capabilities.cachedModels.map((item) => item.platform),
  )) {
    applyDiscoveredModels(
      platform,
      capabilities.cachedModels
        .filter((item) => item.platform === platform)
        .map((item) => ({ id: item.model, type: item.type })),
    );
  }
}

async function handleCreate() {
  await loadCapabilities();
  formModalApi.setData(null).open();
}

/** 编辑模型配置 */
async function handleEdit(row: AiModelModelApi.Model) {
  await loadCapabilities();
  formModalApi.setData(row).open();
}

onMounted(loadCapabilities);

/** 删除模型配置 */
async function handleDelete(row: AiModelModelApi.Model) {
  const hideLoading = message.loading({
    content: $t('ui.actionMessage.deleting', [row.name]),
    duration: 0,
  });
  try {
    await deleteModel(row.id!);
    message.success($t('ui.actionMessage.deleteSuccess', [row.name]));
    handleRefresh();
  } finally {
    hideLoading();
  }
}

async function handleTest(row: AiModelModelApi.Model) {
  const hideLoading = message.loading({ content: `正在测试 ${row.name}`, duration: 0 });
  try {
    const result = await testModel(row.id);
    message.success(`连接成功，耗时 ${result.latencyMs} ms`);
  } finally {
    hideLoading();
  }
}

async function handleDiscover(row: AiModelModelApi.Model) {
  const hideLoading = message.loading({
    content: `正在同步 ${row.name} 的模型目录`,
    duration: 0,
  });
  try {
    const result = await discoverModels({ configId: row.id });
    await loadCapabilities();
    message.success(`已同步 ${result.models.length} 个可用模型`);
  } finally {
    hideLoading();
  }
}

const [Grid, gridApi] = useVbenVxeGrid({
  formOptions: {
    schema: useGridFormSchema(),
  },
  gridOptions: {
    columns: useGridColumns(),
    height: 'auto',
    keepSource: true,
    proxyConfig: {
      ajax: {
        query: async ({ page }, formValues) => {
          return await getModelPage({
            pageNo: page.currentPage,
            pageSize: page.pageSize,
            ...formValues,
          });
        },
      },
    },
    rowConfig: {
      keyField: 'id',
      isHover: true,
    },
    toolbarConfig: {
      refresh: true,
      search: true,
    },
  } as VxeTableGridOptions<AiModelModelApi.Model>,
});
</script>

<template>
  <Page auto-content-height>
    <template #doc>
      <DocAlert title="AI 手册" url="https://doc.iocoder.cn/ai/build/" />
    </template>
    <FormModal @success="handleRefresh" />
    <Grid table-title="模型配置列表">
      <template #toolbar-tools>
        <TableAction
          :actions="[
            {
              label: $t('ui.actionTitle.create', ['模型配置']),
              type: 'primary',
              icon: ACTION_ICON.ADD,
              auth: ['ai:model:create'],
              onClick: handleCreate,
            },
          ]"
        />
      </template>
      <template #actions="{ row }">
        <TableAction
          :actions="[
            {
              label: '测试',
              type: 'link',
              icon: 'lucide:circle-play',
              auth: ['ai:model:update'],
              onClick: handleTest.bind(null, row),
            },
            {
              label: '同步目录',
              type: 'link',
              icon: 'lucide:refresh-cw',
              auth: ['ai:model:update'],
              onClick: handleDiscover.bind(null, row),
            },
            {
              label: $t('common.edit'),
              type: 'link',
              icon: ACTION_ICON.EDIT,
              auth: ['ai:model:update'],
              onClick: handleEdit.bind(null, row),
            },
            {
              label: $t('common.delete'),
              type: 'link',
              danger: true,
              icon: ACTION_ICON.DELETE,
              auth: ['ai:model:delete'],
              popConfirm: {
                title: $t('ui.actionMessage.deleteConfirm', [row.name]),
                confirm: handleDelete.bind(null, row),
              },
            },
          ]"
        />
      </template>
    </Grid>
  </Page>
</template>
