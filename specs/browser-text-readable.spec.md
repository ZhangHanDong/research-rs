spec: task
name: "browser-text-readable"
inherits: project
tags: [actionbook, cli, text-extraction, phase-2]
estimate: 1d
---

## 意图

`actionbook browser text` 当前返回 `document.body.innerText`，对博客/文档页而言会把
导航、footer、cookie banner、侧边栏等噪声与正文一起吐出来。Phase 1 的 Rust async
concurrency deep-research 里读 5 篇博客，平均每篇只有约 40% 的内容是正文，其余是
噪声——这是 active-research 工作流最大的单项 token 成本项。

本任务为 `browser text` 增加 `--readable` 开关：当设置时，CLI 从页面抓 `outerHTML`，
在本地用 Readability 算法抽取文章主体（典型实现：移除 nav/footer/aside，
基于文本密度打分选出 main content），然后返回抽取后的 plain text。目的是让
active-research 及其它研究工作流能在一条命令内拿到 "只含正文" 的输出，而不必
用户自己写 HTML 后处理。

## 已定决策

- 新增 CLI flag：`--readable`，仅在 `browser text` 子命令上
- 语义：不带 `--readable` 时，行为完全不变（innerText）；带 `--readable` 时走 Readability 抽取路径
- 输入来源：`Runtime.evaluate("document.documentElement.outerHTML")`（不是 body.innerText）
- 抽取实现：由 implementer 在 `readability`（crates.io 上 Mozilla Readability 的 Rust 端口）或同类成熟 crate 中选一个；commit 信息里说明选型理由
- 输出格式：plain text（段落间空行保留；不做 markdown 转换）
- HTML → text 转换：implementer 挑一个 `html2text` 或等价的 Rust crate；或内联用 `scraper` 做遍历（若抽取出的 HTML 已经很干净）
- 不做 feature-gating：readability + html2text 作为强制 runtime deps 加入。如果未来 `cargo bloat` 显示 binary 增长超过 3 MB，再考虑 feature 化
- `--readable` 与 `selector` 参数不兼容：二者同时传入 → 早期报错 `INVALID_ARGUMENT`，并提示用户"readability 只在全文档模式可用"
- Fallback：抽取出的 plain text 若 < 100 字符（表明 Readability 打分失败或页面无主体），自动退回 `document.body.innerText`，并向 stderr 写一行 warning `⚠ readability extraction returned < 100 chars, falling back to innerText`
- 不改 `browser text` 的 ResponseContext shape；输出依然走 `value` 字段
- 不破坏非 `--readable` 路径的现有测试；现有测试必须全部继续通过

## 边界

### 允许修改
- packages/cli/src/browser/observation/text.rs
- packages/cli/Cargo.toml
- packages/cli/src/browser/observation/mod.rs（如果需要引入新的 helper 模块）
- packages/cli/tests/e2e/**（新增 readable 场景）

### 禁止做
- 不修改 `browser text` 已有的 innerText 路径行为（带 selector 或不带 selector 都不变）
- 不改 `browser html` 命令
- 不改 `--session` / `--tab` 解析
- 不新增 CLI subcommand（复用现有的 `browser text`）
- 不把 Readability 输出改成 markdown（那是未来的增强）
- 不动 `browser snapshot`

## 完成条件

场景: --readable 在标准博客页返回比 innerText 短的干净文本
  测试:
    包: actionbook-cli
    过滤: text_readable_strips_noise_on_blog
  层级: integration
  命中: Readability crate, reqwest, CDP
  假设 已在 session 里打开一个真实博客页（如 without.boats/blog/why-async-rust/）
  当 执行 `actionbook browser text --readable --session <s> --tab <t>`
  那么 返回文本长度比同页的 `browser text`（无 --readable）短超过 30%
  并且 返回文本包含文章关键词（如 "async" / "Rust"）
  并且 返回文本不包含典型 chrome 噪声如 "Light Mode" / "Subscribe"

场景: --readable 对有 article 标签的页面正确抽取
  测试:
    包: actionbook-cli
    过滤: text_readable_picks_article_tag
  层级: integration
  命中: Readability crate
  假设 已在 session 里打开一个有明确 `<article>` 结构的页面
  当 执行 `actionbook browser text --readable --session <s> --tab <t>`
  那么 返回文本与 `<article>` 内文本高度重合（覆盖率 > 80%）

场景: --readable 抽取失败时 fallback 到 innerText 并告警
  测试:
    包: actionbook-cli
    过滤: text_readable_fallback_when_extraction_empty
  层级: integration
  命中: Readability crate
  假设 已在 session 里打开一个 Readability 无法识别主体的页面（如纯应用 SPA）
  当 执行 `actionbook browser text --readable --session <s> --tab <t>`
  那么 stderr 包含 `⚠ readability extraction returned < 100 chars`
  并且 返回文本等于 `document.body.innerText`（fallback 生效）

场景: --readable 与 selector 同时传入早期报错
  测试:
    包: actionbook-cli
    过滤: text_readable_conflicts_with_selector
  层级: unit
  假设 clap 解析阶段或命令分派早期
  当 执行 `actionbook browser text "#main" --readable --session <s> --tab <t>`
  那么 进程以非零退出码结束
  并且 stderr 包含 "INVALID_ARGUMENT" 或 "readable" 与 "selector" 不兼容的提示

场景: 不带 --readable 时行为完全不变（回归保护）
  测试:
    包: actionbook-cli
    过滤: text_without_readable_unchanged
  层级: integration
  命中: CDP innerText path
  当 执行 `actionbook browser text --session <s> --tab <t>` 不带 `--readable`
  那么 返回值与本次修改前的 innerText 一字不差（使用打开同一个 fixture 页的 baseline 对比）

场景: --readable 与已有 selector path 的代码路径互不干扰
  测试:
    包: actionbook-cli
    过滤: text_selector_still_returns_innertext
  层级: integration
  命中: CDP callFunctionOn path
  假设 session 内打开一个有 `#content` 的页面
  当 执行 `actionbook browser text "#content" --session <s> --tab <t>` 不带 `--readable`
  那么 返回该 selector 的 innerText，不经过 Readability 路径

## 排除范围

- Markdown 输出（保留 `<h1>`/`<p>`/`<a>` 等结构）
- `browser html --readable`（本任务只改 text）
- PDF/printable view 抽取
- 多语言 readability 启发（默认就能工作即可）
- feature-gating（等 `cargo bloat` 证明必要性再做）
- 改变 `ResponseContext` 的字段
- `browser snapshot` 的任何修改
- 非博客类页面的 Readability 调优
