<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import { getPentProjects } from '#/api/pent';
import { requestClient } from '#/api/request';

defineOptions({ name: 'PentAgentStart' });

const router = useRouter();
const projects = ref<any[]>([]);
const creating = ref(false);

const form = ref({
  project_id: undefined as string | undefined,
  name: '',
  description: '',
  targetsText: '',
  priority: 'medium' as 'low' | 'medium' | 'high' | 'critical',
  orchestration: 'deep' as 'deep' | 'plan_execute' | 'supervisor',
});

const targets = computed(() =>
  form.value.targetsText
    .split(/[\n,]/)
    .map((t) => t.trim())
    .filter(Boolean),
);

onMounted(async () => {
  try {
    projects.value = (await getPentProjects()) as any[];
  } catch { /* optional */ }
});

async function startTask() {
  if (!form.value.name.trim()) {
    message.warning('请输入任务名称');
    return;
  }
  if (targets.value.length === 0) {
    message.warning('请输入至少一个目标 URL');
    return;
  }
  creating.value = true;
  try {
    const resp: any = await requestClient.post('/api/tasks', {
      name: form.value.name.trim(),
      description: form.value.description.trim() || undefined,
      project_id: form.value.project_id || undefined,
      targets: targets.value,
      priority: form.value.priority,
      orchestration: form.value.orchestration,
    });
    const taskId = resp?.task_id ?? resp?.data?.task_id;
    if (!taskId) throw new Error('no task_id returned');
    message.success('任务已创建，Agent 正在自主执行');
    router.push(`/pent/tasks/${taskId}`);
  } catch (e: any) {
    message.error(e?.message || '创建任务失败');
  } finally {
    creating.value = false;
  }
}
</script>

<template>
  <div class="p-6 max-w-3xl mx-auto">
    <a-page-header title="新建渗透测试任务" sub-title="输入目标，Agent 全自主执行">
      <template #extra>
        <a-space>
          <a-button @click="router.push('/pent/tasks')">任务列表</a-button>
          <a-button @click="router.push('/pent/projects')">项目管理</a-button>
        </a-space>
      </template>
    </a-page-header>

    <a-card :bordered="false" class="mb-4">
      <a-form layout="vertical">
        <a-row :gutter="16">
          <a-col :span="12">
            <a-form-item label="任务名称" required>
              <a-input v-model:value="form.name" placeholder="例如：Juice Shop 安全评估" />
            </a-form-item>
          </a-col>
          <a-col :span="6">
            <a-form-item label="优先级">
              <a-select
                v-model:value="form.priority"
                :options="[
                  { label: '低', value: 'low' },
                  { label: '中', value: 'medium' },
                  { label: '高', value: 'high' },
                  { label: '紧急', value: 'critical' },
                ]"
              />
            </a-form-item>
          </a-col>
          <a-col :span="6">
            <a-form-item label="关联项目">
              <a-select
                v-model:value="form.project_id"
                allow-clear
                placeholder="可选"
                :options="projects.map((p: any) => ({ label: p.name || p.id?.slice(0, 12), value: p.id }))"
              />
            </a-form-item>
          </a-col>
        </a-row>

        <a-form-item label="任务描述">
          <a-textarea
            v-model:value="form.description"
            :rows="2"
            placeholder="可选：描述测试范围和关注重点"
          />
        </a-form-item>

        <a-form-item label="Agent 编排模式">
          <a-radio-group v-model:value="form.orchestration" button-style="solid">
            <a-radio-button value="deep">Deep 多代理</a-radio-button>
            <a-radio-button value="plan_execute">Plan → Execute → Replan</a-radio-button>
            <a-radio-button value="supervisor">Supervisor 专家路由</a-radio-button>
          </a-radio-group>
        </a-form-item>

        <a-form-item label="目标 URL" required>
          <a-textarea
            v-model:value="form.targetsText"
            :rows="5"
            placeholder="每行一个 URL 或 IP&#10;例如：&#10;http://localhost:13000&#10;http://localhost:18081&#10;https://example.com"
          />
          <div style="color: #999; font-size: 12px; margin-top: 4px">
            已输入 <a-tag color="blue" size="small">{{ targets.length }}</a-tag> 个目标
          </div>
        </a-form-item>

        <a-divider />

        <a-alert
          message="任务创建后 Agent 将全自主执行：信息收集 → 漏洞扫描 → 验证 → 生成报告。您可以在监控面板实时查看进度。"
          type="info"
          show-icon
          class="mb-4"
        />

        <a-button
          type="primary"
          size="large"
          :loading="creating"
          @click="startTask"
          style="width: 100%"
        >
          {{ creating ? '正在创建任务…' : '启动自主渗透任务' }}
        </a-button>
      </a-form>
    </a-card>

    <!-- Quick presets -->
    <a-card title="快速目标" :bordered="false" size="small">
      <a-space wrap>
        <a-button
          v-for="target in [
            { label: 'Juice Shop', url: 'http://localhost:13000' },
            { label: 'DVWA', url: 'http://localhost:18081' },
            { label: 'WebGoat', url: 'http://localhost:18080' },
            { label: 'Metasploitable2', url: 'http://localhost:20080' },
          ]"
          :key="target.url"
          size="small"
          @click="form.targetsText = target.url; form.name = target.label + ' 安全测试'"
        >
          {{ target.label }}
        </a-button>
      </a-space>
    </a-card>
  </div>
</template>
