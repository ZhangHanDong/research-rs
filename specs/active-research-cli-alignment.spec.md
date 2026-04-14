spec: task
name: "active-research-cli-alignment"
inherits: project
tags: [active-research, skill, documentation, dependency-blocker]
estimate: 0.5d
---

## 意图

`~/.claude/skills/active-research/SKILL.md` 多处引用 `browser fetch`、`wait-idle`、
`browser batch` 等命令，但这些命令在当前 `packages/cli/src/cli.rs` 的 `BrowserCommands`
enum 中并不存在（已逐行核实 line 96-245）。本任务把 SKILL.md 中所有 `actionbook browser`
命令串修订为当前 CLI 真实存在的 subcommand，让 skill 本身建立在真实接口之上，为后续
`active-research-api-sources` 任务扫清基础。

最终产出是一份更新过的 SKILL.md，其中每一条命令都能在当前 CLI 的 `--help` 里找到对应。

## 已定决策

- 命令面真相源：`packages/cli/src/cli.rs` 的 `BrowserCommands` enum
- 移除规则：不允许出现 `browser fetch`
- 移除规则：不允许出现 `browser batch`
- 改写规则：`browser wait-idle` 替换为 `browser wait network-idle`
- 改写规则：`browser fetch <url> --format text` 替换为 `browser new-tab` → `browser wait network-idle` → `browser text` 三步序列
- 新增说明：在 "Navigation Pattern" section 末尾加一句 "当前 CLI 无 readability 模式，`browser text` 返回 innerText"

## 边界

### 允许修改
- /Users/zhangalex/.claude/skills/active-research/SKILL.md
- /Users/zhangalex/Work/Projects/actionbook/research-api-adapter/scripts/**

### 禁止做
- 不修改其他 skill 文件
- 不修改 `playground/deep-research/README.md`
- 不新增或删除 SKILL.md 的顶级 section
- 不给 `packages/cli` 补回 `browser fetch` 或 readability 命令

## 完成条件

场景: SKILL.md 中每条 browser 命令都对应真实 subcommand
  测试:
    包: research-api-adapter
    过滤: scripts/verify_skill_cli_alignment.sh
  层级: integration
  命中: ~/.claude/skills/active-research/SKILL.md, packages/cli/src/cli.rs
  假设 `packages/cli/src/cli.rs` 的 `BrowserCommands` enum 列举了全部合法 subcommand
  当 验证脚本扫描 SKILL.md 中所有 `actionbook browser <name>` 引用
  那么 脚本退出码为 "0"
  并且 stdout 输出 "all references match"

场景: 文件中不再出现 browser fetch
  测试:
    包: research-api-adapter
    过滤: scripts/assert_no_browser_fetch.sh
  当 验证脚本在 SKILL.md 中搜索 `browser fetch`
  那么 脚本退出码为 "0"
  并且 匹配数输出为 "0 occurrences"

场景: 文件中不再出现 browser batch
  测试:
    包: research-api-adapter
    过滤: scripts/assert_no_browser_batch.sh
  当 验证脚本在 SKILL.md 中搜索 `browser batch`
  那么 脚本退出码为 "0"
  并且 匹配数输出为 "0 occurrences"

场景: wait-idle 全部改写为 wait network-idle
  测试:
    包: research-api-adapter
    过滤: scripts/assert_wait_network_idle.sh
  层级: integration
  命中: ~/.claude/skills/active-research/SKILL.md
  当 验证脚本在 SKILL.md 中搜索 `wait-idle` 且前面不是 `wait `
  那么 脚本退出码为 "0"
  并且 匹配数输出为 "0 bare wait-idle occurrences"

场景: Navigation Pattern section 说明 innerText
  测试:
    包: research-api-adapter
    过滤: scripts/assert_readability_note.sh
  层级: integration
  命中: ~/.claude/skills/active-research/SKILL.md
  假设 SKILL.md 已经修订
  当 验证脚本在 "Navigation Pattern" section 内部搜索 "innerText"
  那么 脚本退出码为 "0"
  并且 stdout 输出 "innerText note present"

场景: 非法命令引用的错误路径
  测试:
    包: research-api-adapter
    过滤: scripts/verify_skill_cli_alignment.sh
  层级: integration
  命中: packages/cli/src/cli.rs, tmp fixture SKILL.md
  假设 一份临时 fixture SKILL.md 里人为写入无效的 `browser fetch http://example.com`
  当 验证脚本以该 fixture 为输入运行
  那么 脚本退出码为 "1"
  并且 stderr 输出包含 "unknown subcommand: fetch" 表示错误已被捕获

## 排除范围

- 修复其他 skill（deep-research / actionbook / actionbook-electron）的命令引用
- 给 `packages/cli` 补回 `browser fetch` / `browser batch` / readability 命令
- 新增 API-first sources section（见 active-research-api-sources spec）
- 修改 SKILL.md 中 "Topic Detection" 表的条目
