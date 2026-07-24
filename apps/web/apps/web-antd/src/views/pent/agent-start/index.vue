<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { message } from 'ant-design-vue';
import {
  confirmPentSessionScope,
  createPentSession,
  getPentProjects,
  startPentSessionRun,
} from '#/api/pent';

const router = useRouter();
const projects = ref<any[]>([]);
const currentStep = ref(0);
const creating = ref(false);
const starting = ref(false);
const createdSession = ref<any>(null);

const form = ref({
  project_id: undefined as string | undefined,
  objective: '',
  targetsText: '',
  maxMinutes: 120,
  maxTokens: 1_000_000,
  maxTurns: 50,
  maxConcurrency: 5,
  noDestructiveChanges: true,
  riskLevel: 'medium' as 'low' | 'medium' | 'high',
  allowedTools: [] as string[],
});

const targets = computed(() =>
  form.value.targetsText.split(/[\n,]/).map(t => t.trim()).filter(Boolean),
);

const riskLabels: Record<string, string> = {
  low: '低风险（仅读操作）',
  medium: '中风险（标准扫描）',
  high: '高风险（含主动利用）',
};

const steps = [
  { title: '选择项目', description: '关联已有安全项目或独立创建' },
  { title: '描述目标', description: '自然语言 + 授权资产' },
  { title: '配置预算', description: '时间、轮次、并发和风险' },
  { title: '确认范围', description: '审核 Scope Proposal 并启动' },
];

function canProceed(step: number): boolean {
  switch (step) {
    case 0: return true; // Project is optional
    case 1: return form.value.objective.trim().length > 0 && targets.value.length > 0;
    case 2: return form.value.maxMinutes >= 10 && form.value.maxTurns >= 1;
    case 3: return !!createdSession.value;
    default: return false;
  }
}

onMounted(async () => {
  try {
    projects.value = (await getPentProjects()) as any[];
  } catch {
    message.warning('项目列表加载失败，仍可创建独立 Agent Session');
  }
});

async function proposeScope() {
  if (!canProceed(1)) { message.warning('请完整填写目标描述和授权资产'); return; }
  creating.value = true;
  try {
    createdSession.value = await createPentSession({
      project_id: form.value.project_id,
      objective: form.value.objective.trim(),
      targets: targets.value,
      constraints: {
        no_destructive_changes: form.value.noDestructiveChanges,
        max_duration_minutes: form.value.maxMinutes,
        max_turns: form.value.maxTurns,
        max_concurrency: form.value.maxConcurrency,
        risk_level: form.value.riskLevel,
      },
      budget: { max_tokens: form.value.maxTokens },
    });
    currentStep.value = 3;
    message.success('候选范围已生成，请审核后确认启动');
  } catch (e: any) {
    message.error(e?.message || '生成范围失败');
  } finally {
    creating.value = false;
  }
}

async function confirmAndStart() {
  if (!createdSession.value?.id) return;
  starting.value = true;
  try {
    await confirmPentSessionScope(createdSession.value.id, {
      targets: targets.value,
      constraints: {
        no_destructive_changes: form.value.noDestructiveChanges,
        max_duration_minutes: form.value.maxMinutes,
      },
    });
    await startPentSessionRun(createdSession.value.id, {
      name: form.value.objective.slice(0, 80),
      priority: 'normal',
    });
    message.success('Agent 已进入后台执行队列');
    await router.push(`/pent/sessions/${createdSession.value.id}`);
  } catch (e: any) {
    message.error(e?.message || '启动失败');
  } finally {
    starting.value = false;
  }
}

function reset() {
  createdSession.value = null;
  currentStep.value = 0;
}
</script>

<template>
  <div class="p-6 max-w-5xl mx-auto">
    <a-page-header title="启动 Pent Agent" sub-title="对话驱动、授权范围内自主执行">
      <template #extra>
        <a-button @click="router.push('/pent/sessions')">查看历史 Session</a-button>
      </template>
    </a-page-header>

    <!-- Steps indicator -->
    <a-card :bordered="false" class="mb-4">
      <a-steps :current="currentStep" size="small">
        <a-step v-for="s in steps" :key="s.title" :title="s.title" :description="s.description" />
      </a-steps>
    </a-card>

    <a-row :gutter="16">
      <!-- Left: Form steps -->
      <a-col :xs="24" :lg="15">
        <!-- Step 0: Select project -->
        <a-card v-show="currentStep === 0" title="步骤 1：关联项目" :bordered="false">
          <a-alert message="选择已有安全项目或跳过，独立创建 Agent Session" type="info" show-icon class="mb-4" />
          <a-form layout="vertical">
            <a-form-item label="关联安全项目">
              <a-select
                v-model:value="form.project_id"
                allow-clear
                placeholder="可选：关联已有安全项目"
                :options="projects.map(p => ({ label: p.name || p.id?.slice(0,12), value: p.id }))"
                style="width: 100%"
              />
            </a-form-item>
          </a-form>
          <a-button type="primary" @click="currentStep = 1">下一步：描述目标</a-button>
        </a-card>

        <!-- Step 1: Describe objective -->
        <a-card v-show="currentStep === 1" title="步骤 2：描述测试目标" :bordered="false">
          <a-alert
            message="用自然语言描述测试目标。Agent 只提取候选结构，最终范围需人工确认。"
            type="info" show-icon class="mb-4"
          />
          <a-form layout="vertical">
            <a-form-item label="测试目标（自然语言）" required>
              <a-textarea
                v-model:value="form.objective"
                :rows="6"
                placeholder="例如：对 test.example.com 做一次授权安全评估，重点检查登录、越权和文件上传，不要修改生产数据，最多运行两小时。"
              />
            </a-form-item>
            <a-form-item label="授权资产" required>
              <a-textarea
                v-model:value="form.targetsText"
                :rows="4"
                placeholder="每行一个 IP、CIDR、域名或 URL&#10;例如：&#10;192.168.1.0/24&#10;test.example.com&#10;https://api.example.com"
              />
              <div class="text-gray-400 text-xs mt-1">
                <span v-if="targets.length">已识别 {{ targets.length }} 个目标</span>
                <span v-else>请输入至少一个授权目标</span>
              </div>
            </a-form-item>
          </a-form>
          <a-space>
            <a-button @click="currentStep = 0">上一步</a-button>
            <a-button type="primary" @click="currentStep = 2" :disabled="!canProceed(1)">
              下一步：配置预算
            </a-button>
          </a-space>
        </a-card>

        <!-- Step 2: Configure budget -->
        <a-card v-show="currentStep === 2" title="步骤 3：配置执行预算" :bordered="false">
          <a-alert message="以下限制由平台强制执行，Agent 无法在运行中绕过。" type="warning" show-icon class="mb-4" />
          <a-form layout="vertical">
            <a-row :gutter="16">
              <a-col :span="8">
                <a-form-item label="最大运行时间（分钟）">
                  <a-input-number v-model:value="form.maxMinutes" :min="10" :max="1440" style="width: 100%" />
                </a-form-item>
              </a-col>
              <a-col :span="8">
                <a-form-item label="最大轮次 (Turns)">
                  <a-input-number v-model:value="form.maxTurns" :min="1" :max="500" style="width: 100%" />
                </a-form-item>
              </a-col>
              <a-col :span="8">
                <a-form-item label="最大并发">
                  <a-input-number v-model:value="form.maxConcurrency" :min="1" :max="20" style="width: 100%" />
                </a-form-item>
              </a-col>
            </a-row>
            <a-row :gutter="16">
              <a-col :span="8">
                <a-form-item label="Token 预算">
                  <a-input-number v-model:value="form.maxTokens" :min="10000" :step="100000" style="width: 100%" />
                </a-form-item>
              </a-col>
              <a-col :span="8">
                <a-form-item label="风险级别">
                  <a-select v-model:value="form.riskLevel" :options="[
                    { label: '低风险（仅读操作）', value: 'low' },
                    { label: '中风险（标准扫描）', value: 'medium' },
                    { label: '高风险（含主动利用）', value: 'high' },
                  ]" />
                </a-form-item>
              </a-col>
              <a-col :span="8">
                <a-form-item label="禁止破坏性变更">
                  <a-switch v-model:checked="form.noDestructiveChanges" checked-children="是" un-checked-children="否" />
                </a-form-item>
              </a-col>
            </a-row>
          </a-form>
          <a-space>
            <a-button @click="currentStep = 1">上一步</a-button>
            <a-button type="primary" :loading="creating" @click="proposeScope">
              生成执行范围
            </a-button>
          </a-space>
        </a-card>

        <!-- Step 3: Confirm scope -->
        <a-card v-show="currentStep === 3" title="步骤 4：确认授权范围" :bordered="false">
          <template v-if="createdSession">
            <a-alert
              message="请仔细审核以下范围。确认后 Agent 将在这些边界内自主执行，不可通过对话绕过。"
              type="warning" show-icon class="mb-4"
            />
            <a-descriptions :column="2" bordered size="small" class="mb-4">
              <a-descriptions-item label="Session ID" :span="2">{{ createdSession.id }}</a-descriptions-item>
              <a-descriptions-item label="测试目标" :span="2">{{ form.objective }}</a-descriptions-item>
              <a-descriptions-item label="授权目标" :span="2">
                <a-space wrap><a-tag v-for="t in targets" :key="t" color="blue">{{ t }}</a-tag></a-space>
              </a-descriptions-item>
              <a-descriptions-item label="风险级别">
                <a-tag :color="form.riskLevel === 'high' ? 'red' : form.riskLevel === 'medium' ? 'orange' : 'green'">
                  {{ riskLabels[form.riskLevel] }}
                </a-tag>
              </a-descriptions-item>
              <a-descriptions-item label="禁止破坏性变更">
                <a-tag :color="form.noDestructiveChanges ? 'green' : 'red'">
                  {{ form.noDestructiveChanges ? '是' : '否' }}
                </a-tag>
              </a-descriptions-item>
              <a-descriptions-item label="时间上限">{{ form.maxMinutes }} 分钟</a-descriptions-item>
              <a-descriptions-item label="最大轮次">{{ form.maxTurns }} 轮</a-descriptions-item>
              <a-descriptions-item label="最大并发">{{ form.maxConcurrency }}</a-descriptions-item>
              <a-descriptions-item label="Token 预算">{{ (form.maxTokens / 1000).toFixed(0) }}K</a-descriptions-item>
              <a-descriptions-item label="审批策略" :span="2">
                {{ form.riskLevel === 'high' ? '所有高风险操作需人工审批' : '高风险操作需人工审批' }}
              </a-descriptions-item>
            </a-descriptions>

            <a-alert
              message="此确认仅表示授权以上范围。文件大小、路径穿越、凭据保护等平台硬限制始终生效，不可通过对话关闭。"
              type="error" show-icon class="mb-4"
            />

            <a-space>
              <a-button @click="reset">重新配置</a-button>
              <a-button type="primary" danger size="large" :loading="starting" @click="confirmAndStart">
                确认授权并启动 Agent
              </a-button>
            </a-space>
          </template>
        </a-card>
      </a-col>

      <!-- Right: Scope preview sidebar -->
      <a-col :xs="24" :lg="9">
        <a-card title="执行范围预览" :bordered="false" size="small">
          <a-empty v-if="!targets.length" description="输入目标后自动预览" :image="false" />
          <template v-else>
            <a-descriptions :column="1" size="small" bordered>
              <a-descriptions-item label="目标数">{{ targets.length }}</a-descriptions-item>
              <a-descriptions-item label="时间预算">{{ form.maxMinutes }} 分钟</a-descriptions-item>
              <a-descriptions-item label="Token 预算">{{ (form.maxTokens / 1000).toFixed(0) }}K</a-descriptions-item>
              <a-descriptions-item label="最大轮次">{{ form.maxTurns }}</a-descriptions-item>
              <a-descriptions-item label="风险级别">
                <a-tag :color="form.riskLevel === 'high' ? 'red' : form.riskLevel === 'medium' ? 'orange' : 'green'">
                  {{ riskLabels[form.riskLevel] }}
                </a-tag>
              </a-descriptions-item>
            </a-descriptions>

            <a-divider />

            <div class="text-sm">
              <div class="font-medium mb-2">允许目标</div>
              <a-tag v-for="t in targets" :key="'allow-'+t" color="green" class="mb-1">{{ t }}</a-tag>
            </div>

            <div class="text-sm mt-3">
              <div class="font-medium mb-2">禁止操作</div>
              <a-tag v-if="form.noDestructiveChanges" color="red">破坏性变更</a-tag>
              <a-tag color="red">超出授权范围</a-tag>
              <a-tag color="red">修改生产数据</a-tag>
            </div>

            <a-divider />

            <a-alert
              message="LLM 只负责提取候选结构。最终范围由确定性代码校验，必须人工确认。"
              type="info" show-icon :banner="false"
            />
          </template>
        </a-card>
      </a-col>
    </a-row>
  </div>
</template>
