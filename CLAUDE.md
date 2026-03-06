# CLAUDE.md — 个人 Agentic Engineering 工作手册

> 本文件是 Claude Code 的全局行为指导，放置于 `~/.claude/CLAUDE.md`（全局生效）。
> 各项目根目录可再放置局部 `CLAUDE.md` 覆盖特定规则。
> 指导思想来源：Simon Willison 的 Agentic Engineering Patterns。

---

## 一、基本原则

### 代码是廉价的，好代码不是

在提出任何实现方案前，先问自己：这段代码是否满足以下全部条件？

- [ ] **能正确运行**，没有已知 bug
- [ ] **经过验证**，不是靠猜测它能工作
- [ ] **解决了正确的问题**，而不是表面问题
- [ ] **处理了错误路径**，不只考虑 happy path
- [ ] **足够简单**，只做需要的事，未来可维护

**对 Claude 的指令**：生成代码后，不要直接宣称"完成了"。
必须说明：你是否运行了它？输出是什么？有哪些边界情况未处理？

---

## 二、新 Session 开场协议

每次启动新的 Claude Code session，**第一条指令固定为**：

### 有测试的项目（Rust/CLI、Java 后端）

```
Run the tests first. Show me the test count and any failures before doing anything else.
```

- Rust: `cargo test`
- Java: `./mvnw test` 或 `./gradlew test`（按项目实际命令）

### 无测试的项目（React Web / Tauri 前端）

```
Before starting, read the existing code structure and tell me:
1. What the entry points are
2. What the main data flows look like
3. What error handling patterns are currently used
Then we can proceed.
```

这个动作的目的：
- 让 agent 建立项目上下文，而不是盲目开始
- 在有测试时，确保后续改动不会悄悄破坏已有功能
- 在无测试时，强迫 agent 先理解再行动

---

## 三、按场景的 Prompt 规范

### 3.1 新功能开发

**标准流程（有测试项目：Rust / Java）**：

```
# Step 1 - 确认现状
Run the tests. [项目测试命令]

# Step 2 - Red/Green TDD
Write the tests for [功能描述] first.
Confirm they fail before writing any implementation.
Then implement until the tests pass.
Do not write more code than what's needed to pass the tests.

# Step 3 - 收尾
Run all tests again. Summarize what changed and why.
```

**无测试项目的替代方案（React / Tauri）**：

```
Implement [功能描述].
After implementation, write at least one smoke test that proves the core behavior works.
Run it and show me the output.
```

**禁止的 Prompt 模式**：
- ❌ `帮我实现 XX 功能` （没有验收标准）
- ❌ `写完之后你觉得对吗？` （让 agent 自评没有意义）
- ✅ `实现 XX，完成后运行测试/给我一个可执行的验证步骤`

---

### 3.2 调试 & 排查问题

**标准流程**：

```
# 描述症状，不要描述你猜测的原因
The symptom is: [具体的错误信息 / 异常行为]
It happens when: [复现步骤]
It does NOT happen when: [对比情况，如果有]

Do NOT jump to solutions. First tell me your hypothesis about the root cause,
then we'll agree on a fix approach before you write any code.
```

**关键约束**：要求 agent 先给出假设再动手，防止它在错误方向上堆砌代码。

---

### 3.3 遗留代码理解 / 重构

**先做线性漫步（Linear Walkthrough）**：

```
Give me a linear walkthrough of [文件名 / 模块名].
Walk through the code as if explaining it to someone who has never seen it.
Cover: entry points → data flow → side effects → error handling → anything surprising.
Do NOT suggest improvements yet. Just explain what's there.
```

**然后再决定是否重构**：

```
Based on your walkthrough, identify the top 3 risks if we need to modify this code.
Then propose the minimal refactor that reduces those risks.
```

**重构的约束**：
- 重构前必须有能通过的测试（如果没有，先补测试）
- 每次重构步骤要小，完成后运行测试再继续
- Rust / Java 项目：测试必须全绿才能提交

---

## 四、技术栈特定规则

### React / Tauri 前端（无测试）

```
# 每次修改组件后检查
Check: does this component have any props that could be undefined at runtime?
Check: are all async operations handling loading and error states?
Do not add new dependencies without asking me first.
```

### Java 后端（有测试）

```
# 新接口 / 服务类，必须：
1. Run existing tests first
2. Write new tests before implementation (TDD)
3. Follow existing error handling patterns in the project
4. Use the same logging framework already in use
If you need a new library, tell me what it does and why existing ones won't work.
```

### Rust / CLI（有测试）

```
# 每次修改后必须执行
cargo clippy -- -D warnings
cargo test
If either fails, fix it before considering the task done.
Prefer returning Result<> over unwrap/expect in non-test code.
```

---

## 五、多工具协作分工

> 我同时使用 Claude Code CLI 和 Cursor/Copilot，需明确边界，避免互相干扰。

| 工具 | 适用场景 | 避免场景 |
|------|----------|----------|
| **Claude Code CLI** | 跨文件重构、需要执行命令验证、遗留代码线性漫步、生成 PoC | 单行补全、快速语法修正 |
| **Cursor / Copilot** | 行内补全、局部函数生成、快速迭代 UI 细节 | 涉及业务逻辑的大段生成（认知债务风险高） |
| **claude.ai 对话** | 方案设计讨论、技术调研、prompt 设计 | 直接生成要提交的代码（没有执行验证） |

**关键原则**：Cursor/Copilot 生成的代码如果超过 50 行或涉及核心业务逻辑，必须切换到 Claude Code 做一次线性漫步验证，再提交。

---

## 六、认知债务管理

> 当 agent 生成了我看不懂的核心代码时，必须触发线性漫步，而不是直接合并。

**触发条件**（满足任一即触发）：
- 生成超过 100 行的新文件
- 涉及并发、异步、状态管理的实现
- 用了我不熟悉的库或模式
- 卫星协议相关的业务逻辑
- Cursor/Copilot 生成的超过 50 行的逻辑代码

**触发动作**：

```
Before I accept this code, walk me through it line by line.
Explain every non-obvious decision. Flag anything that could surprise a future maintainer.
```

---

## 七、不允许 Agent 做的事

以下操作 **必须暂停并询问我**，不得自行决定：

- 修改数据库 schema 或 migration 文件
- 更改任何与卫星指令/遥测相关的核心解析逻辑
- 添加新的外部依赖（npm package / cargo crate / maven artifact）
- 删除任何文件
- 修改 CI/CD 配置
- 生成超过 200 行的单个文件（先拆分设计）

---

## 八、Session 结束检查清单

每次结束前，要求 agent 执行：

```
Before we finish:
1. Run the tests one more time and confirm they pass
2. List every file you modified
3. Is there anything you're uncertain about in what you built?
4. What's the most likely thing to break in production?
```

---

## 九、异步 Agent 使用原则

> 对应"Writing code is cheap now"——大胆使用异步 session 探索可行性。

**适合异步 agent 的任务**：
- 探索一个不确定的技术方向（10 分钟后看结果）
- 生成 boilerplate / scaffold
- 为 React/Tauri 项目补充测试（目前缺失）
- 将已有 PoC 迁移到新项目

**异步 agent 的验收标准**（事后检查）：
- 代码能编译/运行
- 有基本的错误处理
- 没有引入新依赖（除非我明确要求）

---

*最后更新：2026-03*
*基于：Agentic Engineering Patterns by Simon Willison*
