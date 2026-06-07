# HarnessLab Market Registry Protocol (v1)

本文档定义 **桌面客户端** 与 **未来云端市场** 之间的契约。包格式仍以
[`HarnessLab Package Manifest v1`](../../HarnessLab/docs/spec/package-manifest.md)
为准；本协议只规范 **索引（catalog）** 与 **源解析（source spec）**。

设计原则：

- **去中心化优先**：任意人可在 git / HTTPS 上托管 tarball，无需中央服务器。
- **索引可选**：云端市场 = 托管一份 **静态 `index.json`** + 包 tarball；客户端按需拉取。
- **信任在包上**：索引仅作发现层；安装时仍强制 manifest integrity，可选 ed25519 验签。
- **向前兼容**：`schema_version` / `protocol_version` 升级时旧客户端可明确拒绝。

---

## 1. Source spec（源规格）

客户端 `sources::fetch` 必须支持以下形式（与 Python HarnessLab 对齐）：

| 形式 | 含义 | 示例 |
|------|------|------|
| `file:<path>` 或裸路径 | 本地目录（含 `harness-package.yaml`） | `file:./packages/cool-loop` |
| `https://…/pkg.tar.gz` | 下载 gzip tarball 并解压 | `https://cdn.example.com/cool-loop-0.1.0.tar.gz` |
| `git+<url>#<ref>` | 浅克隆 git 仓库（可选 ref/tag） | `git+https://github.com/org/pkg.git#v0.1.0` |
| `registry:<name>@<version>` | 经配置的索引解析为上述之一 | `registry:cool-loop@0.1.0` |

规则：

- tarball URL **必须**以 `.tar.gz` 或 `.tgz` 结尾。
- `registry:` 解析失败时返回明确错误（未配置索引 / 包不存在 / 版本 yanked）。
- 解析结果为本地目录后，走统一的 manifest → integrity → signature → install 流程。

---

## 2. Registry index（索引文件）

### 2.1 托管方式

云端市场 **无需动态 API**。最低要求：

1. 在 HTTPS 上托管 `index.json`（或带版本的路径，如 `/v1/index.json`）。
2. 每个包版本对应一个 **gzip tarball**，URL 写入索引。
3. （推荐） tarball 内 `harness-package.yaml` 的 `integrity` 字段已填好。

客户端通过 **registries 配置**（见 §4）拉取一个或多个索引 URL，合并展示 catalog。

### 2.2 JSON Schema（`schema_version: "1"`）

```json
{
  "schema_version": "1",
  "registry": "harnesslab-official",
  "description": "Official HarnessLab community packages",
  "updated_at": "2026-06-07T12:00:00Z",
  "homepage": "https://market.harnesslab.example",
  "packages": {
    "cool-loop": {
      "latest": "0.1.0",
      "type": "algorithm",
      "category": "loop",
      "description": "A community loop strategy.",
      "keywords": ["loop", "example"],
      "authors": ["HarnessLab Team"],
      "license": "MIT",
      "versions": {
        "0.1.0": "https://cdn.example.com/packages/cool-loop-0.1.0.tar.gz",
        "0.2.0": {
          "source": "https://cdn.example.com/packages/cool-loop-0.2.0.tar.gz",
          "published_at": "2026-06-01T00:00:00Z",
          "yanked": false
        }
      }
    }
  }
}
```

字段说明：

| 字段 | 必填 | 说明 |
|------|------|------|
| `schema_version` | 是 | 固定 `"1"` |
| `registry` | 是 | 索引唯一标识（slug，kebab-case） |
| `description` | 否 | 人类可读说明 |
| `updated_at` | 否 | ISO-8601，索引最后更新时间 |
| `homepage` | 否 | 市场首页 |
| `packages` | 是 | 包名 → 条目 |

**Package 条目**：

| 字段 | 必填 | 说明 |
|------|------|------|
| `latest` | 推荐 | 默认安装版本（semver） |
| `type` | 推荐 | `algorithm` \| `dataset` |
| `category` | 推荐 | algorithm 的 registry kind 或 `dataset` |
| `description` | 否 | 列表展示用 |
| `keywords` | 否 | 搜索标签 |
| `authors` / `license` | 否 | 元数据 |
| `versions` | 是 | 版本 → source spec 或版本对象 |

**版本值** 两种写法（客户端均须支持）：

1. **字符串** — 直接 source spec：`"0.1.0": "https://…/pkg.tar.gz"`
2. **对象** — 扩展元数据：
   ```json
   {
     "source": "https://…/pkg.tar.gz",
     "published_at": "2026-06-01T00:00:00Z",
     "yanked": false,
     "changelog": "Fix scorer wiring"
   }
   ```
   - `yanked: true` 的包 **不得**被 `registry:name@version` 或 `latest` 解析安装。

### 2.3 索引 URL

索引本身可通过：

- `https://market.example.com/index.json`
- `file:/path/to/index.json`（开发 / 离线）

客户端拉取时使用 `GET`，`Content-Type: application/json`，超时建议 ≥ 60s。

### 2.4 多索引合并

客户端配置多个 registry URL 时：

- catalog 中每条记录带 `registry` 字段（来自 index 的 `registry` 字段）。
- 同名包来自不同 registry 时 **不合并**，以 `(registry, name)` 为唯一键。
- 安装 spec 使用 `registry:<name>@<version>`；客户端按 **配置顺序** 查询索引，**先命中者优先**。

---

## 3. 客户端安装流程（不变）

索引只参与 **第 0 步：解析 source**；之后与 decentralized 安装一致：

1. **Resolve** — `fetch(source)` → 本地含 manifest 的目录。
2. **Validate** — manifest 语义校验 + `protocol_version == "1"`。
3. **Integrity** — 重算 `integrity` 中各文件 sha256（mandatory）。
4. **Signature** — 可选；用户开启「要求签名」时拒绝 unsigned/invalid。
5. **Capability consent** — `capabilities` 非空时需用户确认。
6. **Cache** — `{app_data}/market/packages/<name>/<version>/`。
7. **Wire** — algorithm → plugins；dataset → datasets。
8. **Lock** — `harness.lock` 记录 source + integrity。

下载的 tarball 可缓存在 `{app_data}/market/cache/<sha256(url)>/` 以避免重复拉取。

---

## 4. 客户端配置

路径：`{app_data}/market/registries.json`

```json
{
  "registries": [
    "https://market.harnesslab.example/v1/index.json",
    "file:/opt/dev/registry/index.json"
  ]
}
```

- 首次启动可 seed 内置示例索引（仅开发构建）。
- UI 提供 registries 编辑与 catalog 浏览。
- 环境变量 `HARNESSLAB_REGISTRY`（逗号分隔）在桌面端 **可选** 覆盖/追加，便于 CI。

---

## 5. 云端市场实现清单（未来）

按本协议实现云端时，需交付：

| 交付物 | 说明 |
|--------|------|
| `index.json` | 符合 §2.2，HTTPS 托管，建议 CDN + `Cache-Control` |
| `<name>-<version>.tar.gz` | 根目录含 `harness-package.yaml`；`pack` 填好 integrity |
| （推荐）发布流水线 | tag → 构建 tarball → 更新 index → 部署 |
| （可选）索引签名 | v1.1：对 index JSON 做 ed25519 签名，客户端 pin 市场公钥 |

**不需要**：上传 API、用户账号、动态搜索（v1 客户端本地 filter 即可）。

### 5.1 发布 tarball 约定

- 文件名：`<name>-<version>.tar.gz`
- 解压后顶层即为包根（含 `harness-package.yaml`），不要额外嵌套一层目录。
- `integrity` 覆盖 manifest 引用的所有 artifact。

### 5.2 示例索引

仓库内示例：[`crates/harnesslab-core/examples/registry/index.json`](../crates/harnesslab-core/examples/registry/index.json)

---

## 6. 错误码（客户端 → UI）

| 场景 | 建议错误前缀 |
|------|----------------|
| 网络失败 | `fetch_failed:` |
| 索引 JSON 无效 | `invalid_index:` |
| registry 未配置 | `registry_not_configured:` |
| 包/版本不存在 | `package_not_found:` |
| 版本已 yanked | `version_yanked:` |
| integrity 失败 | `integrity:` |
| 签名失败 | `signature:` |
| 需 capability 同意 | `needs_consent:` |

---

## 7. 版本演进

| 版本 | 变更 |
|------|------|
| v1（本文） | index.json + source spec + 客户端 catalog |
| v1.1（计划） | 索引 ed25519 签名、`registry:<name>` 省略 version 时用 latest |
| v2（计划） | 包 manifest `schema_version` 升级时同步 bump |

---

## 8. 参考实现

桌面端 Rust 模块：

- `crates/harnesslab-core/src/market/registry.rs` — 索引解析与 catalog
- `crates/harnesslab-core/src/market/sources.rs` — source spec fetch
- `crates/harnesslab-core/src/market/client.rs` — install / lockfile

Tauri 命令：`market_catalog`、`market_get_registries`、`market_set_registries`、`market_install`（支持 `registry:…`）。
