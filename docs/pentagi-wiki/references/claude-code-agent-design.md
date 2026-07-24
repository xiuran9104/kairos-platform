---
type: reference
title: Claude Code Agent Design Notes
tags: [claude-code, agent-runtime, chat-runtime]
source_url: https://github.com/sawzhang/deep-dive-claude-code
related: [references/index]
---

# Claude Code Agent Design Notes

PentAGI follows the same build order for the initial runtime:

1. Chat runtime: session, messages, model adapter, streaming output.
2. Tool runtime: schema, registry, dispatch, result injection.
3. Error runtime: validation, timeout, retry, normalized failures.
4. Context, permissions, and state recovery.
5. Advanced agent features: planner/executor, subagents, MCP, and TUI.

For the first implementation, only the chat runtime is active. The Rust module
boundary is intentionally narrow:

- `message`: role-tagged transcript events.
- `session`: durable multi-turn conversation state.
- `model`: swappable LLM adapters with streaming output.
- `agent`: turn orchestration.
- `store`: JSON session persistence.

## Model Providers

The default provider is `claude`, implemented as an Anthropic-compatible
Messages API adapter. It reads configuration in this order:

1. CLI flags and `PENTAGI_*` environment variables.
2. Standard Anthropic environment variables.
3. `~/.claude/settings.json` `env` entries, including `ANTHROPIC_BASE_URL`,
   `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_MODEL`.

This keeps PentAGI aligned with Claude Code-style configuration while still
allowing OpenAI-compatible and local echo adapters for testing.

PentAGI exposes common provider presets through `pentagi chat --provider`:

| Provider | Protocol | Default base | Default model | Key variables |
| --- | --- | --- | --- | --- |
| `claude` | Anthropic Messages | `~/.claude/settings.json` or Anthropic default | Claude settings model | `PENTAGI_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY` |
| `deepseek` | OpenAI chat completions | `https://api.deepseek.com` | `deepseek-v4-pro` | `PENTAGI_API_KEY`, `DEEPSEEK_API_KEY` |
| `deepseek-anthropic` | Anthropic Messages | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` | `PENTAGI_API_KEY`, `DEEPSEEK_API_KEY`, `ANTHROPIC_AUTH_TOKEN` |
| `glm` | OpenAI chat completions | `https://open.bigmodel.cn/api/paas/v4` | `glm-4.5` | `PENTAGI_API_KEY`, `GLM_API_KEY`, `ZHIPUAI_API_KEY`, `BIGMODEL_API_KEY` |
| `qwen` | OpenAI chat completions | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` | `PENTAGI_API_KEY`, `QWEN_API_KEY`, `DASHSCOPE_API_KEY` |
| `siliconflow` | OpenAI chat completions | `https://api.siliconflow.cn/v1` | `Qwen/Qwen3-Coder-480B-A35B-Instruct` | `PENTAGI_API_KEY`, `SILICONFLOW_API_KEY` |
| `openrouter` | OpenAI chat completions | `https://openrouter.ai/api/v1` | `qwen/qwen3-coder` | `PENTAGI_API_KEY`, `OPENROUTER_API_KEY` |
| `ollama` | OpenAI chat completions | `http://localhost:11434/v1` | `qwen3:8b` | optional `OLLAMA_API_KEY` |

All providers accept `--model`, `PENTAGI_MODEL`, `PENTAGI_API_BASE`, and
`PENTAGI_API_KEY` as top-priority overrides.

## Tool Runtime

The first tool runtime implementation uses one unified call shape:

```json
{
  "name": "read_file",
  "arguments": { "path": "Cargo.toml" }
}
```

Built-in tools currently registered. PentAGI exposes snake_case native tools
and Claude Code-compatible PascalCase aliases, so both `read_file` and `Read`
style calls can work through the same agent loop:

| Tool | Capability | Status |
| --- | --- | --- |
| `read_file` | read | implemented |
| `read` | read | Claude-compatible alias |
| `write_file` | write | implemented, explicit permission required |
| `write` | write | Claude-compatible alias, explicit permission required |
| `list_dir` | read | implemented |
| `ls` | read | Claude-compatible alias |
| `search_files` | read | implemented as literal string search |
| `glob` | read | implemented |
| `grep` | read | implemented as regex search with optional include glob |
| `edit_file` | write | implemented with exact string replacement semantics |
| `edit` | write | Claude-compatible alias, exact string replacement semantics |
| `multi_edit` | write | implemented with ordered exact string replacements |
| `bash` | exec | implemented, explicit permission required |
| `Bash` | exec | Claude-compatible alias, explicit permission required |
| `bash_output` | exec | implemented for reading background bash session output |
| `BashOutput` | exec | Claude-compatible alias |
| `kill_bash` | exec | implemented for stopping background bash sessions |
| `KillBash` | exec | Claude-compatible alias |
| `powershell` | exec | implemented through `pwsh`, explicit permission required |
| `todo_read` | read | implemented as in-memory workspace checklist |
| `TodoRead` | read | Claude-compatible alias |
| `todo_write` | read | implemented as in-memory workspace checklist update |
| `TodoWrite` | read | Claude-compatible alias |
| `notebook_read` | read | implemented for Jupyter `.ipynb` cell summaries |
| `NotebookRead` | read | Claude-compatible alias |
| `notebook_edit` | write | implemented for replacing one Jupyter cell source |
| `NotebookEdit` | write | Claude-compatible alias |
| `web_fetch` | network | implemented, explicit network permission required |
| `WebFetch` | network | Claude-compatible alias, explicit network permission required |
| `web_search` | network | implemented through DuckDuckGo HTML fetch, explicit network permission required |
| `WebSearch` | network | Claude-compatible alias, explicit network permission required |
| `task_create` / `task_get` / `task_update` / `task_list` / `task_output` / `task_stop` | read/write by operation | implemented as generic persisted workspace tasks |
| `TaskCreate` / `TaskGet` / `TaskUpdate` / `TaskList` / `TaskOutput` / `TaskStop` | read/write by operation | Claude-compatible aliases |
| `exit_plan_mode` | read | registered no-op compatibility tool |
| `ExitPlanMode` | read | Claude-compatible alias |
| `mcp_call` | mcp | implemented for configured stdio and HTTP MCP servers |
| `mcp_list_servers` / `mcp_health` / `mcp_list_tools` / `mcp_list_resources` / `mcp_read_resource` | mcp | implemented MCP discovery and resource operations |

The compatibility layer now exposes 40+ tool definitions. Some Claude Code
tools map to the same implementation because PentAGI keeps one native function
per behavior and registers aliases for Claude naming compatibility.

Pentest tools are intentionally not part of the Claude-compatible layer. They
live under `tools/pentest/` and are registered separately:

- `pentest_scope_set`
- `pentest_scope_get`
- `pentest_tool_check`
- `nmap_scan`
- `ffuf_fuzz`
- `nuclei_scan`
- `sqlmap_scan`
- `http_probe`
- `recon_basic`
- `web_discovery`
- `vuln_scan_basic`
- `evidence_collect`
- `report_draft`

The pentest tool layer currently provides architecture, schemas, permissions,
and scope gates. External adapter execution is implemented incrementally after
the layer boundary is stable.

This layer is internal to the agent loop. Hidden development commands exist for
manual diagnostics, but they are not part of the public CLI surface:

```bash
cargo run -p pentagi -- tool list
cargo run -p pentagi -- tool call read_file --args '{"path":"Cargo.toml"}'
cargo run -p pentagi -- tool --allow-exec call bash --args '{"cmd":"echo ok"}'
```

Read tools are allowed by default. Write, exec, network, destructive, and MCP
capabilities return a permission-required result unless the CLI explicitly
allows that capability. Filesystem tools resolve paths against the workspace
root and canonicalize existing ancestors to prevent workspace escape through
`..` or symlinks.

The LLM agent loop supports a first native tool-calling pass:

1. Send tool definitions to the model.
2. Parse OpenAI-compatible `tool_calls` or Anthropic-compatible `tool_use`.
3. Record assistant tool calls in session history.
4. Execute through `ToolRegistry` under `PermissionPolicy`.
5. Record `tool_result` messages.
6. Re-enter the model until it returns a normal assistant answer or the maximum
   tool round count is reached.

The loop now treats failures as recoverable runtime events:

- `AgentEvent` is serialized as tagged JSON and used directly by audit logs.
- `MockParityModel` provides deterministic model outputs for parity tests.
- `pentagi doctor` checks local runtime health before interactive use.
- `run_tool_loop_stream` yields events as they are produced.
- `CancellationToken` supports stopping a run before model or tool work.
- Background bash sessions are wired through `BashOutput` and `KillBash`.
- `Task` is model-bound in chat runtime and runs sub-agents concurrently through
  `SubAgentOrchestrator`.
- Chapter-10-style agent definitions are implemented through `AgentDefinition`
  and `AgentCatalog`, including built-in agents, Markdown/JSON parsing, tool
  filtering, `max_turns`, and `Task.agent_type` routing.
- Model failures are retried through `AgentRunConfig`.
- Retry attempts emit `ModelCallRetry`.
- Tool execution errors are converted into failed `tool_result` messages.
- Unknown tools and invalid tool parameters no longer abort the whole turn.
- Permission denial is returned to the model as a failed `tool_result`.

Enable it from the CLI with:

```bash
cargo run -p pentagi -- chat
```

The chat command is now tool-first. The old plain streaming chat path was
removed from the CLI; model streaming remains available internally for tests and
future event-stream work.

## Approval Flow

PentAGI now has a Claude Code-style approval boundary:

1. `PermissionPolicy` classifies a tool capability as allow, deny, or ask.
2. `ChatAgent` emits `PermissionRequested` for ask-level actions.
3. An `ApprovalHandler` decides whether to allow once, allow always for this
   tool in the current CLI session, or deny.
4. Denied actions are written back as `tool_result` errors so the model can
   recover or propose a safer alternative.

CLI behavior:

```bash
cargo run -p pentagi -- chat
```

Read tools run without prompting. Write, exec, and MCP tools prompt:

```text
allow? [y] once / [a] always for this tool / [n] deny
```

The explicit bypass flags remain available for controlled runs:

```bash
cargo run -p pentagi -- chat --allow-write
cargo run -p pentagi -- chat --allow-exec
cargo run -p pentagi -- chat --allow-mcp
```

## Audit Log

PentAGI writes JSONL audit events for each chat session:

```text
~/.pentagi/audit/<session-id>.jsonl
```

The initial audit stream records:

- user input
- permission requests
- tool call requests
- tool call results
- assistant messages

This is intentionally append-only JSONL so later report generation, replay, and
finding evidence extraction can consume it without coupling to a database.

## Context And LLMWiki

`ContextManager` prepares every turn before the model call:

1. If session content grows beyond the configured character budget, older
   messages are compacted into a `[PentAGI Context Summary]` system message.
2. The current user input is searched against a local Tantivy BM25 index built
   from LLMWiki Markdown under `wiki/`.
3. The best matching notes are injected as a transient
   `[PentAGI LLMWiki Context]` system message.

Markdown remains the source of truth. Tantivy index files are derived cache
under `~/.pentagi/wiki-index/<wiki-root-hash>/search-index/`, matching the
geronimo-llm-wiki direction of Markdown pages compiled into a local BM25
search index.

Current scope uses a fixed schema:

- `path`
- `title`
- `heading`
- `body`
- `start_line`
- `end_line`

Next geronimo alignment step: add `wiki.toml` + `schemas/*.json` dynamic schema,
state metadata, staleness checks, incremental update, and corruption recovery.

Public CLI controls:

```bash
cargo run -p pentagi -- chat --wiki-root wiki
cargo run -p pentagi -- chat --no-wiki-context
cargo run -p pentagi -- chat --audit-root /tmp/pentagi-audit
```

Hidden development commands for index diagnostics:

```bash
cargo run -p pentagi -- wiki --wiki-root wiki rebuild
cargo run -p pentagi -- wiki --wiki-root wiki search "nmap port"
```

## Claw Code Reference Points

`ultraworkers/claw-code` is useful as a Rust CLI agent harness reference, but
it should not become a product dependency. The parts worth carrying into
PentAGI are engineering contracts:

1. Health checks: add a `doctor`-style diagnostic that validates Rust build,
   provider credentials, selected model, wiki index state, writable session
   store, and tool permission policy.
2. Provider setup: keep one provider abstraction, with Anthropic-compatible and
   OpenAI-compatible transports underneath it, so DeepSeek, GLM, Qwen,
   SiliconFlow, OpenRouter, Ollama, and Claude share the same agent loop.
3. Parity harness: add deterministic mock model responses and mock tool traces
   so tool-calling behavior can be tested without live API calls.
4. Event/report contract: formalize `AgentEvent` and audit JSONL as stable
   internal contracts before adding TUI, MCP transport, or automated reporting.
5. Minimal public surface: keep normal users on `pentagi chat`; expose internal
   tool/wiki/debug commands only as hidden developer commands unless they are
   needed for operator health checks.

The next implementation step from this reference is not more ordinary chat. It
is a health-check and parity layer around the current tool-first agent runtime.
