# research-api-adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `postagent` 作为 API-first source adapter 接入 `active-research` 研究流程，让结构化源（Reddit / GitHub / arXiv）走 HTTP API、长尾源继续走浏览器。

**Architecture:** 本 repo（`research-api-adapter`）是协调层，承载设计文档、task spec、验证脚本。真正的代码改动落在两个上游：`postagent`（新增 `--anonymous` flag）和 `~/.claude/skills/active-research/SKILL.md`（命令面对齐 + 新增 API-First Sources section）。

**Tech Stack:** Rust (postagent-core, edition 2021), Bash + grep (验证脚本), agent-spec 0.2.7 (契约驱动), Markdown (SKILL.md, DESIGN.md)。

---

## Prerequisites

- [ ] `agent-spec` CLI 已安装 (`command -v agent-spec` 返回路径)
- [ ] `postagent-core` 能 `cargo build` 成功
- [ ] `postagent` npm 包已发布到本地（方便 recipe 脚本直接调 `postagent send`），或使用 `cargo run --bin postagent-core --` 作为替代
- [ ] 可访问 `~/.claude/skills/active-research/SKILL.md`（确认写权限）
- [ ] 当前在 `/Users/zhangalex/Work/Projects/actionbook/research-api-adapter/` 目录

## 执行顺序（依赖图）

```
Spec 1: postagent-anonymous-flag   [0.5d] ──┐
                                            ├──> Spec 3: active-research-api-sources [1d]
Spec 2: active-research-cli-alignment [0.5d] ┘
```

**关键路径**：Spec 1 → Spec 3（1.5 天）
**并行窗口**：Spec 1 和 Spec 2 可以同时进行
**阻塞关系**：Spec 3 需要等 Spec 1 和 Spec 2 都落地

## File Structure

本次实现会创建/修改以下文件：

```
/Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/
├── src/cli.rs                           # 修改：Send 变体加 --anonymous
├── src/commands/send.rs                 # 修改：token 检查逻辑 + 函数签名
├── src/main.rs                          # 修改：Send dispatch 传递 anonymous
└── tests/                               # 新建目录
    └── cli_anonymous.rs                 # 新建：4 个集成测试

/Users/zhangalex/.claude/skills/active-research/
└── SKILL.md                             # 修改：命令面对齐 + 新增 API-First Sources section

/Users/zhangalex/Work/Projects/actionbook/research-api-adapter/
├── PLAN.md                              # 本文件
├── DESIGN.md                            # 已存在
├── specs/                               # 已存在
├── scripts/                             # 新建目录
│   ├── cli_enum_source.sh               # 工具脚本：从 cli.rs 提取 subcommand 列表
│   ├── verify_skill_cli_alignment.sh    # Spec 2 主断言
│   ├── assert_no_browser_fetch.sh
│   ├── assert_no_browser_batch.sh
│   ├── assert_wait_network_idle.sh
│   ├── assert_readability_note.sh
│   ├── assert_api_first_sources_section.sh
│   ├── assert_routing_rule.sh
│   ├── assert_fallback_pattern.sh
│   └── assert_out_of_scope_markers.sh
└── tests/                               # 新建目录
    ├── recipe_reddit_anonymous.sh
    └── recipe_arxiv_anonymous.sh
```

每个 bash 脚本单一职责：一个断言对应一个脚本，便于 spec scenarios 的精准引用。

---

## Spec 1 — postagent-anonymous-flag

**Repo:** `/Users/zhangalex/Work/Projects/actionbook/postagent`
**Spec:** `specs/postagent-anonymous-flag.spec.md`
**Estimate:** 0.5d

**Strategy:** TDD，每个 scenario 先写测试（RED），再改代码（GREEN），提交。因为 spec 禁止新增 Cargo 依赖（包括 dev-dep），所有测试用 `std::process::Command` + `env!("CARGO_BIN_EXE_postagent-core")` 驱动二进制，不引入 `assert_cmd` 或 `httpmock`。

### Task 1.1: 建立 tests/ 目录骨架

**Files:**
- Create: `postagent/packages/postagent-core/tests/cli_anonymous.rs`

- [ ] **Step 1: 创建 tests 目录并写入最小测试文件**

```bash
mkdir -p /Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/tests
```

Create `postagent/packages/postagent-core/tests/cli_anonymous.rs`:

```rust
//! Integration tests for `postagent send --anonymous`.
//!
//! These tests drive the `postagent-core` binary via `std::process::Command`
//! and do not depend on any extra crate (spec forbids new dependencies).

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_postagent-core"))
}

#[test]
fn sanity_binary_runs_help() {
    let output = bin().arg("--help").output().expect("binary runs");
    assert!(output.status.success(), "help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("search"), "help lists search subcommand");
}
```

- [ ] **Step 2: 运行 sanity 测试**

Run:
```bash
cd /Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core && cargo test --test cli_anonymous sanity_binary_runs_help
```

Expected: **PASS**（仅验证测试文件本身能编译 + 二进制能启动）

- [ ] **Step 3: 提交骨架**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/postagent && git add packages/postagent-core/tests/cli_anonymous.rs && git commit -m "[packages/postagent-core]test: bootstrap cli_anonymous integration tests"
```

---

### Task 1.2: RED — 写失败测试验证 `--anonymous` flag 被 clap 接受

- [ ] **Step 1: 追加失败测试到 cli_anonymous.rs**

在 `cli_anonymous.rs` 末尾追加：

```rust
#[test]
fn send_anonymous_flag_is_recognized_by_clap() {
    // 未实现前，clap 会以 code 2 "unexpected argument --anonymous" 退出。
    // 实现后，flag 被接受，但因为占位符依然缺失时行为依赖后续逻辑，
    // 这里只断言 clap 不再报 "unexpected argument"。
    let output = bin()
        .args(["send", "--anonymous", "https://example.invalid/"])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("unrecognized"),
        "--anonymous should be recognized by clap, got stderr: {stderr}"
    );
}
```

- [ ] **Step 2: 运行测试确认失败**

Run:
```bash
cargo test --test cli_anonymous send_anonymous_flag_is_recognized_by_clap
```

Expected: **FAIL**，stderr 提示 `unexpected argument '--anonymous'`

---

### Task 1.3: GREEN — 在 cli.rs 里添加 `--anonymous` flag

**Files:**
- Modify: `postagent/packages/postagent-core/src/cli.rs`

- [ ] **Step 1: 编辑 cli.rs 的 `Send` 变体**

Find the `Send` variant in `src/cli.rs` (around line 101-113) and add the `anonymous` field:

```rust
    /// Send an HTTP request
    #[command(after_help = "\
Token substitution:
  Use $POSTAGENT.<SITE>.API_KEY in URL, headers, or body to inject saved keys.
  Save a key first with `postagent auth <SITE>`.

Anonymous mode:
  Pass `--anonymous` to skip the placeholder requirement for public APIs
  like arXiv, Hacker News, or unauthenticated Reddit `.json` endpoints.

Examples:
  postagent send https://api.example.com/users -H 'Authorization: Bearer $POSTAGENT.GITHUB.API_KEY'
  postagent send --anonymous https://export.arxiv.org/api/query?search_query=ti:rust")]
    Send {
        /// Request URL
        url: String,
        /// HTTP method
        #[arg(short = 'X', long)]
        method: Option<String>,
        /// Request header (repeatable)
        #[arg(short = 'H', long, num_args = 1)]
        header: Vec<String>,
        /// Request body
        #[arg(short = 'd', long)]
        data: Option<String>,
        /// Skip the `$POSTAGENT.<SITE>.API_KEY` placeholder requirement
        /// for public/unauthenticated APIs.
        #[arg(long)]
        anonymous: bool,
    },
```

Also update the existing CLI-parse unit tests at the bottom of `cli.rs` to match the new field (they may pattern-match `Send { url, method, header, data }` — add `anonymous: _` or rename the binding).

- [ ] **Step 2: 编辑 main.rs 的 dispatch**

Find the `Commands::Send { ... }` arm in `src/main.rs` (line 43-48):

```rust
        Commands::Send {
            url,
            method,
            header,
            data,
            anonymous,
        } => commands::send::run(url, method.as_deref(), header, data.as_deref(), *anonymous),
```

- [ ] **Step 3: 更新 send::run 签名（先接受参数，暂不改逻辑）**

Edit `src/commands/send.rs`:

```rust
pub fn run(
    raw_url: &str,
    method: Option<&str>,
    headers: &[String],
    data: Option<&str>,
    anonymous: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = anonymous; // unused for now, Task 1.4 consumes it
    // ... rest unchanged
```

- [ ] **Step 4: 编译 + 运行 Task 1.2 的测试**

Run:
```bash
cargo test --test cli_anonymous send_anonymous_flag_is_recognized_by_clap
```

Expected: **PASS**（clap 接受 flag，example.invalid 触发 "Missing $POSTAGENT." 或 URL builder 错误，stderr 不再包含 "unexpected argument"）

- [ ] **Step 5: 跑一次完整 unit + integration 测试保证没打破现有行为**

Run:
```bash
cargo test
```

Expected: 所有现有测试（`parse_header_*` 等）继续 PASS

- [ ] **Step 6: Commit**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/postagent && git add packages/postagent-core/src/cli.rs packages/postagent-core/src/main.rs packages/postagent-core/src/commands/send.rs packages/postagent-core/tests/cli_anonymous.rs && git commit -m "[packages/postagent-core]feat: wire --anonymous flag through CLI (no behavior yet)"
```

---

### Task 1.4: RED+GREEN — 实际绕过占位符检查

- [ ] **Step 1: 追加失败测试 `send_anonymous_get_returns_ok`（网络集成，默认 ignore）**

在 `cli_anonymous.rs` 追加：

```rust
#[test]
#[ignore = "live network; run with `cargo test -- --ignored`"]
fn send_anonymous_get_returns_ok() {
    let output = bin()
        .args([
            "send",
            "--anonymous",
            "http://export.arxiv.org/api/query?search_query=ti:rust&max_results=1",
        ])
        .output()
        .expect("binary runs");
    assert!(
        output.status.success(),
        "arxiv anonymous GET should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("<feed"),
        "arxiv response should contain <feed>, got: {}",
        &stdout[..stdout.len().min(200)]
    );
}
```

- [ ] **Step 2: 尝试运行该测试（将因占位符检查失败）**

Run:
```bash
cargo test --test cli_anonymous send_anonymous_get_returns_ok -- --ignored
```

Expected: **FAIL** — stderr 包含 `Missing $POSTAGENT.`

- [ ] **Step 3: 在 send.rs 里跳过 anonymous 场景下的占位符检查**

Edit `src/commands/send.rs` line 12-26，把函数顶部改成：

```rust
pub fn run(
    raw_url: &str,
    method: Option<&str>,
    headers: &[String],
    data: Option<&str>,
    anonymous: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // 0. Check for token template (skipped when --anonymous is set)
    if !anonymous {
        let has_token = contains_token_template(raw_url)
            || headers.iter().any(|h| contains_token_template(h))
            || data.map_or(false, |d| contains_token_template(d));
        if !has_token {
            eprintln!("Missing $POSTAGENT.<SITE>.API_KEY in headers or body.\n");
            eprintln!("Example: -H 'Authorization: Bearer $POSTAGENT.GITHUB.API_KEY'");
            std::process::exit(1);
        }
    }
    // 1. Template variable substitution
    // ...
```

注意：把 `let _ = anonymous;` 那行删掉。保留 `contains_token_template` 函数不动。`resolve_template_variables` 调用继续处理 URL/headers/data，即使在 anonymous 模式下也照常替换占位符（向后兼容）。

- [ ] **Step 4: 跑 live 测试确认通过**

Run:
```bash
cargo test --test cli_anonymous send_anonymous_get_returns_ok -- --ignored
```

Expected: **PASS**（arxiv 返回 Atom XML）

- [ ] **Step 5: 跑所有非 ignored 测试确认无回归**

Run:
```bash
cargo test
```

Expected: 所有测试 PASS

---

### Task 1.5: 写回归测试 — 默认行为保持拒绝

- [ ] **Step 1: 追加测试 `send_without_anonymous_rejects_missing_placeholder`**

```rust
#[test]
fn send_without_anonymous_rejects_missing_placeholder() {
    let output = bin()
        .args(["send", "https://example.invalid/"])
        .output()
        .expect("binary runs");
    assert!(
        !output.status.success(),
        "default mode must reject missing placeholder"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing $POSTAGENT."),
        "stderr should contain 'Missing $POSTAGENT.', got: {stderr}"
    );
}
```

- [ ] **Step 2: 跑测试（应直接 PASS，因为默认分支未改）**

Run:
```bash
cargo test --test cli_anonymous send_without_anonymous_rejects_missing_placeholder
```

Expected: **PASS**

---

### Task 1.6: 测试 — `--anonymous` 与占位符并存仍然替换

由于完整验证需要真实 HTTP 接口回显 header，这里分两层：
- **单元测试层**：直接调用 `token::resolve_template_variables`（公开函数）验证替换逻辑
- **集成层**：live 测试（`#[ignore]`）访问 httpbin.org 回显 header

- [ ] **Step 1: 追加单元测试到 cli_anonymous.rs**

```rust
#[test]
fn anonymous_mode_preserves_placeholder_substitution_unit() {
    // 直接调用 token::resolve_template_variables，不经过 send::run，
    // 但验证 anonymous 模式下该函数照常工作的前提：函数签名与行为未变。
    //
    // 这里通过外壳进程验证 CLI 仍然会调用替换逻辑：
    // 输入 --anonymous + 占位符，且 POSTAGENT.NONE.API_KEY 未配置 → 
    // 进程应报 "Token not found"（来自 resolve_template_variables），
    // 而不是跳过整个替换步骤。
    let output = bin()
        .args([
            "send",
            "--anonymous",
            "https://example.invalid/",
            "-H",
            "Authorization: Bearer $POSTAGENT.NONEXISTENT.API_KEY",
        ])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("$POSTAGENT.NONEXISTENT.API_KEY")
            || stderr.contains("not found")
            || stderr.contains("Token"),
        "anonymous mode should still attempt template substitution, got: {stderr}"
    );
}
```

注意：这个测试的具体 stderr 文本依赖 `resolve_template_variables` 当前的错误消息。先运行 `cargo run -- send https://example.invalid/ -H 'Authorization: Bearer $POSTAGENT.NONEXISTENT.API_KEY'` 看真实输出，再调整断言。

- [ ] **Step 2: 运行测试**

Run:
```bash
cargo test --test cli_anonymous anonymous_mode_preserves_placeholder_substitution_unit
```

Expected: **PASS**（如果断言文本不匹配，读 stderr 后调整断言关键词，再跑一遍）

---

### Task 1.7: 测试 — DNS 失败干净退出

- [ ] **Step 1: 追加测试**

```rust
#[test]
#[ignore = "live network; run with `cargo test -- --ignored`"]
fn send_anonymous_dns_error_exits_cleanly() {
    let output = bin()
        .args([
            "send",
            "--anonymous",
            "https://this-domain-does-not-exist-xyz.invalid/",
        ])
        .output()
        .expect("binary runs");
    assert!(!output.status.success(), "DNS failure should not exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!(
        "{}{}",
        stderr,
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !combined.contains("panicked"),
        "must not panic, output: {combined}"
    );
    assert!(
        combined.to_lowercase().contains("error")
            || combined.to_lowercase().contains("failed")
            || combined.to_lowercase().contains("dns")
            || combined.to_lowercase().contains("resolve"),
        "should print an error message, got: {combined}"
    );
}
```

- [ ] **Step 2: 运行 live 测试**

Run:
```bash
cargo test --test cli_anonymous send_anonymous_dns_error_exits_cleanly -- --ignored
```

Expected: **PASS**

- [ ] **Step 3: 运行全部测试（含 ignored）做总验收**

Run:
```bash
cargo test -- --include-ignored
```

Expected: 全部 PASS

---

### Task 1.8: Spec 1 验收 + Commit

- [ ] **Step 1: 跑 agent-spec lifecycle 做合同验证**

Run:
```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter && agent-spec lifecycle specs/postagent-anonymous-flag.spec.md --code /Users/zhangalex/Work/Projects/actionbook/postagent --change-scope none --format compact
```

Expected: 4 scenarios 的 verdict 显示为 `pass` 或 `skip`（依赖 test 运行时能否找到 package+filter）。可接受 skip——主要看有无 `fail`。

- [ ] **Step 2: 运行一次 stamp dry-run 预览 commit trailer**

Run:
```bash
agent-spec stamp specs/postagent-anonymous-flag.spec.md --code /Users/zhangalex/Work/Projects/actionbook/postagent --dry-run
```

Expected: 输出包含 `Spec-Name: postagent-anonymous-flag` 等 trailers。

- [ ] **Step 3: 最终提交 postagent 侧改动**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/postagent && git add packages/postagent-core/src/cli.rs packages/postagent-core/src/commands/send.rs packages/postagent-core/src/main.rs packages/postagent-core/tests/cli_anonymous.rs && git commit -m "[packages/postagent-core]feat: add --anonymous flag to send for public APIs

Skips the \$POSTAGENT.<SITE>.API_KEY placeholder requirement, enabling
anonymous access to arXiv, Reddit .json, Hacker News, and public GitHub
endpoints. Default behavior unchanged.

Spec-Name: postagent-anonymous-flag
Spec-Passing: true"
```

---

## Spec 2 — active-research-cli-alignment

**Targets:**
- `/Users/zhangalex/.claude/skills/active-research/SKILL.md`
- `research-api-adapter/scripts/`

**Spec:** `specs/active-research-cli-alignment.spec.md`
**Estimate:** 0.5d

**Strategy:** 先写验证脚本（可以在 SKILL.md 改动前就跑，预期失败），再逐条修 SKILL.md，最后让所有脚本通过。

### Task 2.1: 建立 scripts/ 目录 + 工具脚本

**Files:**
- Create: `research-api-adapter/scripts/cli_enum_source.sh`

- [ ] **Step 1: 创建目录**

```bash
mkdir -p /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts
```

- [ ] **Step 2: 写工具脚本 `cli_enum_source.sh`（提取当前 CLI 的所有 browser subcommand）**

Create `research-api-adapter/scripts/cli_enum_source.sh`:

```bash
#!/usr/bin/env bash
# Extract the list of valid `actionbook browser` subcommand names
# from packages/cli/src/cli.rs. Prints one name per line on stdout.
#
# Used by other assertion scripts as the source of truth.
set -euo pipefail

CLI_RS="${ACTIONBOOK_CLI_RS:-/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/src/cli.rs}"

if [[ ! -f "$CLI_RS" ]]; then
    echo "ERROR: cli.rs not found at $CLI_RS" >&2
    exit 2
fi

# Extract variant names from the BrowserCommands enum.
# Each variant we care about maps to a lowercase subcommand name.
awk '
    /pub enum BrowserCommands/ { in_enum = 1; next }
    in_enum && /^}/ { exit }
    in_enum && /#\[command\(alias *= *"([a-z-]+)"\)\]/ {
        # capture the alias
        match($0, /alias *= *"([a-z-]+)"/, arr)
        if (arr[1]) print arr[1]
    }
    in_enum && /^    [A-Z][a-zA-Z]*[ (]/ {
        # capture enum variant: "    NewTab(..." -> "new-tab"
        gsub(/[(,{ ].*$/, "")
        gsub(/^ +/, "")
        name = tolower($0)
        # CamelCase -> kebab-case
        cmd = ""
        for (i = 1; i <= length($0); i++) {
            c = substr($0, i, 1)
            if (c ~ /[A-Z]/ && i > 1) cmd = cmd "-"
            cmd = cmd tolower(c)
        }
        print cmd
    }
' "$CLI_RS" | sort -u
```

- [ ] **Step 3: 让脚本可执行 + 试跑**

```bash
chmod +x /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/cli_enum_source.sh
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/cli_enum_source.sh | head -30
```

Expected: 列出 `back / box / click / close / describe / eval / fill / goto / help / hover / html / ...`。**注意**：awk 可能没按预期转 kebab-case — 如果输出不干净，先手动核对结果，必要时调整 awk 或改用硬编码 enum name 列表作为 fallback。

- [ ] **Step 4: Commit 工具脚本**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter && git add scripts/cli_enum_source.sh && git commit -m "scripts: add cli_enum_source.sh for current browser subcommand list"
```

---

### Task 2.2: 写 4 个禁出现/必出现断言脚本

每个脚本都是 3-6 行的 grep + 退出码检查。

- [ ] **Step 1: `assert_no_browser_fetch.sh`**

Create `research-api-adapter/scripts/assert_no_browser_fetch.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"
COUNT=$(grep -c -E 'browser +fetch' "$TARGET" || true)
echo "$COUNT occurrences"
if [[ "$COUNT" -ne 0 ]]; then
    grep -n -E 'browser +fetch' "$TARGET" >&2
    exit 1
fi
```

- [ ] **Step 2: `assert_no_browser_batch.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"
COUNT=$(grep -c -E 'browser +batch' "$TARGET" || true)
echo "$COUNT occurrences"
if [[ "$COUNT" -ne 0 ]]; then
    grep -n -E 'browser +batch' "$TARGET" >&2
    exit 1
fi
```

- [ ] **Step 3: `assert_wait_network_idle.sh`**

```bash
#!/usr/bin/env bash
# Fails if any `wait-idle` appears without being preceded by `wait ` (space).
# Good: `browser wait network-idle`
# Bad:  `browser wait-idle`
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"
# Grep for 'wait-idle' NOT preceded by 'wait ' within the same word.
# Simpler: grep for bare 'wait-idle' as a whole token.
COUNT=$(grep -c -E '(^|[^a-zA-Z-])wait-idle' "$TARGET" || true)
echo "$COUNT bare wait-idle occurrences"
if [[ "$COUNT" -ne 0 ]]; then
    grep -n -E '(^|[^a-zA-Z-])wait-idle' "$TARGET" >&2
    exit 1
fi
```

- [ ] **Step 4: `assert_readability_note.sh`**

```bash
#!/usr/bin/env bash
# Asserts that the "Navigation Pattern" section contains an innerText note.
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"
# Extract content from "## Navigation Pattern" up to the next "## " header.
SECTION=$(awk '/^## Navigation Pattern/{flag=1; next} /^## /{flag=0} flag' "$TARGET")
if echo "$SECTION" | grep -q -E 'innerText'; then
    echo "innerText note present"
else
    echo "innerText note missing from Navigation Pattern section" >&2
    exit 1
fi
```

- [ ] **Step 5: 让四个脚本可执行**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter
chmod +x scripts/assert_no_browser_fetch.sh scripts/assert_no_browser_batch.sh scripts/assert_wait_network_idle.sh scripts/assert_readability_note.sh
```

- [ ] **Step 6: 试跑四个脚本（期望 3 个失败，因为 SKILL.md 还没改）**

```bash
for s in scripts/assert_no_browser_fetch.sh scripts/assert_no_browser_batch.sh scripts/assert_wait_network_idle.sh scripts/assert_readability_note.sh; do
    echo "=== $s ==="
    bash "$s" || echo "  (failed — expected before SKILL.md fix)"
done
```

Expected: `assert_no_browser_fetch.sh` 和 `assert_wait_network_idle.sh` 应该失败(SKILL.md 里确实有 `browser fetch` 和 `wait-idle`)，`assert_readability_note.sh` 失败（innerText 说明还没加），`assert_no_browser_batch.sh` 可能通过或失败（取决于 SKILL.md 当前内容）。

- [ ] **Step 7: Commit 脚本（先提交，SKILL.md 改动下一步做）**

```bash
git add scripts/assert_*.sh && git commit -m "scripts: add assertion scripts for SKILL.md command alignment"
```

---

### Task 2.3: 写主验证脚本 `verify_skill_cli_alignment.sh`

**Files:**
- Create: `research-api-adapter/scripts/verify_skill_cli_alignment.sh`

- [ ] **Step 1: 写主脚本**

Create `research-api-adapter/scripts/verify_skill_cli_alignment.sh`:

```bash
#!/usr/bin/env bash
# For every `actionbook browser <name>` occurrence in SKILL.md,
# verify <name> exists in the current BrowserCommands enum.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

VALID=$(bash "$SCRIPT_DIR/cli_enum_source.sh")

# Extract all `actionbook browser <word>` occurrences.
# Tolerate optional global flags before `browser`.
MATCHES=$(grep -oE 'actionbook[^`]*browser +[a-z][a-z-]*' "$TARGET" \
    | sed -E 's/.*browser +([a-z-]+).*/\1/' \
    | sort -u)

FAIL=0
if [[ -z "$MATCHES" ]]; then
    echo "all references match (0 references found)"
    exit 0
fi

while IFS= read -r cmd; do
    if ! echo "$VALID" | grep -q -Fx "$cmd"; then
        echo "unknown subcommand: $cmd" >&2
        FAIL=1
    fi
done <<< "$MATCHES"

if [[ "$FAIL" -eq 0 ]]; then
    echo "all references match"
else
    exit 1
fi
```

- [ ] **Step 2: chmod + 试跑**

```bash
chmod +x scripts/verify_skill_cli_alignment.sh
bash scripts/verify_skill_cli_alignment.sh
```

Expected: 失败，stderr 输出 `unknown subcommand: fetch` 等（因为 SKILL.md 还没改）

- [ ] **Step 3: Commit**

```bash
git add scripts/verify_skill_cli_alignment.sh && git commit -m "scripts: add verify_skill_cli_alignment.sh main checker"
```

---

### Task 2.4: 修订 SKILL.md — 替换 `browser fetch` 为三步序列

**Files:**
- Modify: `/Users/zhangalex/.claude/skills/active-research/SKILL.md`

**Strategy:** 逐段替换。先用 grep 定位所有 `browser fetch` 出现点。

- [ ] **Step 1: 列出所有 `browser fetch` 出现位置**

Run:
```bash
grep -n 'browser fetch' ~/.claude/skills/active-research/SKILL.md
```

预期会看到 3-6 处（"Navigation Pattern" section 的 Option A，"Complete Workflow" 里的示例等）。

- [ ] **Step 2: 针对每一处做替换**

对每一处 `actionbook [flags] browser fetch <url> [--format text|snapshot] [--json]`，替换为三步序列。例：

Before:
```bash
actionbook --block-images --rewrite-urls browser fetch "<url>" --format text --json
```

After:
```bash
# Three-step pattern (current CLI has no one-shot fetch):
actionbook browser new-tab "<url>" --session <s> --tab <t>
actionbook browser wait network-idle --session <s> --tab <t>
actionbook browser text --session <s> --tab <t>
```

对 `--format snapshot` 变体：

Before:
```bash
actionbook --block-images --rewrite-urls browser fetch "<url>" --format snapshot --max-tokens 2000 --json
```

After:
```bash
actionbook browser new-tab "<url>" --session <s> --tab <t>
actionbook browser wait network-idle --session <s> --tab <t>
actionbook browser snapshot --filter interactive --max-tokens 2000 --session <s> --tab <t>
```

对 `--lite` 变体（HTTP-first 优化）：当前 CLI 没有等价项，删除整个 `--lite` 段落，注释说明"当前 CLI 不支持 HTTP-first 优化，所有请求走浏览器"。

- [ ] **Step 3: 跑 `assert_no_browser_fetch.sh` 验证**

```bash
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/assert_no_browser_fetch.sh
```

Expected: `0 occurrences`，exit 0

---

### Task 2.5: 修订 SKILL.md — `wait-idle` → `wait network-idle`

- [ ] **Step 1: 用 sed 批量替换**

```bash
# 先备份
cp ~/.claude/skills/active-research/SKILL.md ~/.claude/skills/active-research/SKILL.md.bak

# 替换：wait-idle 前面不是 "wait " 的 → "wait network-idle"
# 用 perl 因为需要负前视
perl -i -pe 's/(?<!wait )\bwait-idle\b/wait network-idle/g' ~/.claude/skills/active-research/SKILL.md
```

- [ ] **Step 2: 跑断言脚本验证**

```bash
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/assert_wait_network_idle.sh
```

Expected: `0 bare wait-idle occurrences`，exit 0

- [ ] **Step 3: 人工扫读一下 `wait-fn` 是否被误替换**（`wait-fn` 不应出现在当前 CLI）

```bash
grep -n 'wait-fn' ~/.claude/skills/active-research/SKILL.md
```

预期会看到若干 `wait-fn` — 这些也是当前 CLI 没有的命令。对照 `BrowserCommands` 的 `Wait { Condition }`，`wait-fn "<expr>"` 应替换为 `wait condition "<expr>"`。

- [ ] **Step 4: 用 sed 替换 `wait-fn`**

```bash
perl -i -pe 's/\bwait-fn\b/wait condition/g' ~/.claude/skills/active-research/SKILL.md
```

---

### Task 2.6: 修订 SKILL.md — 删除 `browser batch` 或拆成多条

- [ ] **Step 1: 定位 batch 示例**

```bash
grep -n -A5 'browser batch' ~/.claude/skills/active-research/SKILL.md
```

- [ ] **Step 2: 对每一段 `browser batch` JSON 示例做替换**

把整块 `batch` 命令替换为一组等价的 `click` / `fill` / `select` 命令。例：

Before:
```bash
cat <<'EOF' | actionbook browser batch --delay 150
{
  "actions": [
    {"kind": "click", "selector": "#terms-0-field"},
    {"kind": "click", "selector": "option[value='title']"},
    {"kind": "type", "selector": "#terms-0-term", "text": "large language model agent"}
  ]
}
EOF
```

After:
```bash
# batch is not available in current CLI; execute actions sequentially.
actionbook browser click "#terms-0-field" --session <s> --tab <t>
actionbook browser click "option[value='title']" --session <s> --tab <t>
actionbook browser fill "#terms-0-term" "large language model agent" --session <s> --tab <t>
```

注意：`type` 命令当前 CLI 里叫 `Type`（keystroke-by-keystroke）但 spec 不改变 CLI，所以用 `fill`（一次性设值）或 `type` 都行，按原场景选。

- [ ] **Step 3: 跑断言脚本**

```bash
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/assert_no_browser_batch.sh
```

Expected: `0 occurrences`，exit 0

---

### Task 2.7: 在 Navigation Pattern section 追加 innerText 说明

- [ ] **Step 1: 打开 SKILL.md，定位 "## Navigation Pattern" section**

- [ ] **Step 2: 在 section 末尾追加段落**

```markdown
### innerText vs readability

The current `packages/cli` CLI has **no `--mode readability` option** — `browser text`
returns the raw `innerText` from the page. Readability extraction (boilerplate removal,
main-article detection) is not available from the CLI. If you need cleaner content,
either:

1. Accept `innerText` noise (blog layouts, navigation, footer) and rely on the LLM to filter.
2. Use `browser html` to get raw HTML and extract locally in the orchestrator.

Do not use `--format text` or `--mode readability` — these flags do not exist.
```

- [ ] **Step 3: 跑断言**

```bash
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/assert_readability_note.sh
```

Expected: `innerText note present`，exit 0

---

### Task 2.8: 跑主验证脚本 + 错误路径回归

- [ ] **Step 1: 跑主脚本（全量检查）**

```bash
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/verify_skill_cli_alignment.sh
```

Expected: `all references match`，exit 0

- [ ] **Step 2: 跑 error-path 场景（fixture 含非法 `browser fetch`）**

Create a temp fixture:

```bash
TMPF=$(mktemp /tmp/skill-fixture-XXXX.md)
cat > "$TMPF" <<'EOF'
## Navigation
actionbook browser fetch https://example.com
EOF
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/verify_skill_cli_alignment.sh "$TMPF"
```

Expected: exit code 1，stderr 输出 `unknown subcommand: fetch`

- [ ] **Step 3: 清理 fixture**

```bash
rm "$TMPF"
```

---

### Task 2.9: Spec 2 验收 + Commit

- [ ] **Step 1: 跑 lifecycle（shell 脚本作为 test，可能为 skip/uncertain，但无 fail 即可）**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter && agent-spec lifecycle specs/active-research-cli-alignment.spec.md --code . --change-scope worktree --format compact
```

Expected: 无 fail。skip 可接受（shell 脚本测试机械匹配超出 TestVerifier 能力）。

- [ ] **Step 2: Commit research-api-adapter 侧**

```bash
git add scripts/ && git commit -m "scripts: add SKILL.md command alignment assertions

Spec-Name: active-research-cli-alignment
Spec-Passing: true"
```

- [ ] **Step 3: SKILL.md 的改动不在本 repo，但要确保 `.bak` 文件已删除**

```bash
rm -f ~/.claude/skills/active-research/SKILL.md.bak
```

SKILL.md 本身不在 git 管理范围；改动落地即可。

---

## Spec 3 — active-research-api-sources

**Targets:**
- `/Users/zhangalex/.claude/skills/active-research/SKILL.md`
- `research-api-adapter/scripts/`
- `research-api-adapter/tests/`

**Spec:** `specs/active-research-api-sources.spec.md`
**Estimate:** 1d
**Depends:** Spec 1（postagent-anonymous-flag）+ Spec 2（cli-alignment）必须都完成

### Task 3.1: 建立 tests/ 目录 + 写 recipe 测试

- [ ] **Step 1: 创建目录**

```bash
mkdir -p /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests
```

- [ ] **Step 2: 写 `recipe_reddit_anonymous.sh`**

Create `research-api-adapter/tests/recipe_reddit_anonymous.sh`:

```bash
#!/usr/bin/env bash
# Recipe test: anonymous Reddit thread fetch via postagent.
set -euo pipefail

URL="https://www.reddit.com/r/rust/top.json?t=week&limit=3"

OUTPUT=$(postagent send --anonymous "$URL" 2>&1) || {
    EXIT=$?
    if echo "$OUTPUT" | grep -q -E '(unexpected argument|unrecognized)'; then
        echo "FAIL: postagent is too old; --anonymous flag not recognized" >&2
        echo "Fix: update postagent per spec postagent-anonymous-flag" >&2
        exit 2
    fi
    echo "FAIL: postagent send exited $EXIT" >&2
    echo "$OUTPUT" >&2
    exit 1
}

# Response must be valid JSON and contain "data" at root.
if ! echo "$OUTPUT" | python3 -c 'import sys, json; d = json.load(sys.stdin); assert "data" in d' 2>/dev/null; then
    echo "FAIL: response is not valid JSON or missing 'data' field" >&2
    echo "$OUTPUT" | head -c 200 >&2
    exit 1
fi

echo "recipe_reddit_anonymous: PASS"
```

- [ ] **Step 3: 写 `recipe_arxiv_anonymous.sh`**

Create `research-api-adapter/tests/recipe_arxiv_anonymous.sh`:

```bash
#!/usr/bin/env bash
# Recipe test: anonymous arXiv API query via postagent.
set -euo pipefail

URL="http://export.arxiv.org/api/query?search_query=ti:rust&max_results=3"

OUTPUT=$(postagent send --anonymous "$URL" 2>&1) || {
    EXIT=$?
    if echo "$OUTPUT" | grep -q -E '(unexpected argument|unrecognized)'; then
        echo "FAIL: postagent is too old; --anonymous flag not recognized" >&2
        echo "Fix: update postagent per spec postagent-anonymous-flag" >&2
        exit 2
    fi
    echo "FAIL: postagent send exited $EXIT" >&2
    echo "$OUTPUT" >&2
    exit 1
}

if ! echo "$OUTPUT" | grep -q '<feed'; then
    echo "FAIL: response does not contain <feed> (expected Atom XML)" >&2
    echo "$OUTPUT" | head -c 200 >&2
    exit 1
fi

echo "recipe_arxiv_anonymous: PASS"
```

- [ ] **Step 4: chmod + 试跑（因为 postagent 已有 --anonymous，应该 PASS）**

```bash
chmod +x /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests/*.sh
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests/recipe_reddit_anonymous.sh
bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests/recipe_arxiv_anonymous.sh
```

Expected: 两个都 PASS

**如果 PASS**：继续。
**如果 recipe_reddit 返回 429/403**：Reddit 对未认证请求有 rate-limit。在脚本里加 `--user-agent "research-api-adapter/0.1"` header 重试，或改用 `old.reddit.com`。
**如果 postagent CLI 不在 PATH**：用 `cargo run --manifest-path /Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/Cargo.toml --release -- send ...` 临时代替，稍后装全局。

---

### Task 3.2: 写 4 个 section 结构断言脚本

- [ ] **Step 1: `assert_api_first_sources_section.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

LINE=$(grep -n -E '^## API-First Sources' "$TARGET" | head -1 | cut -d: -f1 || true)
if [[ -z "$LINE" ]]; then
    echo "FAIL: API-First Sources section not found" >&2
    exit 1
fi

# Body between this header and the next ## header.
BODY=$(awk -v start="$LINE" '
    NR > start && /^## / { exit }
    NR > start { print }
' "$TARGET")

BYTES=$(echo -n "$BODY" | wc -c | tr -d ' ')
if [[ "$BYTES" -lt 200 ]]; then
    echo "FAIL: section body too short ($BYTES bytes)" >&2
    exit 1
fi

echo "section found at line $LINE, body bytes >= 200"
```

- [ ] **Step 2: `assert_routing_rule.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

# Extract API-First Sources section body.
BODY=$(awk '/^## API-First Sources/{flag=1; next} /^## /{flag=0} flag' "$TARGET")

if echo "$BODY" | grep -q 'postagent' && echo "$BODY" | grep -q 'actionbook browser'; then
    echo "routing rule present"
else
    echo "FAIL: routing rule must mention both 'postagent' and 'actionbook browser'" >&2
    exit 1
fi
```

- [ ] **Step 3: `assert_fallback_pattern.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

BODY=$(awk '/^## API-First Sources/{flag=1; next} /^## /{flag=0} flag' "$TARGET")

if echo "$BODY" | grep -q 'new-tab' && echo "$BODY" | grep -q 'wait network-idle' && echo "$BODY" | grep -q 'text'; then
    echo "fallback pattern present"
else
    echo "FAIL: fallback pattern must include new-tab, wait network-idle, text" >&2
    exit 1
fi
```

- [ ] **Step 4: `assert_out_of_scope_markers.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:-$HOME/.claude/skills/active-research/SKILL.md}"

BODY=$(awk '/^## API-First Sources/{flag=1; next} /^## /{flag=0} flag' "$TARGET")

MISSING=()
for kw in Tavily Exa Brave "Hacker News"; do
    if ! echo "$BODY" | grep -q "$kw"; then
        MISSING+=("$kw")
    fi
done

if [[ "${#MISSING[@]}" -eq 0 ]]; then
    echo "all out-of-scope markers present"
else
    echo "FAIL: missing out-of-scope markers: ${MISSING[*]}" >&2
    exit 1
fi
```

- [ ] **Step 5: chmod + 试跑（预期 4 个都失败，因为 section 还没写）**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter
chmod +x scripts/assert_api_first_sources_section.sh scripts/assert_routing_rule.sh scripts/assert_fallback_pattern.sh scripts/assert_out_of_scope_markers.sh
for s in scripts/assert_api_first_sources_section.sh scripts/assert_routing_rule.sh scripts/assert_fallback_pattern.sh scripts/assert_out_of_scope_markers.sh; do
    bash "$s" || echo "  (failed — expected before SKILL.md section added)"
done
```

Expected: 4 个脚本都失败并输出相应的 FAIL 消息。

- [ ] **Step 6: Commit 测试 + 断言脚本**

```bash
git add tests/ scripts/assert_api_first_sources_section.sh scripts/assert_routing_rule.sh scripts/assert_fallback_pattern.sh scripts/assert_out_of_scope_markers.sh
git commit -m "scripts: add API-First Sources section assertions and recipe tests"
```

---

### Task 3.3: 编辑 SKILL.md — 插入 API-First Sources section

**Files:**
- Modify: `/Users/zhangalex/.claude/skills/active-research/SKILL.md`

- [ ] **Step 1: 定位 "## Navigation Pattern" section 起始行**

```bash
grep -n '^## Navigation Pattern' ~/.claude/skills/active-research/SKILL.md
```

记录行号 N。新 section 要插入到第 N 行之前。

- [ ] **Step 2: 准备新 section 内容**

Create a temp file `/tmp/api-first-sources.md`:

```markdown
## API-First Sources (via postagent)

Before opening a URL in `actionbook browser`, check if it matches an API-accessible source.
Structured sources (Reddit, GitHub, arXiv) are 10-100x faster via HTTP API than browser
automation, and they come back as clean JSON/XML instead of scraped HTML.

**Routing rule:** If a URL matches an entry in the source table below, route it to
`postagent` with `--anonymous`. Otherwise fall back to the existing `actionbook browser`
Navigation Pattern (`new-tab` → `wait network-idle` → `text`).

### Source routing table

| Domain / URL shape | Executor | Command template |
|---|---|---|
| `reddit.com/r/*/comments/*` | postagent | `postagent send --anonymous "https://www.reddit.com/r/<sub>/comments/<id>.json"` |
| `reddit.com` search | postagent | `postagent send --anonymous "https://www.reddit.com/search.json?q=<q>"` |
| `github.com/{owner}/{repo}` | postagent | `postagent send --anonymous "https://api.github.com/repos/<o>/<r>/readme"` |
| `github.com/*/issues/*` | postagent | `postagent send --anonymous "https://api.github.com/repos/<o>/<r>/issues/<n>"` |
| `arxiv.org/abs/*` | postagent | `postagent send --anonymous "http://export.arxiv.org/api/query?id_list=<id>"` |
| arXiv keyword search | postagent | `postagent send --anonymous "http://export.arxiv.org/api/query?search_query=<q>"` |
| Anything else | `actionbook browser` | `new-tab <url>` → `wait network-idle` → `text` |

### Recipes

#### Reddit thread + comments

```bash
postagent send --anonymous "https://www.reddit.com/r/rust/comments/ABC123.json"
# Returns: JSON array; [0] is the post, [1].data.children is the comment tree.
```

#### GitHub repo README

```bash
postagent send --anonymous "https://api.github.com/repos/rust-lang/rust/readme"
# Returns: JSON { "content": "<base64 README>", ... }; decode `content` with base64.
```

#### arXiv keyword search

```bash
postagent send --anonymous "http://export.arxiv.org/api/query?search_query=ti:rust+async&max_results=10"
# Returns: Atom XML; each <entry> is a paper with title, authors, summary, link.
```

### Topic Detection extension

Add to the existing Topic Detection logic:

| Pattern | Type | Strategy |
|---|---|---|
| Topic mentions reddit / subreddit / r/xyz | Discussion | Start with `postagent send --anonymous` to the matching search endpoint |
| Topic mentions specific GitHub repo / project name | Code | Start with GitHub API via postagent, not `browser open` |

### Out of scope (this section)

The following sources are **not** covered by the Phase 1 MVP recipes above and should
continue to use `actionbook browser` or be added in Phase 2:

- **Tavily** / **Exa** / **Brave** search APIs — require user-provided tokens, Phase 2
- **Hacker News** Firebase API — Phase 2
- Any site without a documented public API — `actionbook browser` fallback

```

- [ ] **Step 3: 使用 sed 或 perl 插入**

```bash
# Read new section
NEW_SECTION=$(cat /tmp/api-first-sources.md)

# Use awk to insert before "## Navigation Pattern"
awk -v insert="$NEW_SECTION" '
    /^## Navigation Pattern/ && !done { print insert "\n\n---\n"; done = 1 }
    { print }
' ~/.claude/skills/active-research/SKILL.md > /tmp/skill-new.md

# Verify: show first 30 lines around the insertion
grep -n -A2 -B2 'API-First Sources' /tmp/skill-new.md | head -20

# If it looks right, move it in place
mv /tmp/skill-new.md ~/.claude/skills/active-research/SKILL.md
```

- [ ] **Step 4: 跑全部 4 个断言脚本验证**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter
for s in scripts/assert_api_first_sources_section.sh scripts/assert_routing_rule.sh scripts/assert_fallback_pattern.sh scripts/assert_out_of_scope_markers.sh; do
    echo "=== $s ==="
    bash "$s"
done
```

Expected: 4 个全部 PASS

- [ ] **Step 5: 跑 recipe tests 再验证一次（Spec 1 依赖已就绪，应 PASS）**

```bash
bash tests/recipe_reddit_anonymous.sh
bash tests/recipe_arxiv_anonymous.sh
```

Expected: 两个都 PASS

- [ ] **Step 6: 清理临时文件**

```bash
rm /tmp/api-first-sources.md
```

---

### Task 3.4: 回归测试 — postagent 老版本场景

- [ ] **Step 1: 模拟 postagent 老版本（通过临时修改 PATH 或 stub）**

验证 recipe 脚本在 postagent 没有 `--anonymous` 时输出清晰错误。本步骤不需要真的回滚 postagent——脚本内已有错误检测逻辑。做一次人工 review 即可：

```bash
grep -A3 'unexpected argument' /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests/recipe_reddit_anonymous.sh
```

Expected: 看到清晰的错误分支，exit 2 并指向 spec `postagent-anonymous-flag`。

- [ ] **Step 2 (可选): 用 shim 脚本做一次真正回归**

```bash
TMPDIR=$(mktemp -d)
cat > "$TMPDIR/postagent" <<'EOF'
#!/usr/bin/env bash
echo "error: unexpected argument '--anonymous' found" >&2
exit 2
EOF
chmod +x "$TMPDIR/postagent"

# 用 fake postagent 跑 recipe
PATH="$TMPDIR:$PATH" bash /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/tests/recipe_reddit_anonymous.sh || {
    CODE=$?
    echo "recipe exited $CODE (expected 2)"
}
rm -rf "$TMPDIR"
```

Expected: recipe 输出 `FAIL: postagent is too old` 并 exit 2

---

### Task 3.5: Spec 3 验收 + Commit

- [ ] **Step 1: 跑 lifecycle**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter && agent-spec lifecycle specs/active-research-api-sources.spec.md --code . --change-scope worktree --format compact
```

Expected: 无 fail

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "feat: integrate API-first sources into active-research

Adds scripts/ assertions, tests/ recipes, and records the implementation
gate for SKILL.md's API-First Sources section.

Spec-Name: active-research-api-sources
Spec-Passing: true"
```

---

## 全局验收

### Task G.1: 跑 guard 做整体检查

- [ ] **Step 1: 全量验证**

```bash
cd /Users/zhangalex/Work/Projects/actionbook/research-api-adapter && agent-spec guard --spec-dir specs --code . --format compact
```

Expected: 所有 spec 无 fail

### Task G.2: 运行 explain 生成 PR review 摘要

- [ ] **Step 1: 生成三份 markdown**

```bash
for spec in specs/postagent-anonymous-flag.spec.md specs/active-research-cli-alignment.spec.md specs/active-research-api-sources.spec.md; do
    echo "=== $spec ==="
    agent-spec explain "$spec" --code . --format markdown
    echo
done
```

review 前两个输出，确认每个 scenario 的 verdict 一目了然。

### Task G.3: （可选）打开 active-research 跑一个真实研究验证端到端

- [ ] **Step 1: 选一个主题跑 `/active-research`**

```bash
# In Claude Code session:
/active-research "Rust async runtime comparison 2026"
```

观察日志（如果有开启 tracing）：应该能看到 Reddit / GitHub / arXiv URL 被路由到 `postagent send --anonymous`，其它博客被路由到 `actionbook browser new-tab → wait network-idle → text`。

Expected: 报告正常生成，且结构化源的抓取明显比之前快。

---

## 风险登记

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| R1 | Reddit 对匿名请求有 rate-limit / User-Agent 检查 | recipe_reddit_anonymous.sh 偶发失败 | 在 URL 加 `?limit=3`，必要时切到 `old.reddit.com/r/<sub>/top.json` |
| R2 | cli_enum_source.sh 的 awk 解析不完美 | 主验证脚本漏报或误报 | 保留硬编码 fallback 列表；在 Task 2.1 Step 3 人工核对输出 |
| R3 | SKILL.md 里引用的命令比预期多 | Task 2.4-2.6 替换工作被低估 | 先跑一遍 `verify_skill_cli_alignment.sh` 数准确行数再估工时 |
| R4 | postagent send 原有错误消息文本在 Task 1.6 不匹配 | 测试断言需要调整 | 人工跑一次命令看真实 stderr，再调断言关键词 |
| R5 | `env!("CARGO_BIN_EXE_postagent-core")` 在 Task 1.1 找不到 | 集成测试无法启动 | 确认 Cargo.toml 的 `[[bin]]` section 有 `name = "postagent-core"`（已验证存在）|
| R6 | 两个不同 repo 的 git commit 需要各自推送 | 协调摩擦 | 每个 spec 在 stamp 时明确 repo，本计划的 commit 命令都带完整 `cd <repo>` 前缀 |

---

## Definition of Done

整个项目算完成，当且仅当：

1. **三份 spec 的 lifecycle 结果均无 `fail`**：`agent-spec guard --spec-dir specs --code . --format compact` 干净
2. **postagent 侧**：`cargo test`（含 `-- --ignored`）全绿；`cargo run --bin postagent-core -- send --anonymous http://export.arxiv.org/api/query?search_query=ti:rust&max_results=1` 能返回 Atom XML
3. **SKILL.md 侧**：`verify_skill_cli_alignment.sh` 返回 `all references match`；所有 `assert_*.sh` 返回 0
4. **research-api-adapter 侧**：`tests/recipe_*.sh` 全部 PASS
5. **端到端**：一次真实 `/active-research` 调用里能观察到结构化源走 postagent 路径（日志或命令历史为证）
6. **三个 repo 都有清晰的 commit**：postagent repo 有 `[packages/postagent-core]feat: add --anonymous flag`，research-api-adapter repo 有 scripts/tests/PLAN 的初版提交

---

## 参考依据

- **Spec 文档**：`specs/postagent-anonymous-flag.spec.md`, `specs/active-research-cli-alignment.spec.md`, `specs/active-research-api-sources.spec.md`
- **设计文档**：`DESIGN.md`
- **真实命令面来源**：`/Users/zhangalex/Work/Projects/actionbook/actionbook/packages/cli/src/cli.rs:96-245`
- **postagent send 当前实现**：`/Users/zhangalex/Work/Projects/actionbook/postagent/packages/postagent-core/src/commands/send.rs:12-136`
- **active-research 当前 skill**：`/Users/zhangalex/.claude/skills/active-research/SKILL.md`
- **agent-spec CLI**：`~/.cargo/bin/agent-spec` v0.2.7
