# research-api-adapter — DESIGN

更新时间：2026-04-15

> **项目定位**：把 `postagent` 作为 API-first source adapter 接入 `active-research` 研究流程。不新起研究产品线，不做新 CLI，只扩展现有 skill 的源路由能力。
>
> 这个 repo 只承载：设计文档（本文件）、task contracts（`specs/`）、跨项目协调工作的追踪。实际代码改动落在上游的 `postagent` 和 `active-research` skill 文件里。

---

## 裁定结论（已拍板）

1. **不新起 writing-assistant 产品线**。用户入口只保留 `active-research` / `deep-research`。
2. **新增一层 API-first source adapter**，让 `active-research` 在遇到结构化源（Reddit / GitHub / arXiv 等）时走 `postagent`，而不是走浏览器。
3. **以当前 `packages/cli/src/cli.rs` 真实命令面为准**。skill 文档里提过但 CLI 里不存在的命令（`browser fetch` / `--mode readability` / `browser batch` / `wait-idle`），**不作为可依赖前提**写进计划。
4. **修改 `postagent send` 支持匿名请求**。不要让 orchestrator 旁路 postagent 再造一个 HTTP client。

---

## 三层边界

```
┌─────────────────────────────────────────────────────┐
│        active-research (orchestrator)              │
│  Claude skill: 规划 → 调度 → 综合 → 输出 HTML/JSON  │
└─────────────────────────────────────────────────────┘
           ↓                          ↓
    ┌──────────────┐            ┌──────────────┐
    │  postagent   │            │  actionbook  │
    │ (API adapter)│            │(browser adpt)│
    └──────────────┘            └──────────────┘
  结构化源 (Reddit / GitHub     长尾源 (博客、登录内容、
  / arXiv / Hacker News /       SPA、政务/学术站点)
  Tavily / Exa / Brave ...)
```

三层职责：

| 层 | 职责 | 不做什么 |
|---|---|---|
| active-research | 研究编排、LLM 综合、报告生成、json-ui 输出 | 不做 HTTP 抓取的具体实现 |
| postagent | 有 API 的源（含匿名公开 API） | 不做 SPA、不做登录 UI |
| actionbook browser | 无 API 或 UI-only 的源 | 不做"只为拿 JSON 而启动浏览器"的浪费 |

---

## 阻塞依赖（必须先解决）

这三个在 orchestrator 层写任何代码之前必须定掉。

### D1 — `postagent send --anonymous` 支持匿名请求

**现状**：`postagent-core/src/commands/send.rs:18-26` 硬校验命令里必须出现 `$POSTAGENT.<SITE>.API_KEY` 占位符，否则 `exit(1)`。

**需要**：加一个 `--anonymous` 或类似开关，跳过 token 校验。对公开 API（arXiv、未登录 Reddit `.json`、匿名 GitHub 搜索）放行。

**安全考量**：占位符机制是为了防 token 泄漏到 LLM 上下文。对**无 token 的公开 API**不存在这个风险，不应被这条规则误伤。默认仍要求占位符，`--anonymous` 是显式 opt-in。

**落地位置**：`postagent/packages/postagent-core/src/commands/send.rs`，加一个 CLI flag 和分支，`contains_token_template` 检查改为"非 anonymous 时强制"。

**验收**：

```bash
postagent send --anonymous "http://export.arxiv.org/api/query?search_query=ti:rust"
postagent send --anonymous "https://api.github.com/search/repositories?q=rust"
postagent send --anonymous "https://www.reddit.com/r/rust/top.json?t=week"
```

三条都能返回正常 HTTP 响应。

---

### D2 — Readability 风格内容抽取的处理方式

**现状**：

- 前一版计划把 `get_readable_text` / `text --mode readability` / `browser fetch` 当成现成 CLI 能力，实际都不存在于 `packages/cli/src/cli.rs`
- 老 `packages/actionbook-rs` 里有 I1/I4 feature，但源码已经迁走，`actionbook-rs/` 现在只剩 `benches/ python/ skills/ target/`
- `packages/cli` 的 `browser text` 只返回 `innerText`，没有 Readability 能力

**决策分支**：

| 方案 | 在哪里做 | 成本 | 选它的条件 |
|---|---|---|---|
| A：orchestrator 本地做 | active-research skill 用 `browser html` 拿原 HTML，在本地跑 Readability | 一次性中等 | 想先跑通，不想扩 CLI |
| B：把 readability 补回 `packages/cli` | 新增 `browser readable` 或 `text --readable` | 中等，但要测试/review/发版 | 认为这是 CLI 的长期职责 |
| C：什么都不做，只用 `browser text` 拿 innerText | 零改动 | 接受更脏的上下文 | MVP 能容忍 |

**我的倾向**：**先 C，后 A**。

- C 可以让 MVP 立刻跑起来，innerText 对大多数博客和文档已经够用
- 等真实数据证明 innerText 不够干净（比如噪声压过信号），再做 A
- B 在这个 plan 里不做。CLI 恢复旧能力属于独立决策，不应该被一个编排层计划绑定

**验收**：C 不需要验收，A 需要 orchestrator 能把 `browser html` 输出转成 markdown（任意轻量 extractor 都行）。

---

### D3 — `active-research` SKILL.md 本身和当前 CLI 对齐

**现状**：`skills/active-research/SKILL.md` 大量引用：

- `browser fetch <url> --format text --json`（不存在）
- `browser wait-idle`（应该是 `browser wait network-idle`）
- `browser batch`（不存在）
- `--auto-dismiss-dialogs --no-animations --rewrite-urls`（这些是 global flag，需要逐个核对是否还存在）

**需要**：按当前 `packages/cli` 实际命令面，修订 `active-research` SKILL.md 的"Navigation Pattern"和"Complete Workflow"两个 section。

**理由**：我们这个计划的 integration point 就在这个 skill 里。如果 skill 本身建立在过时接口上，API adapter 接进来也跑不起来。

**落地位置**：`/Users/zhangalex/.claude/skills/active-research/SKILL.md`。

**验收**：skill 里出现的每一条 `actionbook browser ...` 命令，都能在当前 `packages/cli` `--help` 里找到对应 subcommand。

---

## 当前 CLI 真实命令面（只用这些）

以下命令已通读 `packages/cli/src/cli.rs` 的 `BrowserCommands` enum 确认存在：

### Actionbook Browser（研究阶段会用到的子集）

```bash
# Session
actionbook browser start [--session <id>] [其它 flag]
actionbook browser close --session <id>

# Tab / Navigate
actionbook browser new-tab <url> --session <id>     # alias: open
actionbook browser goto <url> --session <id> --tab <id>

# Wait (注意是子命令，不是 wait-idle)
actionbook browser wait network-idle --session <id> --tab <id> [--timeout <ms>]
actionbook browser wait condition "<js>" --session <id> --tab <id>
actionbook browser wait element "<selector>" --session <id> --tab <id>

# Read
actionbook browser text [selector] --session <id> --tab <id>
actionbook browser html [selector] --session <id> --tab <id>
actionbook browser snapshot --session <id> --tab <id> [--filter interactive] [--max-tokens N]
actionbook browser screenshot <path> --session <id> --tab <id>

# Interact
actionbook browser click <selector> --session <id> --tab <id>
actionbook browser fill <selector> <text> --session <id> --tab <id>
actionbook browser press <key> --session <id> --tab <id>
actionbook browser scroll [...] --session <id> --tab <id>
actionbook browser eval "<js>" --session <id> --tab <id>
```

### Actionbook Action Library（查选择器）

```bash
actionbook search "<keywords>" [-d <domain>]
actionbook get "<area_id>"
```

### Postagent（API 适配层）

```bash
# 需要 auth 的源（现存能力）
postagent auth <site>
postagent send <url> -X <method> -H '... $POSTAGENT.<SITE>.API_KEY ...' -d '<body>'

# 匿名源（依赖 D1 落地后）
postagent send --anonymous <url>
postagent send --anonymous <url> -X GET

# 搜索 / 文档浏览（现存能力）
postagent search "<query>"
postagent manual <site> [group] [action]
```

**`packages/cli` 里不存在、本计划不使用的命令**：

- `browser fetch`
- `browser batch`
- `browser wait-idle`（正确写法是 `browser wait network-idle`）
- `browser text --mode readability` / `get_readable_text`

---

## Source Routing 决策表

orchestrator（active-research skill）按此表选择执行器。

| URL 模式 | 执行器 | 具体命令 |
|---|---|---|
| `reddit.com/r/*/comments/*` | postagent | `postagent send --anonymous "https://www.reddit.com/r/<sub>/comments/<id>.json"` |
| `reddit.com` 搜索 | postagent | `postagent send --anonymous "https://www.reddit.com/search.json?q=<q>"` |
| `api.github.com/*` | postagent | 有 token 用占位符，无 token 用 `--anonymous`（60 req/h 限额） |
| `github.com/{owner}/{repo}` | postagent | → 转成 `api.github.com/repos/{owner}/{repo}/readme` |
| `github.com/*/issues/*` / `pull/*` | postagent | → `api.github.com/repos/{o}/{r}/issues/{n}` + `/comments` |
| `arxiv.org/abs/*` | postagent (anonymous) | `http://export.arxiv.org/api/query?id_list=<id>` |
| arXiv 关键词搜索 | postagent (anonymous) | `http://export.arxiv.org/api/query?search_query=...` |
| `news.ycombinator.com/item?id=*` | postagent (anonymous) | `https://hacker-news.firebaseio.com/v0/item/<id>.json` |
| Tavily / Exa / Brave / Serper | postagent (token) | 用户自备 key，走占位符 |
| 普通博客 / Medium / Substack / 官方文档 | actionbook browser | `new-tab → wait network-idle → text` |
| 登录后内容 / SPA / 有人机验证 | actionbook browser | 同上，可加 cookie 注入 |
| Google 搜索兜底 | actionbook browser | `new-tab google.com/search?q=... → wait network-idle → text` |

**"结构化源 vs 长尾源" 的实际比例**：按前期研究经验估算，结构化源能覆盖高权威内容的 ~60-70%；长尾源虽占 URL 数量的一半以上，但信息密度更低。所以 postagent 承担 80% 的**有效数据量**并不夸张。

---

## Integration 工作分解

### W1 — `postagent send --anonymous`（postagent 侧）

- 改 `postagent-core/src/commands/send.rs`
- 加 CLI flag
- `contains_token_template` 检查门禁化
- 跑 D1 三条验收命令
- 发新版 postagent

### W2 — `active-research` SKILL.md 命令面对齐

- 在 `~/.claude/skills/active-research/SKILL.md` 里把 `browser fetch` / `wait-idle` / `batch` 改成当前 CLI 可用的形态
- 明确 "interactive multi-step pattern" 是现在的主路径（因为 fetch 没有）
- 补一段 "当前 CLI 无 readability，使用 `browser text` 原始 innerText" 的提示

### W3 — `active-research` SKILL.md 新增 "API-first sources" section

主要是 prompt engineering + routing 指引，不是新代码。

结构：

```markdown
## API-First Sources (via postagent)

Before opening a URL in actionbook browser, check if it matches an API-accessible source:

| Domain | Route | Command |
| ... (Source Routing 决策表的 skill 版本) |

### Decision rule
1. If URL matches an entry in the API source table → use postagent
2. Else → use actionbook browser (existing Navigation Pattern)

### Recipes
(每个 API 源一个小段落，含示例命令和预期 JSON 结构)
```

重点 recipes：

- Reddit thread + comments tree（`.json` 后缀路径）
- GitHub repo README / issue / PR 详情 / 搜索
- arXiv 关键词搜索 + 单篇 metadata
- Tavily / Brave / Exa 搜索（需用户 token）

### W4 — Topic Detection 扩展

`active-research` 当前的 Topic Detection 表主要识别 `arxiv:` / `doi:` / URL / 通用文本。

扩展：

- 识别"主题里包含明显 reddit / github 讨论信号"→ 优先走 postagent 的对应搜索
- 识别"主题里包含论文信号" → 继续走 arXiv Advanced Search（现有），但优先用 postagent API 的 `http://export.arxiv.org/api/query` 而不是浏览器表单

### W5 — 报告层不动

`json-ui` 输出、session 目录、命令入口（`/active-research` / `/deep-research`）**完全不改**。用户心智保持不变。

---

## 分阶段路线

### Phase 0 — 依赖项解决（1 周）

并行三条：

- W1：`postagent send --anonymous`
- W2：`active-research` SKILL.md 命令面对齐当前 CLI
- D2 决策：先选 C（只用 innerText），把 A 放后面

**验收**：

- `postagent send --anonymous` 三条命令返回正常响应
- `active-research` skill 里的每一条 `actionbook browser ...` 命令都能在 `packages/cli` `--help` 里找到

---

### Phase 1 — MVP：三个 API 源接入（1 周）

在 `active-research` SKILL.md 里加 "API-First Sources" section，含三个 recipe：

1. **Reddit**：匿名抓 thread + 评论树（`.json` 后缀路径）
2. **GitHub**：匿名抓 repo README / issue 详情 / 搜索 API
3. **arXiv**：匿名 API 查询 + 单篇 metadata

Orchestrator 的决策规则：

- URL 命中 API 源 → 走 postagent
- 其它 → 走 actionbook browser（现有 Navigation Pattern）

**验收**：跑 `/active-research "Rust async runtime 2026 comparison"`，能观察到：

- Reddit 讨论走了 postagent（从日志里看到 `postagent send` 而非 `actionbook browser`）
- 至少有 1 个 arXiv 论文 metadata 走了 postagent
- 其余博客、官方文档仍走 actionbook browser
- 最终报告正常产出（不破坏现有路径）

---

### Phase 2 — 扩展源 + Tavily/Brave 接入（1 周）

加入：

- Hacker News Firebase API（匿名）
- Tavily / Exa / Brave 搜索 API（用户自备 key，走占位符）
- GitHub 带 token 的搜索（跳过 60 req/h 限额）

**验收**：同 Phase 1，但覆盖面更广，Search Strategy 的 5-8 个 query 里有 ≥2 个走 API 搜索而不是浏览器 Google。

---

### Phase 3 — Readability 决策复审（可选）

跑若干真实任务，观察 `browser text` innerText 的噪声是否真的压过信号。

- 如果影响报告质量 → 上马 D2 方案 A（orchestrator 本地做 readability）
- 如果可接受 → 关闭该项，不再做

---

## MVP 的最小承诺

两周内：

- `postagent send --anonymous` 发版
- `active-research` skill 更新到命令面对齐 + 3 个 API 源接入
- 跑一个真实研究主题，观察日志确认结构化源走了 postagent

**不承诺**：

- 不做新 CLI
- 不做新 session 目录约定
- 不改 `json-ui` 报告格式
- 不做 verify / claims / 交叉验证（那是前一版计划的内容，本版不做）
- 不做 writing-assistant 命名

---

## 明确不做

1. **不做新用户入口**。`/active-research` 和 `/deep-research` 是唯一入口。
2. **不做新 session 目录**。沿用 `active-research` 现有的 output 约定。
3. **不做 claim / verify / plan 四阶段流水线**。前一版的 Verify/Plan 阶段在本版里被砍掉，未来如果真的出现"独立写作辅助"用户心智再单独立项。
4. **不做 Rust 代码编排**。本版的 integration 主要在 skill 的 prompt 层 + postagent 的一个小 flag。
5. **不把 `browser fetch` / `readability` 补回 CLI**。这是 CLI 侧的独立决策，不被本计划绑定。
6. **不做凭证互通**。Actionbook ↔ Postagent 的凭证桥在另一份文档（`.docs/actionbook-x-postagent-integration-ideas.md`）里跟踪，本计划不依赖它。

---

## 参考依据

### 当前 CLI 真实命令面

- `packages/cli/src/cli.rs:41-245` — `Commands` 和 `BrowserCommands` 完整 enum
- `packages/cli/src/browser/tab/open.rs:11-41` — `new-tab` / `open` 需要 `--session`，auto-assign tab_id
- `packages/cli/src/browser/wait/network_idle.rs:17-33` — `wait network-idle` 签名
- `packages/cli/src/browser/observation/text.rs:22-33` — `text` 签名（无 readability 模式）

### Postagent 现状

- `postagent/packages/postagent-core/src/commands/send.rs:18-26` — token 占位符硬校验
- `postagent/packages/postagent-core/src/token.rs:15-30` — 凭证存 `~/.postagent/profiles/default/<site>/auth.yaml`
- `postagent/packages/postagent-core/src/cli.rs:78-113` — `Config / Auth / Send` 命令定义

### 现有研究能力

- `playground/deep-research/README.md` — `/deep-research` 命令 + json-ui 报告工作流
- `/Users/zhangalex/.claude/skills/active-research/SKILL.md` — `/active-research` skill，需要做 W2 对齐

### 相关设计文档

- `.docs/actionbook-x-postagent-integration-ideas.md` — 凭证互通等长线整合方向
- `.docs/actionbook-x-lighthouse-synergy-analysis.md` — 另一个产品方向探索

---

## 一句话判断

不要造第二个 `active-research`。把 postagent 当成 `active-research` 的一个新 fetcher 接进来，只加一层路由规则，就够拿到 80% 的价值。剩下的 20% 是命令面对齐和匿名请求支持，属于工程卫生，不属于产品创新。

先把 postagent 的匿名请求做了，再改 skill，剩下的自然水到渠成。
