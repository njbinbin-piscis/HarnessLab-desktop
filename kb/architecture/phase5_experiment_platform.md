# Phase 5: 完整可插拔 / 可调参 / 可观察实验平台

> 目标：在已完成的"多层解耦 harness 运行时"之上，补齐两块——**AI 实验设计助手**
> （本身是一个可被设计的 agent，能为研究者开发新的 harness 组件并以可插拔方式创建
> 新 agent/设计实验）与**验证集管理 + 执行观测**，使"设计 → 运行 → 评测 → 迭代"
> 全程可插拔、可调参、可观察，并支持自动自举闭环。

## 1. 现状盘点

| 支柱 | 现状 | 评价 |
|------|------|------|
| ① 运行时（harness） | piscis-kernel 可插拔压缩/记忆/上下文/LoopStrategy/工具/多 Agent；workbench `validate`/`run`/`exec`（含交互模式） | ✅ 已完成 |
| ② AI 设计助手 | `meta_generator.py` 纯规则模板；`api/meta_agent.py` 暴露 generate/refine/templates；`rust_validator.py` 可 validate + dry-run | ⚠️ 非真 agent，不能造组件、自身不可配置 |
| ③ 验证集 + 观测 | `benchmarks/`（datasets/scorers/studies/runner/plot）仅 CLI；`backend/runner.py` 单次内存态执行 + SSE | ⚠️ 双轨割裂、无持久化、无对比/看板 |

## 2. 设计原则

- **可插拔**：所有可替换部件（压缩/记忆/上下文/loop 策略/coordinator/scorer/数据集）
  经统一注册表登记，按名称解析；新增组件不改核心 `match`。
- **可调参**：每个组件暴露 JSON 参数 schema，配置层填参，支持 sweep。
- **可观察**：执行轨迹与指标结构化持久化（非仅流式打印），可事后回放与跨运行对比。
- **同构自举**：设计助手是跑在运行时上的 agent；它产出的组件/agent 也跑在同一运行时，
  由验证集层打分并回流，形成"AI 改进 AI"的闭环。
- **行为保持**：默认路径不变；新增能力以可选注册/可选字段引入。

## 3. 分阶段实施

### 阶段 A — 组件注册表 + 参数 schema（地基）
- Python：`backend/core/registry.py` 统一注册表（kind: coordinator/scorer/memory/
  context/loop/component），每条目带 `name/kind/params_schema/factory/source`。
  收编现有 `swarm.load_coordinator`、`benchmarks.SCORERS`。
- Rust：kernel 新增 `contrib` 运行时注册钩子，`resolve_compaction_strategy` /
  `resolve_loop_strategy` 在内置 `match` 未命中时 fallthrough 到 contrib 注册表，
  使设计助手生成的命名 Rust 组件可被 YAML 直接引用。
- API：`GET /api/v1/registry` 列出全部可插拔组件及其参数 schema。

### 阶段 B — 验证集管理 + 持久化评测运行
- `backend/core/datasets.py`：DatasetRegistry 复用 `benchmarks.datasets` + `scorers`，
  支持内置 smoke set 与用户上传 JSONL（`{task_id,prompt,answer,metadata}`）。
- `backend/core/store.py`：SQLite 运行存储（eval_run / task_result / trajectory / metric）。
- `backend/core/evaluator.py`：agent×数据集评测运行，复用 `benchmarks.runner.run_arm`，
  逐任务持久化轨迹+得分，聚合 accuracy/pass^k/tokens/latency。
- API：`/api/v1/datasets`（列表/任务/上传）、`/api/v1/eval-runs`（启动/SSE 进度/结果/
  列表/对比）。

### 阶段 C — 观测看板（前端，最小可用）
- 数据集视图、评测运行列表、运行详情（每任务表 + 轨迹回放）、arm 对比图表。

### 阶段 D — 设计助手 = 真 agent（支柱二核心）
- `configs/meta_agent.yaml`："Architect" persona，自身可配置（提示词/压缩/记忆/工具）。
- 组件创作工具集：写文件、`cargo build`、跑回归、注册组件、dry-run 配置
  （扩展 `rust_validator.py`，新增 `component_builder.py`）。
- agent 驱动的设计会话（走 `InteractiveSession`），规则模板降级为 dry/兜底。
- API：`/api/v1/design`（会话式生成/精修/造组件）。

### 阶段 E — 自举闭环（支柱二 + 三）
- `backend/core/autoloop.py`：设计 → 评测运行 → 读分 → 修订 → 迭代 N 轮，记录血缘
  （每版 config/component + 指标），择优收敛；可选"自改 meta_agent.yaml + 回归门禁"。
- API：`/api/v1/autoloop`（启动/进度/血缘）。

## 4. 验证策略
每阶段后跑 `pytest`（Python）与 `cargo test`（Rust，含 contrib 注册）；
重点回归默认配置 behavior-preserving；evaluator/autoloop 用 FakeSession 做 dry 测试。
