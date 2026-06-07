# agent-workbench → HarnessLab Desktop 重写计划

> 把 **agent-workbench** 原地重写为一个**桌面版的 HarnessLab**，并**彻底去 Python**。
> 技术形态：**Tauri v2 桌面壳 + `harnesslab-core`(Rust) + 复用 `piscis-kernel` 引擎 + 移植 HarnessLab 的 React 画布前端**，
> 用 **Tauri IPC（command/event）取代 FastAPI 的 REST/SSE**。
>
> 本文为架构 / 迁移映射 / 路线图的方案稿，供评审与实施依据。

状态图例：✅ 已具备 · 🟡 需扩展 · ⬜ 待新建

---

## 0. 已确认决策

| 决策点 | 选择 |
| --- | --- |
| 桌面外壳 | **Tauri v2**（照搬同仓库 `openpiscis` / `AgentZ` 模式，原生 Rust 后端） |
| v1 范围 | **砍掉 `bus`（分布式总线）与 `collab`（多用户协作）**，专注单机单用户 |
| 前端策略 | **移植 HarnessLab 前端**（画布成熟、功能全），仅替换数据访问层 |
| 语言 | **Rust + TypeScript/React**，不再使用 Python |

---

## 1. 现状盘点

### 1.1 HarnessLab（被对标的源平台，纯 Python Web）
- 后端 Python 约 9.5k 行，模块：`engine / library / project / experiment / factory / design / market / api / bus / collab / runtime`。
- 前端 React 19 + Vite + `@xyflow/react` + `zustand`（Visual Designer 画布、Inspector、Palette、Dock、Session/History/Monitor）。
- 通过 FastAPI 暴露 `/api/v1/*` 的 REST + SSE。是一个 **Web 应用**。

### 1.2 agent-workbench（被重写的目标仓库，当前是混合体）
- Rust 约 2.5k 行（`src/`），已链接 `piscis-kernel`（git rev `v0.8.42`），`config.rs` / `loader.rs` 已实现 YAML→`HarnessConfig` 桥接。
- Python `backend/`（FastAPI）+ `core/*.py` + `tests/*.py`（**将全部移除/改写为 Rust**）。
- React 前端（`frontend/`，相对简单）。

### 1.3 piscis-engine / piscis-kernel（Rust 引擎，复用核心）
- 成熟 Rust agent 运行时（约 38k 行）：`agent loop`、`loop_strategy`、LLM 客户端、tools、memory、compaction、`harness::HarnessConfig`、`contrib` 注册表、`pool`（多智能体协调）。
- 自带 `rusqlite`（bundled SQLite）、`reqwest`、`ed25519` 友好生态。
- 明确**不依赖任何 GUI crate**，适合作为纯库被桌面壳调用。

### 1.4 现成桌面模板
- `openpiscis` / `AgentZ` 均为 **Tauri v2 + piscis-kernel + React** 桌面应用，`src-tauri/` 结构、插件（shell/dialog/fs/notification/updater）可直接照搬。

---

## 2. 目标架构

```
┌──────────────── 前端 (React + TS + Vite，移植自 HarnessLab) ────────────────┐
│  Palette   Canvas(@xyflow/react)   Inspector   Copilot / RunConsole / History │
│                         │  invoke() / event listen                            │
└─────────────────────────┼─────────────────────────────────────────────────────┘
                          Tauri IPC（取代 HTTP REST + SSE）
┌─────────────────────────▼──────────── src-tauri (Rust bin) ──────────────────┐
│  commands: projects / ops / registry / eval / factory / design / market ...    │
│  events:   run 事件流（取代 SSE）         SQLite(rusqlite) 本地存储            │
└─────────────────────────┬──────────────────────────────────────────────────────┘
                          调用
┌─────────────────────────▼──────── harnesslab-core (Rust lib) ────────────────┐
│  project/ops · library/registry · experiment/evaluator · factory(codegen)      │
│  design(edit-ops planner) · market(包协议+签名) · scorer/coordinator           │
└─────────────────────────┬──────────────────────────────────────────────────────┘
                          复用
              piscis-kernel（agent loop / LLM / tools / memory / compaction / pool）
```

**单一真相源原则不变**：画布、Inspector、生成代码、AI 修改全部读写同一个 `Project`（Rust serde 模型）；视图状态（坐标/视口/折叠）独立存为 `ProjectLayout`，不污染可运行模型。

---

## 3. 模块迁移映射（Python → Rust）

| HarnessLab Python 模块 | 行数 | 目标处理 | 说明 |
| --- | --- | --- | --- |
| `engine/`（loop/harness/llm/tools/events） | 949 | **复用 piscis-kernel** | kernel 已有 agent loop、loop_strategy、LLM、tools、compaction；`loader.rs` 已桥接，按需扩展 |
| `library/`（registry + loop/compaction/memory/context/coordinator/scorer/stages） | 2261 | **Rust 重写 + kernel contrib** | 单体算法走 kernel；多智能体 coordinator（coscientist/tournament/debate/map_reduce/hierarchical）与 scorer 在 `harnesslab-core` 实现，注册到统一 catalog |
| `project/`（model + ops + store） | 611 | **Rust 重写** | `serde` 结构体 + 事务性 Edit Ops 引擎 + `rusqlite` 存储 |
| `experiment/`（datasets/runner/evaluator/autoloop） | 509 | **Rust 重写** | 数据集加载 + agent×dataset 评测 + 自改进 autoloop |
| `factory/`（IR + py/ts/rust emitters，Jinja2 模板） | 1021 | **Rust 重写** | 用 `minijinja` / `askama` 或字符串模板，沿用现有模板内容产 py/ts/rust |
| `design/`（Architect + edit-ops 规划器） | 485 | **Rust 重写** | NL→校验过的 Edit Ops；确定性规则 + 可选 LLM 规划 |
| `market/`（包协议 + 签名） | 767 | **Rust 重写** | `ed25519` + HMAC-SHA256，sha256 完整性；git/url/file 源 |
| `runtime/`（RunMonitor + 实时监控） | 291 | **Rust 重写** | 进程内运行注册表，经 Tauri event 推前端 |
| `api/`（FastAPI 路由 13 文件） | 1068 | **改写为 Tauri commands** | REST → `#[tauri::command]`；体量大幅缩小 |
| `bus/`（分布式 worker 总线） | 1174 | **砍掉** | 跨进程/跨语言 worker 总线对单机桌面无意义 |
| `collab/`（多用户在线/租约/SSE） | 250 | **砍掉** | 桌面默认单用户；保留本地 run history / session tree |

**可直接复用的 agent-workbench 现有 Rust 资产**（作为 `harnesslab-core` 地基）：
`config.rs`、`loader.rs`、`contrib.rs`、`context_managers.rs`、`hooks.rs`、`plugins.rs`。

**待删除的 Python 资产**：`agent-workbench/backend/`、`core/*.py`、`tests/*.py`、`pyproject.toml`、`pytest.ini`、`.venv`、`src/main.rs`（旧 CLI，按需保留为 headless 入口）。

---

## 4. 接口形态换代：REST/SSE → Tauri IPC

| HarnessLab（Web） | HarnessLab Desktop（Tauri） |
| --- | --- |
| `GET /projects`, `POST /projects/{id}/ops` | `#[tauri::command] list_projects() / apply_ops(...)` |
| `GET /registry`, `.../reference` | `get_registry() / get_reference(kind, name)` |
| `GET/PUT /layouts/{id}` | `get_layout(id) / put_layout(id, layout)` |
| `POST /projects/{id}/run/stream`（SSE） | `start_run(...)` + `app.emit("run_event", payload)` 事件流 |
| `POST /design/edit` | `design_edit(instruction, selection)` |
| `POST /factory/generate` | `factory_generate(project, lang, options)` |
| `GET /api/v1/runtime/stream`（SSE） | `listen("runtime_update", ...)` |
| `/market/*` | `market_install / market_pack / market_trust ...` |

**前端改动面**：核心集中在 `frontend/src/api.ts`——把 `fetch` 换成 `@tauri-apps/api` 的 `invoke`，把 `EventSource` 换成 `listen`。其余画布组件基本零改动；删除 collab 相关 UI（presence/lease/handoff）。

---

## 5. crate / 目录布局

```
agent-workbench/                # 重写后即 HarnessLab Desktop
  Cargo.toml                    # workspace 根
  crates/
    harnesslab-core/            # 项目模型 / ops / registry / experiment / factory / design / market
      src/
        project/                # model.rs, ops.rs, store.rs(rusqlite)
        library/                # registry.rs, coordinator/, scorer/, stages/
        experiment/             # datasets.rs, evaluator.rs, autoloop.rs
        factory/                # ir.rs, emitters/{python,ts,rust}.rs, templates/
        design/                 # planner.rs (rule + optional LLM)
        market/                 # manifest.rs, client.rs, sign.rs
        loader.rs / config.rs / contrib.rs / ...  # 迁自现 src/
  src-tauri/                    # Tauri 壳：commands/ events/ SQLite/ 窗口·托盘·更新
    src/
      commands/                 # 一文件一组命令，薄封装 core
      events.rs                 # run/runtime 事件桥
      main.rs
    tauri.conf.json
    capabilities/
    icons/
  frontend/                     # 移植自 HarnessLab 的 React 前端
```

复用 kernel 的方式与现状一致：`harnesslab-core` 依赖 `piscis-kernel`（git rev，建议统一升到 `v0.8.47` 或与 openpiscis/AgentZ 对齐的 tag）。

---

## 6. 分阶段路线图

| 阶段 | 交付物 | 验收标准 |
| --- | --- | --- |
| **P0 桌面骨架** ✅ | 照 openpiscis 起 Tauri v2 壳；移植前端跑通空壳；建 workspace 与 `harnesslab-core`，迁入现有 6 个 `.rs`；删 Python | `tauri dev` 打开窗口、前端加载；`cargo build` 通过；仓库无 `.py` |
| **P1 模型 + 注册表 + 只读画布** ✅ | `Project/AgentSpec/Topology`(serde) + 统一 registry catalog；command 供数据，画布只读渲染 + 点选 Inspector | 任意 project 正确可视化；catalog 列出 kernel + 自带算法 |
| **P2 Edit Ops + 双向编辑** ✅ | 事务性 ops 引擎（`ops.rs` 校验/diff/apply，全集 op）+ `rusqlite`(bundled) 存储 + Layout 落库；`apply_ops` 命令带 base_rev 乐观并发、422/409/404 错误负载 | ops/store 单测（合法/非法引用/dry-run/stale rev）；编辑→存→重载一致 |
| **P3 运行台 + 监控** ✅ | 经 kernel 跑 agent（`runner.rs`：Project→AgentConfig 映射 + AgentEvent→`{kind,agent,text,data,ts}` 翻译）；`run_project` 经 Tauri **Channel** 流式回放；节点 live phase 点亮 + 指标叠加；运行落 `canvas_runs` + Session 树。**单 agent 已通；多 agent 协调留作后续** | 画布跑任务、事件实时点亮、指标回填、历史可回看；无密钥优雅报错 |
| **P4 Factory 代码生成** ⬜ | IR + py/ts/rust 发射器；实时代码预览 | 生成 Rust 可编译、TS 可类型检查 |
| **P5 Design Copilot** ⬜ | NL→校验 Edit Ops→diff→apply（规则 + 可选 LLM 规划器） | 指令产出合法 ops；diff 可审阅；非法 ops 被拒 |
| **P6 Market + 打磨** ⬜ | 包安装/打包/签名（ed25519 / HMAC）；自动布局 / 撤销重做 / 模板库 / 键盘流 | 安装本地包、签名校验通过；模板一键起手；undo/redo 正确 |

**建议起点**：P0 + P1（投入小、视觉冲击大），同时把 P2 的 ops 校验地基打好，为 P5 的 AI Copilot 铺路。

---

## 7. 风险与取舍

- **多智能体协调覆盖度**：优先复用 kernel `pool`；缺失的高级 coordinator（coscientist/tournament/debate/map_reduce/hierarchical）需在 core 补齐，是 P3 的主要工作量。
- **代码生成模板移植**：Jinja2 → Rust 模板引擎（`minijinja` 兼容 Jinja 语法，迁移成本最低）；保证三语言产物可编译/类型检查是验收硬指标。
- **去 Python 的测试缺口**：现有 Python 测试需用 Rust `#[test]` 重建；ops 校验、round-trip、codegen 编译是重点回归项。
- **kernel 能力边界**：若某些 HarnessLab `engine` 行为 kernel 未覆盖（如特定 loop_strategy / 事件粒度），需在 kernel `contrib` 或 core 适配层补齐，避免改动 kernel 主体。
- **范围蔓延**：严格按 P0→P6 推进，每阶段独立可交付；bus / collab 明确不在 v1。
- **后续可选回填**：如确需多机/协作，可在桌面内置本地 WebSocket 重新引入 collab，不影响 v1 架构。

---

## 8. 一句话总结

HarnessLab 的“可运行模型 + Palette + Inspector + 运行 + 代码生成 + 市场”能力，
在桌面端由 **Tauri v2 壳 + `harnesslab-core`(Rust) + 复用 `piscis-kernel`** 承载，
前端沿用成熟的 `@xyflow/react` 画布，仅把 REST/SSE 换成 Tauri IPC——
**彻底去 Python，单机单用户优先，分阶段可交付。**
