spec: task
name: "research-report-templates"
inherits: project
tags: [research-cli, report, templates, multi-format, phase-4]
estimate: 1.0d
depends: [research-synthesize, research-session-lifecycle, research-add-source]
---

## 意图

把 session 确立为 **canonical store**,在其上加一层 `research report` 命令,从**同一份** session.md
派生出多种报告视图。v1 只实装 `rich-html` 一种格式 —— 把当前 `bu-harness/report.html`
这份手写的精美 editorial HTML **沉淀为 CLI 模版**,并约束 agent 通过 session.md 自然地
产出这种质量的报告,而不是每次手写 HTML。

这次 task 要解决三个问题:

1. **重复劳动** —— agent 每次写报告都重造壳 (CSS / 字体 / layout)
2. **真相漂移** —— 手写 HTML 和 session.md 失去一致性 (刚才 bu-harness 就是这样:
   session.md 是空模板,report.html 却内容丰富)
3. **单一输出** —— 现有 `synthesize` 只能走 json-ui 一条路。session 里的内容其实可以
   投影成 rich-html / brief-md / slides / json-export 等多种视图

本 task 不引入 LLM 调用,不做内容生成。只做 **模板 + 装配 + 多 format 分发** 的
机械管道。

## 背景

现状数据流:
```
agent writes session.md  →  research synthesize  →  report.json  →  json-ui  →  report.html
                                                   (功能性结构)      (外部渲染)  (结构化报告)
```

json-ui 渲染的 report.html 是**功能性结构**(ContributionList / MetricsGrid / LinkGroup 等
组件化呈现),适合 consume,**不适合 publish**。手写 `bu-harness/report.html` 用的是另一种
风格 —— Instrument Serif + Geist + Geist Mono,带 paper 背景色 + coral editorial accent,内嵌
SVG diagram + aside callout。这种 editorial 风格是**人读的报告**,不是数据驱动仪表盘。

v1 就专门把这种 editorial 风格沉淀成模版。两种 HTML 并存:
- `report.html` ← `synthesize` 产 (json-ui,结构化)
- `report-rich.html` ← `report --format rich-html` 产 (editorial)

以后 `--format brief-md` / `slides-reveal` / `json-export` 走同一个分发点。

## 已定决策

### 命令

```
research report [<slug>] [--format <FMT>] [--open] [--no-open]
```

- 无 slug 读 `.active`
- `--format` 必填 (无默认,强制 agent 显式选择)。v1 支持值:
  - `rich-html` — editorial HTML,embed 模版
  - (其他值 v1 返回 `FORMAT_NOT_IMPLEMENTED`,列在 `supported` 字段里,便于未来扩展)
- `--open` / `--no-open` 行为对齐现有 `synthesize`
  - 非 TTY / `CI=1` / `RESEARCH_NO_OPEN=1` → 忽略 `--open`
- 输出路径按 format 决定:
  - `rich-html` → `<session_dir>/report-rich.html`
  - 未来 brief-md → `report-brief.md`,slides → `report-slides.html`,json-export → `report.json.export.json`
- **不覆盖** `synthesize` 产出的 `report.json` / `report.html`,两者独立文件

### session.md 扩展约定

保持现有 marker (`## Overview`,`## Findings`,`## Sources` block 等),新增三条惯例:

1. **Aside callout**(引言卡片,serif italic):
   ```markdown
   > **aside:** 
   > The bitter lesson — the less you build, the more it works.
   > — Gregor Zunic, 2026-01-16
   ```
   - 识别:blockquote 首行以 `**aside:**` 开头(可容忍空格)
   - 渲染:`<p class="aside">…</p>`,剥掉 `**aside:**` 前缀

2. **SVG diagram inline**:
   ```markdown
   ![Fig · philosophy axis](diagrams/axis.svg)
   ```
   - 路径必须相对 session_dir,且在 `<session>/diagrams/` 子目录内(安全约束,防越界)
   - 文件必须 `.svg`,尺寸 ≤ 512 KB(避免巨图拖垮 HTML)
   - 渲染:把 `<svg>…</svg>` 直接内联进 `<div class="diagram">` wrapper,保留 alt 作为 `<p class="caption">`
   - 找不到 / 太大 / 不是 SVG:warning 事件,降级为 `<img src="…">`

3. **Section 编号约定**:
   - Markdown heading 形如 `## 01 · WHY`(匹配正则 `^\d{1,2}\s+·\s+`)
   - 渲染时把 `01 · ` 提取为 `.section-num` monospace badge(coral 色),剩下作主标题
   - 不匹配的 heading 正常渲染,不做特殊处理

其他 markdown 元素(段落、列表、code block、table、link、inline code)按标准 CommonMark
渲染,套 spec 里定义的 CSS class。

### Sources 段是**事实源,非 markdown 源**

report 生成时 **忽略** session.md 的 `<!-- research:sources-start --> … --end -->` 块内容,
改从 `session.jsonl` 的 `source_accepted` 事件实时构造 Sources section。理由:
- session.md 的 sources 块是 CLI 自动维护的 cache,不是 agent 权威输入
- 从 jsonl 取能带上 kind / timestamp / trust_score / bytes 等元数据,支持更丰富呈现
- 避免双写漂移

渲染结构:每 source 一个 `<li>`,包含:
- `<span class="kind">{kind}</span>` 前缀 badge
- `<a href="{url}">` 可点击链接
- 按 accepted 时间升序排列

### 模版落盘位置

```
packages/research/templates/
  rich-report.html        # include_str! 编译进二进制
  rich-report.README.md   # 模版维护说明(fonts / colors / layout 原则)
```

模版用**最朴素**的 `{{PLACEHOLDER}}` 字符串替换,**不拉 tera / handlebars 这类 crate**。
占位符清单:
- `{{TITLE}}` — session topic
- `{{SUBTITLE}}` — `Session: {slug} · tagged {tags}` + 主 Sources 点击列表(顶部)
- `{{ASIDE_QUOTE}}` — 第一个识别到的 aside block (optional,缺失则整块 `<p class="aside">` 省略)
- `{{BODY_HTML}}` — markdown 渲染结果,不含 sources
- `{{SOURCES_HTML}}` — 从 jsonl 生成的 `<ul>` source 列表
- `{{GENERATED_AT}}` — RFC3339 UTC 时间戳
- `{{SESSION_FOOTER}}` — `Session · {abs_path} · {n} accepted sources · {total_bytes}`

模版本身包含 CSS + font link + frame shell,不含任何业务数据。

### Markdown → HTML 实装选择

用 `pulldown-cmark`(workspace 已间接依赖,零心智负担)。不自己写 parser,
不拉 markdown-it / comrak 这类更重的库。对 `pulldown-cmark` 的 event stream 做一次
pre-pass 识别 aside / diagram / section-num 三种约定,产出增广后的 event,再走
html::push_html 写出。

### 错误契约

| code | 含义 | 退出码 |
|------|------|-------|
| `FORMAT_UNSUPPORTED` | 未知 format 字符串 | 2 |
| `FORMAT_NOT_IMPLEMENTED` | 已声明但 v1 未实装的 format | 2 |
| `SESSION_NOT_FOUND` | slug 不存在 | 2 |
| `MISSING_OVERVIEW` | session.md 没 Overview 段 / 内容为空 | 2 |
| `RENDER_FAILED` | 模版替换 / markdown parse 失败 | 1 |
| `DIAGRAM_OUT_OF_BOUNDS` | SVG 路径越出 session_dir | 2 |

Warnings(不 fatal,进 envelope `meta.warnings`):
- `aside_multiple` — 发现多个 aside block,只用第一个,其他当普通 blockquote
- `diagram_fallback_img` — SVG 文件问题,降级 `<img>`
- `no_sources` — jsonl 里一条 accepted source 都没有,Sources section 显示 "(no sources)"

### 写的事件

- `report_started` `{format, slug}`
- `report_completed` `{format, output_path, bytes, diagrams_inlined, sources_count, duration_ms}`
- `report_failed` `{format, reason, stage}` (stage ∈ `parse_md` | `render_svg` | `write_file`)

### 可重跑

同 slug + 同 format 的第二次 `research report` 直接覆盖输出文件,和 `synthesize`
行为一致。

### Agent 使用契约 (写给 active-research skill 的)

v1 在 `packages/research/templates/rich-report.README.md` 里落一份面向 agent 的"如何写 
session.md 使其渲染为专业报告"文档,涵盖:
- 各段落的期望长度 / 风格
- aside 用什么时候用(**最多一个**,引言性)
- diagrams/ 文件夹怎么组织
- section 编号约定
- 一个完整 example 的 session.md 作为样板

## 边界

### 允许修改

- `research-api-adapter/packages/research/src/commands/report.rs`(新)
- `research-api-adapter/packages/research/src/report/template.rs`(新,模版加载 + 替换)
- `research-api-adapter/packages/research/src/report/markdown.rs`(新,pulldown-cmark 包装)
- `research-api-adapter/packages/research/src/report/diagram.rs`(新,SVG 内联 + 安全校验)
- `research-api-adapter/packages/research/src/report/sources.rs`(新,从 jsonl 生成 source list)
- `research-api-adapter/packages/research/src/cli.rs`(加 `Commands::Report` 分支)
- `research-api-adapter/packages/research/src/session/event.rs`(加 `report_*` 事件 variant)
- `research-api-adapter/packages/research/templates/rich-report.html`(新 asset)
- `research-api-adapter/packages/research/templates/rich-report.README.md`(新 asset)
- `research-api-adapter/packages/research/Cargo.toml`(按需加 pulldown-cmark)
- `research-api-adapter/packages/research/tests/report.rs`(新)

### 禁止做

- **不调 LLM**。模版 + 装配 + 渲染,纯机械。
- **不重构 `synthesize`**。两个命令独立文件、独立输出、独立测试。未来可能统一,v1 不动。
- **不改 session.md 自动块** (sources-start/end 继续用于 `add` 命令维护)。
- **不支持跨 session 引用** (report 只读当前 slug,不跨 parent / series)。series 报告
  是另一个 feature (见 `research-session-series.spec.md`),不在本 task 范围。
- **不加 `--watch` 模式**。改一次 session.md 重跑一次 `research report`,简单。
- **不做 SVG 生成**。agent 负责写 SVG,CLI 只内联。

### 不动的文件

- `synthesize.rs` 及其 tests
- `json-ui` (外部依赖)
- `report/builder.rs` (json-ui 结构装配器,synthesize 专用)

## 验收标准

### 必须通过的测试 (`tests/report.rs`)

1. **happy path rich-html**:
   - 起 session,填 session.md (Overview / 2 条 Findings / 1 个 aside / 1 个 SVG ref)
   - 跑 `research report <slug> --format rich-html --json --no-open`
   - 断言:
     - exit 0
     - `<session>/report-rich.html` 存在且 > 0 bytes
     - 输出 HTML 里含 `<p class="aside">`(1 次,不多不少)
     - 输出 HTML 里含 `<svg ` 开头的 tag (内联成功)
     - 输出 HTML 里含 Sources section 且所有 source URL 都是 `<a href>`
     - envelope `.data.sources_count` 等于 jsonl 里 accepted 数

2. **missing Overview 是 fatal**:
   - session.md 的 Overview 段为空
   - 断言 code `MISSING_OVERVIEW`,exit 2

3. **aside 多个时只用第一个 + warning**:
   - session.md 含 2 个 `> **aside:**` blockquote
   - 断言 HTML 里 `<p class="aside">` 仅 1 次,envelope `meta.warnings` 含 `aside_multiple`

4. **diagram 越界路径被拒**:
   - session.md 里写 `![](../../../etc/passwd.svg)`
   - 断言 code `DIAGRAM_OUT_OF_BOUNDS`

5. **diagram 缺失降级 `<img>`**:
   - session.md 里引用 `diagrams/missing.svg`,但文件不存在
   - 断言 exit 0,HTML 含 `<img src="diagrams/missing.svg">`,`meta.warnings` 含 `diagram_fallback_img`

6. **section 编号样式**:
   - session.md 里有 `## 01 · WHY`
   - HTML 含 `<span class="section-num">01</span>`(或等价 class),主标题里不再含 `01 · `

7. **sources 来自 jsonl 不是 md**:
   - 人为改 session.md 的 sources-start/end 块删掉一条 URL
   - session.jsonl 保持 3 条 accepted
   - 断言 HTML 的 `<ul>` 里还是 3 条

8. **未实装 format 报错 `FORMAT_NOT_IMPLEMENTED`**:
   - `research report <slug> --format slides --json`
   - exit 2,error code `FORMAT_NOT_IMPLEMENTED`,error details `supported` 含 `rich-html`

9. **可重跑**:
   - 连跑两次同一命令,都 exit 0,第二次覆盖第一次的文件

### 必须的集成证据

- 用本 task 完成后的 CLI 重新渲染 `bu-harness` session (先把我手写的 body 回写进去 session.md,
  再跑 `research report bu-harness --format rich-html`),产出的 `report-rich.html` 肉眼比对
  应当接近当前手写版的质量。**这是 eat-your-own-dog-food 的强验证**,spec 要求 PR 附上
  对比截图。

## Out of scope (v1 不做,未来 task)

- `--format brief-md` / `slides-reveal` / `json-export` 的具体实装
- 多 session 合并报告 (series 报告)
- PDF 输出 (走 chromium headless 或 wkhtmltopdf)
- 主题变体 (默认 stone+rust 之外的 dark / 学术风)
- 模版热更新 (不用文件系统模版,坚持 `include_str!`)
- RSS / email 投递

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 模版 CSS 与 bu-harness 报告漂移 | 把 bu-harness/report.html 作为 "golden" fixture,CI 跑 diff 相似度检查 |
| pulldown-cmark 对扩展语法不够灵活 | pre-pass event-stream 识别三种约定,不改 parser |
| agent 不知道约定 | rich-report.README.md + 在 `research report --help` 里引导到 spec |
| SVG 安全(XSS via onclick 等) | 不过滤 — 模版明确 "SVG 是 agent 写的,等同代码审计",不做 sanitize。 session 目录是 local 可信边界 |
| template 文件 diff 造成大 PR | 模版单独 commit,独立 review |

## 实装提示 (非强制)

- 模版占位符替换用 `str::replace`,5 个占位符各跑一次,不引模版引擎
- markdown event stream pre-pass 用 `pulldown_cmark::Parser` + 手动 mapping
- SVG 内联前用 `std::fs::metadata` 查 size,`Path::canonicalize` + `starts_with(session_dir)` 查越界
- Sources 从 jsonl 拉时用现有的 `session::events::iter_accepted(slug)` helper (如已有)
- 输出 HTML 前做 `<script>` 纯字面量出现次数断言(防模版文件被意外污染)

## 待决问题 (PR 前确认)

1. 是否让 `synthesize` 和 `report` 共享 Overview / Findings 解析逻辑?(倾向:抽到
   `session::md_parser`,两端都调)
2. `rich-report.README.md` 的 example session.md 要不要就用 `bu-harness` 反向填充后的
   那份?(倾向:是,既作 example 又作 golden test fixture)
3. `--no-color` 对 rich-html 无意义,但为了和其他命令 flag 保持一致,保留还是拒绝?
   (倾向:保留但无行为)
