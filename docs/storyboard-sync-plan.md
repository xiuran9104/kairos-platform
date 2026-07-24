# 分镜制作同步修复计划

更新日期：2026-07-20

对照目标：`/home/xiuran/code/Toonflow-app` 1.1.8 的分镜制作流程。

审计记录：`/.codex/audit/storyboard-sync/report.md`。

## 当前状态

### 已完成

- 完成 Kairos Platform 与 Toonflow-app 分镜制作页面、接口和数据结构的首轮对比。
- 新增独立 `StoryboardPanel.vue`，避免继续把分镜交互堆入项目详情页。
- 分镜卡片已接入：
  - 单条选择与全选。
  - 单条生成和失败重试。
  - 批量生成。
  - 强制重新生成。
  - 单条删除和批量删除。
  - 编辑分镜描述和图片提示词。
  - 生成状态、失败原因展示。
- 已接入分镜生成状态轮询，生成完成后自动更新卡片，不再依赖手动刷新。
- 后端批量新增分镜已按 `track` 分组：
  - 同组分镜共享 `trackId`。
  - 自动创建视频轨道。
  - 自动汇总组内分镜时长。
- 单条和批量删除分镜时已增加：
  - 清理分镜与资产关联。
  - 清理图片编辑流。
  - 删除空视频轨道。
  - 非空轨道重新汇总时长。
- `cargo fmt --check` 通过。
- `cargo check -p kairos-platform-toon-server` 通过；仅存在仓库原有 warning。
- 新增分镜组件及本次涉及的模板没有新增 TypeScript 报错。

### 当前验证限制

- 全量前端 typecheck 仍失败，报错集中在原有 `AgentChat.vue` 和 `detail.vue` 的消息类型、未使用变量及空值检查，不是本次分镜组件新增的错误。
- 当前项目所选剧本的分镜数为 0，尚未完成真实数据下的选择、批删、生成、失败重试和轮询端到端验证。
- Toonflow-app 依赖安装在中断前尚未确认完成，也尚未启动本地参考服务。

## 后续计划

### P0 收尾：功能闭环与回归验证

1. 检查 Toonflow-app 依赖安装结果；未完成时继续安装。
2. 启动 Toonflow-app 本地服务，使用账号 `admin`、密码 `root` 登录。
3. 使用 Google Chrome 实际走通 Toonflow-app 分镜流程并重新截图：
   - 进入项目与选择剧本。
   - 分镜表生成完成状态。
   - 分镜卡片选择与批量操作。
   - 单条生成、失败状态和重试。
   - 合成预览与图片编辑流入口。
4. 在 Kairos Platform 测试项目中准备可恢复的分镜数据，验证：
   - 同名 Track 是否共享一个 `trackId`。
   - 轨道时长是否等于组内分镜时长总和。
   - 轮询是否从“生成中”正确收敛到“已完成”或“生成失败”。
   - 单删、批删是否同步清理关联记录和空轨道。
5. 增加后端数据库集成测试，覆盖批量分组和删除清理边界。
6. 修复当前前端全量 typecheck 的既有错误，重新运行：

```bash
cargo test --workspace
bash script/test-database-migrations.sh
pnpm --dir apps/web --filter @vben/web-antd run typecheck
pnpm --dir apps/web --filter @vben/web-antd run build
```

### P1：同步 ToonFlow 工作台结构

1. 调整分镜制作整体布局：
   - Agent 面板支持折叠或拖动宽度。
   - 主画布设置最小可用宽度。
   - 1440px 视口下不再互相覆盖。
2. 改造固定五阶段画布：
   - 减少固定绝对坐标依赖。
   - 增加适应视图、定位当前阶段和重置缩放。
   - 分镜表与分镜卡片可以快速互相定位。
3. 将分镜按 Track 分组显示：
   - 展示组时长与分镜数量。
   - 支持组内选择和整组生成。
   - 展示轨道与视频制作阶段的对应关系。
4. 补齐图片编辑流入口：
   - 从单条分镜进入图片编辑流。
   - 保存生成图片后调用 `updateStoryboardUrl` 回写分镜。
5. 补齐合成预览下载按钮和下载接口调用。
6. 完善编辑能力，使时长、Track、关联资产、是否生成图片均可更新；修改后同步重算轨道。

### P2：视觉、响应式与无障碍

1. 修复 Agent 消息区白底浅色文字和深色主题对比度。
2. 为所有图标按钮补充可访问名称和 Tooltip。
3. 为画布节点、卡片选择、生成状态变化提供键盘操作和明确焦点样式。
4. 为生成成功、生成失败和批量删除结果增加屏幕阅读器可感知的状态通知。
5. 验证 1280px、1440px、1920px 和窄屏布局。
6. 避免通过过低默认缩放容纳全部节点，保证分镜正文默认可读。

## 建议实施顺序

1. 完成 Toonflow-app 启动和浏览器实测。
2. 完成 Kairos Platform P0 端到端测试及后端集成测试。
3. 修复前端全量 typecheck。
4. 实现图片编辑流回写与预览下载。
5. 实施 P1 布局和 Track 分组。
6. 完成 P2 无障碍和响应式收尾。

## 涉及文件

- `apps/web/apps/web-antd/src/views/toonflow/projects/StoryboardPanel.vue`
- `apps/web/apps/web-antd/src/views/toonflow/projects/ProductionFlowCanvas.vue`
- `apps/web/apps/web-antd/src/views/toonflow/projects/detail.vue`
- `apps/web/apps/web-antd/src/api/toonflow/index.ts`
- `crates/modules/toon-server/src/toonflow.rs`
- `crates/modules/toon-server/src/toonflow_image_workflow.rs`
