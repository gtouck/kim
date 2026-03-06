# Implementation Plan: 用户输入统计工具

**Branch**: `001-input-stats` | **Date**: 2026-03-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-input-stats/spec.md`

## Summary

构建一个名为 `kim`（Key Input Monitor）的 Windows 后台输入统计工具。工具以两个可执行文件形式交付：`kimd.exe`（无窗口 daemon，持续监控键盘/鼠标/IME 输入）和 `kim.exe`（CLI 接口，用于启动/停止 daemon 及查询统计数据）。数据持久化至本地 SQLite 数据库，支持按日、按应用、按编程语言三个维度查询历史统计，并提供开机自启功能。

技术路线：Rust + `windows` crate 低级钩子 + UIA 文本事件 + `rusqlite`（WAL 模式）+ 4 线程并发模型（钩子线程、事件处理线程、UIA COM 线程、写入线程）。

## Technical Context

**Language/Version**: Rust stable (≥ 1.75)
**Primary Dependencies**: `windows = "0.56"` (windows-rs，WinAPI 绑定) · `rusqlite = "0.31"` (bundled) · `crossbeam-channel = "0.5"` · `clap = "4"` (derive) · `chrono = "0.4"` · `log + simplelog = "0.12"`
**Storage**: SQLite 单文件 (`%LOCALAPPDATA%\kim\stats.db`)，WAL 模式
**Testing**: `cargo test`（TDD 强制要求，先写测试再实现）
**Target Platform**: Windows 10/11，x86\_64
**Project Type**: CLI daemon + 查询工具（双二进制）
**Performance Goals**: 事件统计延迟 < 1s；钩子回调 < 300ms；CLI 查询响应 < 1s；连续运行 8h 内存增长 < 50%
**Constraints**: CPU < 2% 空闲 / < 5% 活跃；内存 < 50MB；钩子回调 300ms 超时约束；数据最大丢失 30s
**Scale/Scope**: 单用户本地工具，30 天历史数据保留，数据量每日约数千行事件

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### 初次检查（Phase 0 前）

| 原则 | 状态 | 说明 |
|------|------|------|
| **I. Privacy by Design** | ✅ PASS | 所有数据本地 SQLite，不传输外部；密码字段检测（UIA IsPassword）排除字符计数；spec 明确定义了隐私保护边界 |
| **II. Minimal Footprint** | ✅ PASS | CPU < 2%/5% 目标已写入 SC-005/SC-007；AtomicU64 无锁计数确保钩子路径零阻塞；4 线程模型避免线程过多 |
| **III. Reliability & Correctness** | ✅ PASS | AtomicU64 保证零计数丢失；WAL 模式防写冲突；钩子线程 try_send 防超时阻塞；UIA 回退策略已定义 |
| **IV. Test-First Development** | ✅ PASS | Constitution 要求 TDD；quickstart.md 明确 Red-Green-Refactor 工作流；所有模块均需先测试再实现 |
| **V. Simplicity** | ✅ PASS | 无 async runtime（tokio 排除）；无 serde（直接 SQL 绑定）；无 regex（字符串操作解析标题）；单 SQLite 文件 |

### 复查（Phase 1 设计后）

| 原则 | 状态 | 说明 |
|------|------|------|
| **I. Privacy by Design** | ✅ PASS | data-model.md 中密码字段排除路径清晰；contracts/cli.md 无日志记录明文输入 |
| **II. Minimal Footprint** | ✅ PASS | 4 线程模型轻量；内存计数器为 AtomicU64（8 字节/计数器）；写入线程 sleep 30s 无忙等 |
| **III. Reliability & Correctness** | ✅ PASS | 数据库写入为 UPSERT（ON CONFLICT DO UPDATE）保证幂等性；WAL 模式防止崩溃丢数据 |
| **IV. Test-First Development** | ✅ PASS | 模块拆分（hooks/ime/db/stats/cli）每个模块均支持单元测试 |
| **V. Simplicity** | ✅ PASS | 两个 binary 职责单一；CLI 直接读 SQLite 无 IPC 协议；数据模型 3 张主表结构清晰 |

**结论**: 所有 5 项原则均通过，无 Complexity Tracking 违规需登记。

## Project Structure

### Documentation (this feature)

```text
specs/001-input-stats/
├── plan.md              # 本文件
├── research.md          # Phase 0 技术研究结果
├── data-model.md        # Phase 1 数据模型定义
├── quickstart.md        # Phase 1 开发者上手指南
├── contracts/
│   └── cli.md           # Phase 1 CLI 接口规范
└── tasks.md             # Phase 2 输出（/speckit.tasks 命令生成，本命令不创建）
```

### Source Code (repository root)

```text
src/
├── bin/
│   ├── kim/
│   │   └── main.rs          # CLI 入口：解析子命令，连接 SQLite，格式化输出
│   └── kimd/
│       └── main.rs          # Daemon 入口：初始化线程，注册钩子，监控运行
├── hooks/
│   ├── mod.rs
│   ├── keyboard.rs          # WH_KEYBOARD_LL 钩子：键击计数，Ctrl+C/V 检测
│   ├── mouse.rs             # WH_MOUSE_LL 钩子：鼠标点击计数
│   └── window.rs            # EVENT_SYSTEM_FOREGROUND + EVENT_OBJECT_FOCUS 追踪
├── ime/
│   └── mod.rs               # UIA STA 线程：TextChanged 事件 → 字符计数
├── db/
│   ├── mod.rs
│   ├── schema.rs            # 建表 SQL，schema 迁移
│   └── writer.rs            # 30s 定时写入线程，graceful shutdown flush
├── stats/
│   ├── mod.rs
│   ├── counters.rs          # GlobalCounters (AtomicU64)
│   ├── app_tracker.rs       # AppCounterMap (HashMap + Mutex)
│   └── lang_tracker.rs      # LanguageFocusTracker，5s 阈值过滤
└── cli/
    ├── mod.rs
    ├── today.rs             # kim today 表格渲染
    ├── history.rs           # kim history 列表渲染
    ├── apps.rs              # kim apps 排行表格
    └── langs.rs             # kim langs 专注时间格式化

tests/
├── integration/
│   ├── db_writer_test.rs    # 写入线程集成测试（tmpfile SQLite）
│   ├── cli_output_test.rs   # CLI 命令输出格式验证
│   └── autostart_test.rs    # 注册表读写（使用临时测试键）
└── unit/                    # 各模块单元测试（与 src/ 中 #[cfg(test)] 互补）
```

**Structure Decision**: 采用单 Cargo workspace，双 binary target（`kim` + `kimd`）。代码按功能层（hooks/ime/db/stats/cli）组织为独立模块，而非按实体组织。理由：各功能层依赖关系单向（cli → db → stats ← hooks），便于独立测试和替换实现。

## Complexity Tracking

> 无 Constitution Check 违规，仅记录有意识的复杂度决策供审查。

| 决策 | 为何需要 | 更简单替代方案被排除的原因 |
|------|----------|--------------------------|
| 双 binary（kim + kimd） | daemon 需要 `windows_subsystem = "windows"`，CLI 需要控制台输出 | 单 binary + 启动参数会导致 CLI 命令在 windowless 子系统下无输出 |
| UIA COM 独立 STA 线程 | UIA 接口要求 STA 线程模型，不可在钩子回调线程调用 | 合并线程会导致 COM 死锁，无法规避 |
| 4 线程模型（钩子/处理/UIA/写入） | 钩子线程 300ms 超时限制；UIA STA 要求；写 IO 不阻塞计数 | 更少线程会导致超时风险（Windows 自动卸载钩子）或 IO 阻塞计数路径 |
| `try_send` 在 channel 满时静默丢弃事件 | 钩子回调必须在 300ms 内返回，不可阻塞等待消费者 | Constitution §III "正常负载下零丢失"：1024 event bounded buffer 在 < 300 WPM 下永远不会溢出；极端负载下宪法允许 graceful degradation，优于钩子超时被 Windows 自动卸载 |
