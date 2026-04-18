spec: task
name: "source-route-domain-preset"
inherits: project
tags: [actionbook, cli, routing, research-workflow, phase-4, discovery]
estimate: 0.5d
depends: [actionbook-source-route]
---

## 意图

`actionbook source route` 当前的 5 条规则（HN / GitHub × 2 / arXiv × 1）本质是针对
"Rust + AI + tech" 研究者的路由集。法律、医疗、金融等领域的研究者使用相同命令,所有
URL 都会落到 `browser-fallback` —— 他们拿不到任何 API 加速价值。

本任务**不实装**新功能,只做**可行性调研**(discovery spec):盘点 3 个候选领域各自的
权威源能否做 preset,最后决定:

- **A)** Rust 实装 `source route --domain <preset>`(内置 preset 够用)
- **B)** 走 skill 变体(每领域独立 SKILL.md + 各自 hardcode 源)
- **C)** 开放用户配置文件(`~/.actionbook/domains/*.toml`)
- **D)** 当前单 preset 已足够,本 task 关闭

决定依据:每个候选领域盘点"有公开/匿名 HTTP API 的权威源数量"。若至少 2 个领域
每个都有 ≥ 3 条 API 可用,走 A;若只有 1 个新领域能凑出 preset,走 B;若源都需要
token/OAuth,走 C 或 D。

本 task 的产出是一份**评估报告**(写到 `research-api-adapter/reports/domain-preset-feasibility.md`),
不是代码、不是新命令、不是 skill 变体。

## 已定决策

- 3 个候选领域(选型理由:覆盖"技术相关但非 Rust"、"学术非 AI"、"非技术/非学术"):
  - **Web 前端开发** (React/Vue/Next/TS 生态)
  - **生物医学学术** (PubMed / bioRxiv / ClinicalTrials)
  - **美国法律** (SCOTUS 判决 / Congress.gov / CourtListener)
- 每个领域盘点至少 5 个候选权威源,对每个记录:
  - URL 模式(用于 Rust 枚举 match)
  - 是否有公开 JSON/XML API
  - 是否需要 token / OAuth / 邮箱注册
  - API rate limit(匿名访问是否受限)
  - 响应数据质量(返回结构化数据 vs HTML scrape 结果)
- 用**已有的 `actionbook source route`** 对 10 个各领域代表 URL 跑一遍,观察当前默认
  是否真的全部 fallback(证实问题确实存在)
- **不**提出 preset 配置文件格式(那是决定走 C 路线之后的事)
- **不**做 "Rust/AI preset" vs 其他 preset 的代码分离(那是走 A 路线之后的事)
- 评估报告是**决策输入**,不是决策本身——每个路线 A/B/C/D 在报告结尾标明 cost/benefit

## 边界

### 允许修改
- research-api-adapter/reports/domain-preset-feasibility.md(新建)
- research-api-adapter/specs/source-route-domain-preset.spec.md(本 spec)
- 本 spec 的 RETROSPECTIVE 段落追加一条"discovery 结果" note(完成后)

### 禁止做
- 不新建 Rust 代码
- 不改 `actionbook source route` 的规则集
- 不新建 SKILL.md 变体
- 不做配置文件 infra
- 不写 TOML schema
- 不把候选 preset 提前实装到 `rules.rs`
- 不跨领域混合 preset(e.g. "Rust + Medical" 双开)

## 完成条件

场景: 每个候选领域至少 5 个源被盘点
  测试:
    包: research-api-adapter
    过滤: 人工审计 reports/domain-preset-feasibility.md
  层级: docs
  假设 报告已写成
  当 数 Web 前端 / 生物医学 / 美国法律三段里的源条目
  那么 每段 >= 5 条
  并且 每条至少含字段 {url_pattern, api_available, auth_required, rate_limit_anon, response_shape}

场景: 当前 `source route` 行为被现场验证
  测试:
    包: research-api-adapter
    过滤: bash reports 附录
  层级: integration
  假设 报告附录列出 10 个各领域代表 URL
  当 对每条 URL 跑 `actionbook source route --json`
  那么 至少 8 条返回 `executor: "browser"`(证明当前 router 对这些领域确实无效)
  并且 未返回 `executor: "browser"` 的那 2 条要记录理由(意外匹配了现有规则)

场景: 报告结尾给出明确路线建议
  测试:
    包: research-api-adapter
    过滤: 人工审计
  层级: docs
  命中: reports 结尾段落
  当 读 "Recommendation" 段
  那么 明确写 "proceed with route A/B/C/D"
  并且 有 cost 估算(实装天数)
  并且 有 benefit 估算(覆盖用户 vs 维护成本)
  并且 若选 D(本 task 关闭不再做),要给出为何无法做 preset 的技术原因

场景: 报告区分 "匿名可用" 和 "注册即可用" 和 "付费"
  测试:
    包: research-api-adapter
    过滤: 人工审计表格
  层级: docs
  假设 每条源有 auth_required 字段
  当 审查表格
  那么 字段取值限定在 {none, email-registration, oauth, paid}
  并且 "paid" 和 "oauth" 类源**不进任何 preset**(与 `postagent-anonymous-flag` 约束一致)

## 排除范围

- 真正实装 preset / 变体 / 配置文件(本 task 只产出决策依据)
- 调研超过 3 个领域(宁少不滥,3 个足够给出方向性判断)
- 非 HTTP 协议(FTP、gRPC、GraphQL 之类暂不考虑)
- 非英文源(中文法律、日文学术等)——本轮聚焦英文源
- 源的**内容质量评估**(返回数据是否"好用"是另一码事,本 task 只问"API 是否存在")
- 对 `active-research` 现有 5 个源规则的重新评估(那些留在原位,本 task 只看新领域)
- 和 AI agent 调用方式对接(e.g. "Claude 怎么知道自己处于法律研究模式")——那是决定路线后的问题
