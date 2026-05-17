spec: task
name: "provider-opencode-go"
inherits: project
tags: [research-cli, autoresearch, provider, opencode-go, third-party-llm]
estimate: 0.5d
depends: [research-autonomous-loop-v2]
---

## 意图

ascent-research 现有两个 LLM provider:`provider-claude`(经 `cc-sdk`,
需 Claude Code Pro 订阅 $20+/mo) 与 `provider-codex`(spawn `codex
app-server`,需 ChatGPT Plus $20+/mo)。**两者都要 GUI-级别订阅,且对
非美国地区支付不友好**;同时**没有任何路径访问中国厂商 model**(DeepSeek
/ Kimi / GLM / Qwen / MiniMax)。

社区贡献 PR #19(@Paul-Yuchao-Dong)指出了真实痛点 + 提供 [OpenCode Go]
(https://opencode.ai/zen/go) 路径:$10/mo 聚合订阅,**单 API key + 标
准 HTTP**(OpenAI-compatible + Anthropic-compatible 两种 wire 协议),
直连上述中国厂商 model。

本 spec 把 PR #19 的核心价值采纳为正式 feature `provider-opencode-go`,
但**只采纳作者的实质 contribution,过滤掉个人偏好**:

- 采纳:新 provider trait impl + OpenAI/Anthropic 双协议 +
  `reasoning_content` 兜底 + Windows 文档思路 + bilingual 支持
- 不采纳:默认 provider `fake → opencode-go`(把作者个人 workflow 写
  进 upstream 默认值)、`is_anthropic()` 按 model 名前缀启发式路由(脆)、
  `temperature=0.7` / `max_tokens=32768` 硬编码、未验证的 `kimi-k2.6` /
  `deepseek-v4-pro` 模型默认

历史脉络:已实现的 `cc-sdk` 与 `codex` provider 都没正式 spec(早于
spec-first 流程上线);本 provider 走 spec-first,顺便给后续新增 provider
立模板。

## 已定决策

### Feature flag + dep

`packages/research/Cargo.toml`:

```toml
[features]
provider-opencode-go = ["autoresearch", "dep:reqwest"]

[dependencies]
reqwest = { version = "0.12", default-features = false,
            features = ["json", "rustls-tls"], optional = true }
```

- **`rustls-tls`** 不要 system OpenSSL(Windows / Alpine / 小镜像都无痛)
- **`default-features = false`** 避免拖入 `native-tls` / `cookies` 等
  不需要的子 feature
- **optional**:只有 `--features provider-opencode-go` 才拉 reqwest;默
  认 install 0 影响

### Provider 配置 — 全部 env-driven,**无硬编码默认 model**

| env var | 必需 | 默认 | 说明 |
|---------|------|------|------|
| `OPENCODE_API_KEY` | ✅ | — | OpenCode Go API key(以 `oc-` 开头);未设 → `ProviderError::NotAvailable("OPENCODE_API_KEY not set")` |
| `ASR_OPENCODE_MODEL` | ✅ | — | model id(如 `deepseek-v3.2-exp` / `glm-4.6` / `kimi-k2-instruct` —— **由用户依据 OpenCode Go 当时实际目录选择**);未设 → `ProviderError::NotAvailable("ASR_OPENCODE_MODEL not set; see https://opencode.ai/zen/go for current model list")` |
| `ASR_OPENCODE_PROTOCOL` | 否 | `"openai"` | `openai` \| `anthropic`;**显式选**,不靠 model 名前缀猜测 |
| `ASR_OPENCODE_TEMPERATURE` | 否 | `0.2` | 研究/推理任务低温更稳;`f32` 0.0-2.0;parse fail → fallback to default |
| `ASR_OPENCODE_MAX_TOKENS` | 否 | `16384` | 安全中位;真实上限按 model 走(超了 server 自截);超 65536 报错 |
| `ASR_OPENCODE_TIMEOUT_MS` | 否 | `120000` | 单次 request HTTP timeout;clamp 到 `[5000, 600000]` |
| `ASR_OPENCODE_ENDPOINT_OPENAI` | 否 | `https://opencode.ai/zen/go/v1/chat/completions` | 允许指向兼容代理 |
| `ASR_OPENCODE_ENDPOINT_ANTHROPIC` | 否 | `https://opencode.ai/zen/go/v1/messages` | 允许指向兼容代理 |

**关键决策:不带 model 默认值**。Paul 原 PR 默认 `deepseek-v4-pro` —— 这
个 model id 我们无法验证(OpenCode Go docs 是 SPA,curl 不到 model 目
录),写死等于赌运气。**让用户 explicit 设置 + 文档里指向官方 model
列表页**,把"current model 真实性"责任留给用户,**ascent 自己只承诺
协议正确**。

### Protocol 选择 — 显式 env var,不启发式猜

Paul 用 `model.starts_with("minimax")` 判断走 Anthropic 端点。问题:
- OpenCode Go 加任意 `claude-*` 模型 → 默认 OpenAI 端点 → 错
- 假如某天 `qwen-anthropic-coder-2027` 出现 → 同样错
- 同一 vendor 不同 mode(对话 vs 推理)可能各走不同协议

改成 `ASR_OPENCODE_PROTOCOL` 显式 env var,**用户从 OpenCode docs 抄一
次**就完事。Future:可加可选 `[opencode_model_registry]` TOML 让用户写
自己的 model → protocol 映射,**本 spec 不做**。

### HTTP behavior — 必须 timeout + 必须 retry

```rust
// reqwest::Client 必须有 timeout —— 默认无 timeout,网络挂了会永远 hang
Client::builder()
    .timeout(Duration::from_millis(timeout_ms))
    .build()?
```

- **timeout 默认 120s**,大模型大 max_tokens 可能 90s+ 才完
- **429 / 503 retry**:指数退避 1s / 2s / 4s,共 3 次;**第 4 次 fail
  即 propagate**
- **其它 HTTP 4xx / 5xx**:不 retry,直接 `ProviderError::CallFailed`
- **超时 / 连接 reset**:计 1 次 retry(network 抖动),第 2 次仍失败 →
  `ProviderError::CallFailed`

### 响应解析 — `content` 优先 + `reasoning_content` 兜底

OpenAI 路径:

```rust
// json["choices"][0]["message"]
// 1. 试 ["content"](str) 且 非空
// 2. fallback ["reasoning_content"](str)  -- DeepSeek 推理模型遇到 budget
//    用尽时会把 final answer 塞这字段(Paul PR 实测发现,保留)
// 3. 都没拿到 → ProviderError::CallFailed("unexpected response shape: ...")
```

Anthropic 路径:

```rust
// json["content"](array)
// 收集 type == "text" 的 block,join 它们的 "text"
// 空 → ProviderError::EmptyResponse
```

### `AgentProvider::name() = "opencode-go"`

跟 PR 一致。

### 不动什么(明确)

- **CLI `--provider` default 不变** —— 仍 `fake`。要用 opencode-go 必
  须 `--provider opencode-go`。这点跟 PR 不一样。
- **wiki query 的 `--provider` default 不变** —— 仍 `claude`。
- 现有 `provider-claude` / `provider-codex` 行为零变化。
- AgentProvider trait 签名零变化。
- `bilingual.rs` provider chain 加 opencode-go(如果 feature 启用)
  —— 跟 Paul PR 一致。

### 风险与缓解

- **风险**:用户设错 `ASR_OPENCODE_PROTOCOL`(`openai` 调 `anthropic` 端
  点)→ 400 with JSON schema error。
  **缓解**:`ProviderError::CallFailed` 把 server 原始 message 透传(包
  含 "expected messages.system" 之类提示);文档里写明 protocol 选错的
  典型症状。

- **风险**:user 设 `ASR_OPENCODE_TIMEOUT_MS=999999999` 想要无限等。
  **缓解**:clamp 到 `[5000, 600000]` —— 5s 最低(防勿设 0)、10min
  最高(防 hang infrastructure)。clamp 是 silent 行为,**不报错**
  (不阻塞工作流),只 doc 注明。

- **风险**:`OPENCODE_API_KEY` 长得像 `oc-...` 但被人误设为 `sk-...`
  之类。
  **缓解**:不做格式校验 —— provider 直接拼 `Bearer <key>` 发出去,
  server 返 401 之后 propagate。format 校验属于 over-engineering。

- **风险**:reqwest 拖大依赖树(rustls + h2 + 一堆 sub-crates)。
  **缓解**:`default-features = false` + 只开 `json` + `rustls-tls`,把
  binary 体积影响降到最小。**且整段 gate 在 `provider-opencode-go`
  feature 后**,默认 `cargo install` 不受影响。

- **风险**:Paul 的工作没拿到 attribution。
  **缓解**:commit message `Co-Authored-By: Paul-Yuchao-Dong
  <paulcynic@gmail.com>`;CHANGELOG entry 显式致谢 + 引 PR #19;
  PR #19 close 评论解释"采纳哪些 / 不采纳哪些 / 为何"。

## 边界

### 允许修改

- `packages/research/Cargo.toml` —— 加 `provider-opencode-go` feature
  + 可选 `reqwest` dep
- `packages/research/src/autoresearch/mod.rs` —— 注册 `pub mod opencode_go`
- `packages/research/src/autoresearch/opencode_go.rs` —— 新文件,~200 行
- `packages/research/src/commands/loop_cmd.rs` —— 加 `"opencode-go"`
  arm 到 provider 选择
- `packages/research/src/cli.rs` —— help 字符串列出 opencode-go(不改
  default value)
- `packages/research/src/report/bilingual.rs` —— provider chain 加
  opencode-go 候选
- `packages/research/src/commands/wiki/query.rs`(或同等位置)—— 加
  opencode-go arm
- `packages/research/tests/opencode_go.rs` —— 新测试文件
- README.md —— "What's new in 0.4.2" 段落 + provider table 加 row
- CHANGELOG.md —— 0.4.2 entry
- skills/ascent-research/SKILL.md —— provider 列表加一行

### 禁止做

- **不**改 `--provider` 默认值(loop / wiki query 都不动)
- **不**写 model 默认值(没有 hardcoded model name)
- **不**用 model 名启发式判断 protocol(必须 env explicit)
- **不**改 `cc-sdk` / `codex` provider 行为
- **不**给 reqwest 拉 `native-tls` / `cookies` / `multipart` 子 feature
- **不**实现 token 流式输出(streaming);本 spec 只做完整 response
- **不**对 OpenCode Go specific bugs 做补丁(model-side 问题不该 client 兜)
- **不**预先验证 API key(`from_env()` 只读 env,不打网络;真实校验在
  第一次 `ask()` 调用时由 server 返 401)

## 验收标准

测试包:`packages/research/tests/opencode_go.rs`(integration unless
注明 unit);所有 HTTP 测试用 **in-process TcpListener mock**(参照
`tests/composite_fetch.rs::McpMock` pattern),不引新 dev-dep。

场景: from_env 缺 OPENCODE_API_KEY 报 NotAvailable
  测试: from_env_missing_key_returns_not_available
  假设 OPENCODE_API_KEY 未设
  当 调用 OpenCodeGoProvider::from_env()
  那么 返回 Err(ProviderError::NotAvailable)
  并且 错误 message 含 "OPENCODE_API_KEY"

场景: from_env 有 API key 但缺 ASR_OPENCODE_MODEL 报 NotAvailable
  测试: from_env_missing_model_returns_not_available
  假设 OPENCODE_API_KEY=test ASR_OPENCODE_MODEL 未设
  当 调用 OpenCodeGoProvider::from_env()
  那么 返回 Err(ProviderError::NotAvailable)
  并且 错误 message 含 "ASR_OPENCODE_MODEL"
  并且 错误 message 含 "opencode.ai/zen/go"(指向官方 model 列表)

场景: from_env 全配齐返回 OK
  测试: from_env_with_all_required_returns_ok
  假设 OPENCODE_API_KEY=test
  并且 ASR_OPENCODE_MODEL=deepseek-v3.2-exp
  当 调用 OpenCodeGoProvider::from_env()
  那么 返回 Ok(provider)
  并且 provider.name() == "opencode-go"

场景: protocol env 未设时默认 openai
  测试: protocol_defaults_to_openai
  假设 OPENCODE_API_KEY=test
  并且 ASR_OPENCODE_MODEL=foo
  并且 ASR_OPENCODE_PROTOCOL 未设
  当 调用 from_env() 并检查 provider.protocol()
  那么 protocol() 等于 Protocol::OpenAi

场景: protocol env "anthropic" 切到 Anthropic
  测试: protocol_env_anthropic_routes_to_anthropic
  假设 ASR_OPENCODE_PROTOCOL=anthropic
  当 调用 from_env()
  那么 provider.protocol() 等于 Protocol::Anthropic

场景: timeout env clamp 到合法区间
  测试: timeout_env_clamped_to_range
  假设 ASR_OPENCODE_TIMEOUT_MS=999999999(超 600000 上限)
  当 调用 from_env()
  那么 provider.timeout_ms() 等于 600000
  另:ASR_OPENCODE_TIMEOUT_MS=100(低于 5000)
  那么 provider.timeout_ms() 等于 5000

场景: temperature parse fail fallback 到 0.2
  测试: temperature_parse_fail_falls_back_to_default
  假设 ASR_OPENCODE_TEMPERATURE="not-a-number"
  当 调用 from_env()
  那么 provider.temperature() 约等于 0.2

场景: OpenAI 200 响应正确解析 content
  测试: openai_200_returns_content
  假设 in-process mock server 返 200 body
       {"choices":[{"message":{"content":"hello world"}}]}
  并且 ASR_OPENCODE_PROTOCOL=openai ASR_OPENCODE_ENDPOINT_OPENAI 指向 mock
  当 调用 ask("sys", "user")
  那么 返回 Ok("hello world")

场景: OpenAI content 为空且有 reasoning_content 走 fallback
  测试: openai_empty_content_falls_back_to_reasoning_content
  假设 mock 返 200 body
       {"choices":[{"message":{"content":"","reasoning_content":"hi"}}]}
  当 调用 ask()
  那么 返回 Ok("hi")(走 Paul 的 fallback)

场景: OpenAI content 与 reasoning_content 都空 → CallFailed
  测试: openai_both_empty_returns_call_failed
  假设 mock 返 200 body {"choices":[{"message":{"content":"","reasoning_content":""}}]}
  当 调用 ask()
  那么 返回 Err(ProviderError::CallFailed) 含 "unexpected response shape"

场景: HTTP 401 不 retry 直接 CallFailed
  测试: http_401_does_not_retry
  假设 mock 总是返 401
  当 调用 ask()
  那么 返回 Err(ProviderError::CallFailed)
  并且 mock 收到的请求数 == 1(无 retry)

场景: HTTP 429 触发 retry 最多 3 次,第 4 次仍失败即 propagate
  测试: http_429_retries_up_to_3_times
  假设 mock 总是返 429
  当 调用 ask()
  那么 返回 Err(ProviderError::CallFailed)
  并且 mock 收到的请求数 == 4(1 初次 + 3 retry)

场景: HTTP 429 在 retry 后成功
  测试: http_429_then_200_succeeds
  假设 mock 前 2 次返 429,第 3 次返 200 {"choices":[{"message":{"content":"ok"}}]}
  当 调用 ask()
  那么 返回 Ok("ok")
  并且 mock 收到 3 个请求

场景: HTTP 500 不 retry,作为 server 永久错误立即 fail
  测试: http_500_does_not_retry
  假设 mock 总是返 500
  当 调用 ask()
  那么 返回 Err(ProviderError::CallFailed)
  并且 mock 收到的请求数 == 1
  Note: 仅 429 + 503 进入 retry,500 视为永久 server bug,fail fast

场景: HTTP 503 触发 retry
  测试: http_503_retries
  假设 mock 总是返 503
  当 调用 ask()
  那么 返回 Err(ProviderError::CallFailed)
  并且 mock 收到的请求数 == 4

场景: Anthropic 200 响应正确解析 content array
  测试: anthropic_200_returns_joined_text_blocks
  假设 ASR_OPENCODE_PROTOCOL=anthropic
  并且 mock 返 200 body {"content":[{"type":"text","text":"foo"},{"type":"text","text":"bar"}]}
  当 调用 ask()
  那么 返回 Ok("foobar")

场景: Anthropic content array 空 → EmptyResponse
  测试: anthropic_empty_content_returns_empty_response
  假设 mock 返 200 body {"content":[]}
  当 调用 ask()
  那么 返回 Err(ProviderError::EmptyResponse)

场景: HTTP timeout 触发即 retry 1 次
  测试: http_timeout_retries_once
  假设 mock 第一次 sleep 超 timeout 才回,第二次正常 200
  当 调用 ask() 配 ASR_OPENCODE_TIMEOUT_MS=2000
  那么 返回 Ok(...)
  并且 mock 收到 2 个请求

场景: provider name 等于 "opencode-go"
  测试: name_is_opencode_go
  假设 任意已构造的 provider
  当 调用 .name()
  那么 返回 "opencode-go"

场景: cli loop --provider default 仍是 fake(回归保护)
  测试: cli_loop_provider_default_is_fake_regression
  假设 编译时启用 provider-opencode-go feature
  当 解析 `ascent-research loop foo` 的 args
  那么 args.provider == "fake"
  Note: 这条 BDD 是 cli arg 解析层面,防止以后被改

## 排除范围

- 不实现 streaming response(SSE / chunked transfer)
- 不实现 token 计数 / cost tracking
- 不预存 model registry —— 用户每次显式设 `ASR_OPENCODE_MODEL`
- 不实现 model 自动发现(没有 `/v1/models` discovery 调用)
- 不为 OpenCode Go 特定 server bug 做 client patch
- 不实现 API key rotation / refresh
- 不实现 protocol auto-detect(必须 env 显式)
- 不改 `--provider` 默认值
- 不动 `cc-sdk` / `codex` provider 行为
- 不实现 wiki query 的 opencode-go default 改动(保持 claude default)
- 不引 `wiremock` crate dev-dep —— 用 in-process TcpListener 模式
