spec: task
name: "browser-fetch-oneshot"
inherits: project
tags: [actionbook, cli, research-workflow, phase-2, superseded]
estimate: 1d
depends: [browser-text-readable, browser-text-readable-paragraphs]
status: removed
---

### ⚠ REMOVED 2026-04-17

This task was implemented (commits `3263ee7e`, `3818b579`) then reverted
(commit `b0d969ce`) after Layer 4 end-to-end acceptance surfaced two bugs:

1. `IO_ERROR: early eof` on live URLs (fixed)
2. Second call on same session returns `about:blank` with 0 bytes —
   silent failure that produces empty research output without surfacing
   an error (**unresolved**)

Decision: the 3-step pattern (`new-tab` + `wait network-idle` + `text`)
is canonical for the research workflow. A one-shot command saves ~2 IPC
round-trips per source but hides per-primitive observability from the
LLM consumer, which is unacceptable when source quality judgement
depends on per-step status, URL, and byte counts.

The original intent (below) is preserved for historical reference. If a
future iteration wants to revisit, the observability loss must be
addressed first — e.g., by returning all intermediate step statuses in
the response envelope rather than one black-box result.

---

## 意图

Research 工作流里最常用的模式是「导航 → 等载入 → 抽文本」,当前需要三次 CLI 调用:

```bash
actionbook browser new-tab "<url>" --session s --tab t
actionbook browser wait network-idle --session s --tab t
actionbook browser text --session s --tab t
```

每篇博客就是 3 个 daemon round-trip,一次研究读 10 个源 = 30 个 round-trip。Retrospective
里 B4 被列为 Tier 1 高 ROI 项:减 2/3 的 round-trip。

本任务新增 `actionbook browser fetch <url> --session s` 子命令,内部执行上面三步序列,
用完自动关闭 tab,把结果按现有 `browser text` 的响应 shape 返回。同时透传 `--readable`
让抽取路径完整可用。目标是让 active-research skill 把 "one-shot read" 从 3 行变 1 行,
并且研究结束后 session 里没有残留 tab。

## 已定决策

- 新 subcommand:`browser fetch <url>`,不是 flag
- 必需 arg:`<url>` 位置参数,`--session` flag
- 可选 flag:`--readable`(透传给内部 text 调用)、`--timeout <ms>`(作用于 wait network-idle,默认 15000)
- 内部创建 ephemeral tab,tab_id 由 daemon 自动分配(不需要 `--tab` 参数)
- **无条件清理 tab**:成功或失败都 close 内部 tab,不留痕迹
- 不自动创建 session:如果 `--session` 指向的 session 不存在,返回现有的 `SESSION_NOT_FOUND` fatal,不做 magic auto-start
- 响应 shape 复用 `browser text` 的 `{ "target": ..., "value": ..., "__ctx_url": ..., "__ctx_title": ... }`,保证调用方无感
- 错误时仍尝试清理 tab(best effort),错误本身按标准 `ActionResult::Fatal` 返回
- 失败分类:`NAVIGATION_FAILED`(goto 报错)/ `WAIT_TIMEOUT`(network-idle 超时但不算 fatal,尝试继续读 text)/ `EXTRACTION_FAILED`(text 报错)
- `wait network-idle` 超时**不**直接 fatal,因为很多页面载入慢但已经有可用内容;继续走 text,若 text 能拿到 > 100 字符就算成功

## 边界

### 允许修改
- packages/cli/src/browser/observation/ (新建 fetch.rs 或放在合适的位置)
- packages/cli/src/browser/mod.rs(注册新模块)
- packages/cli/src/cli.rs(注册新 subcommand)
- packages/cli/tests/e2e/(新增 fetch 场景)

### 禁止做
- 不修改 `browser new-tab`、`browser wait network-idle`、`browser text` 的公有行为
- 不改 `--readable` 在 `browser text` 上的行为
- 不增加 session 自动创建(保持 stateless CLI 哲学)
- 不加 `--no-cleanup` 这类 flag 让 tab 保留(简化心智,要保留就用 3 步模式)
- 不新增 Cargo 依赖(复用现有 goto / wait / text 的内部实现)
- 不改 `__warnings` channel

## 完成条件

场景: fetch 一个简单页面返回文本并清理 tab
  测试:
    包: actionbook-cli
    过滤: fetch_returns_text_and_cleans_tab
  层级: integration
  命中: goto + wait network-idle + text + close-tab 内部序列
  假设 session s1 已启动,session 内当前有 1 个 tab
  当 执行 `actionbook browser fetch "data:text/html,<html><body>hello</body></html>" --session s1`
  那么 进程退出码为 0
  并且 响应 data.value 非 null
  并且 fetch 结束后 session 内 tab 数量仍然是 1(内部 tab 已关闭)

场景: fetch 透传 --readable 做 readability 抽取
  测试:
    包: actionbook-cli
    过滤: fetch_readable_extracts_article
  层级: integration
  命中: browser fetch + browser text --readable path
  假设 session 有一个带 `<article>` 的 fixture 页面可通过 data: URL 载入
  当 执行 `actionbook browser fetch "<data-url>" --session s1 --readable`
  那么 返回的 value 文本包含文章关键词
  并且 value 不含 nav/footer 噪声字符串
  并且 value 至少有 2 个 `\n\n` 段落分隔(B3.1 的段落保留仍然工作)

场景: fetch 对不存在 session 报错
  测试:
    包: actionbook-cli
    过滤: fetch_missing_session_errors
  层级: unit/integration
  当 执行 `actionbook browser fetch "data:text/html,<html><body>hello</body></html>" --session no-such-session`
  那么 进程以非零退出码结束
  并且 返回 `SESSION_NOT_FOUND` 或等价错误 code
  并且 stderr 提示 "browser start" 或等价

场景: fetch 在 goto 失败时仍然清理 tab
  测试:
    包: actionbook-cli
    过滤: fetch_cleans_up_on_nav_failure
  层级: integration
  命中: fetch error-path cleanup
  假设 session s1 启动前有 1 个 tab
  当 执行 `actionbook browser fetch "this-is-not-a-url" --session s1`
  那么 进程以非零退出码结束
  并且 fetch 结束后 session 内 tab 数量仍然是 1(失败路径也清理了内部 tab)

场景: fetch 在 network-idle 超时时尝试继续读 text
  测试:
    包: actionbook-cli
    过滤: fetch_recovers_from_idle_timeout
  层级: integration
  命中: fetch wait-idle timeout fallthrough
  假设 目标 URL 是一个网络持续活跃但已有内容的 fixture
  当 执行 `actionbook browser fetch "<busy-url>" --session s1 --timeout 1500`
  那么 进程退出码为 0
  并且 响应 data.value 长度 > 0
  并且 wait-idle 内部超时不升级为 fatal 错误(fetch 继续走 text)

场景: fetch 与三步手写序列在同一页面上文本基本一致(等价性基线)
  测试:
    包: actionbook-cli
    过滤: fetch_equivalence_with_three_step
  层级: integration
  命中: 对比 manual 序列 vs fetch 输出
  假设 fixture 在 session 内可稳定载入
  当 分别跑:(a) `new-tab + wait network-idle + text`,(b) `fetch`
  那么 两者返回的 value 非空
  并且 value 长度相差不超过 5%(允许 tab lifecycle 微差异)

## 排除范围

- `browser fetch` 对多个 URL 批量抓取(MVP 只支持单 URL)
- `--format snapshot` 或其它非 text 模式(保持单一职责,snapshot 需要可用再开独立任务)
- `--headful` / 渲染视觉等 flag(fetch 是 headless 逻辑,与 session 设置继承)
- 持久化缓存(research-session 级别 cache 是 Tier 2 范围)
- 批量并发 fetch(session-level 并发由上层编排)
- `browser fetch` 返回 HTML 模式(用 `browser html` 独立路径)
