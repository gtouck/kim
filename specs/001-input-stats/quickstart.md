# Quickstart: 开发者快速上手指南

**Project**: key-input-monitor (`kim`) | **Branch**: `001-input-stats` | **Date**: 2026-03-06

---

## 前置条件

| 工具 | 最低版本 | 安装方式 |
|------|---------|---------|
| Rust (rustup) | stable (≥ 1.75) | https://rustup.rs |
| Windows SDK | Windows 10/11 | 随 Visual Studio Build Tools 安装 |
| Git | 任意 | https://git-scm.com |

> 注意：本项目**仅支持 Windows 10/11**，所有 WinAPI 钩子相关代码不可在 Linux/macOS 上编译通过。

---

## 快速启动

### 1. 克隆并构建

```powershell
git clone <repo-url> key-input-monitor
cd key-input-monitor

# 构建所有二进制（Debug 模式）
cargo build

# 构建发布版（优化 + 更小体积）
cargo build --release
```

构建成功后，二进制位于：
- `target\debug\kim.exe` — CLI 入口
- `target\debug\kimd.exe` — 后台 daemon

### 2. 运行测试

```powershell
# 运行所有测试（必须全部通过）
cargo test

# 运行带输出的测试（调试时）
cargo test -- --nocapture

# 检查 lint（0 警告要求）
cargo clippy -- -D warnings
```

### 3. 启动 daemon 并验证

```powershell
# 将构建目录添加到 PATH 或使用绝对路径

# 启动后台监控
.\target\debug\kim.exe start
# 输出: kim started (PID: xxxxx)

# 查询状态
.\target\debug\kim.exe status
# 输出: running  PID: xxxxx  uptime: 00:00:05

# 查看今日统计（等待几秒，敲几下键盘和鼠标点击）
.\target\debug\kim.exe today

# 停止
.\target\debug\kim.exe stop
# 输出: kim stopped
```

---

## 目录结构

```
key-input-monitor/
├── Cargo.toml              # 工作区配置（两个 binary target）
├── src/
│   ├── bin/
│   │   ├── kim/
│   │   │   └── main.rs     # CLI 入口，处理子命令
│   │   └── kimd/
│   │       └── main.rs     # Daemon 入口，启动所有线程
│   ├── hooks/
│   │   ├── mod.rs
│   │   ├── keyboard.rs     # WH_KEYBOARD_LL 钩子
│   │   ├── mouse.rs        # WH_MOUSE_LL 钩子
│   │   └── window.rs       # WinEventHook 窗口追踪
│   ├── ime/
│   │   └── mod.rs          # UIA TextChanged 字符计数
│   ├── db/
│   │   ├── mod.rs
│   │   ├── schema.rs       # SQL DDL 和迁移
│   │   └── writer.rs       # 批量写入逻辑（30s 定时器）
│   ├── stats/
│   │   ├── mod.rs
│   │   ├── counters.rs     # AtomicU64 全局计数器
│   │   ├── app_tracker.rs  # 应用维度统计
│   │   └── lang_tracker.rs # 语言专注时间追踪
│   └── cli/
│       ├── mod.rs
│       ├── today.rs        # kim today 输出格式化
│       ├── history.rs      # kim history
│       ├── apps.rs         # kim apps
│       └── langs.rs        # kim langs
├── tests/
│   ├── integration/        # 集成测试（使用临时 SQLite 文件）
│   └── unit/               # 单元测试（与模块同目录或此处）
└── specs/                  # 规格文档（不影响构建）
    └── 001-input-stats/
```

---

## 核心开发工作流（TDD）

**严格遵循 Red → Green → Refactor 循环**：

```powershell
# 1. 写失败的测试
# 2. 运行确认测试失败（Red）
cargo test <test_name> -- --nocapture

# 3. 实现最简可通过代码（Green）
# 4. 运行确认测试通过
cargo test

# 5. 重构（保持 Green）
# 6. 提交前确认 clippy 0 警告
cargo clippy -- -D warnings
cargo test
```

---

## 常见调试场景

### Daemon 不产生统计数据

```powershell
# 检查 daemon 是否真的在运行
kim status

# 查看日志
Get-Content "$env:LOCALAPPDATA\kim\kim.log" -Tail 50

# 检查 hook 是否被注册（Process Monitor 或 Spy++）
```

### SQLite 数据库问题

```powershell
# 数据库位置
$db = "$env:LOCALAPPDATA\kim\stats.db"
# 用 sqlite3 命令行检查（需单独安装 sqlite3.exe）
sqlite3 $db ".tables"
sqlite3 $db "SELECT * FROM daily_stats ORDER BY date DESC LIMIT 5;"
```

### 构建错误（Windows SDK 特性缺失）

```powershell
# 确认 windows crate features 是否包含所需 API
# 在 Cargo.toml 中添加缺失的 feature flag
# 参考: https://microsoft.github.io/windows-docs-rs/
```

---

## 数据文件位置

| 文件 | 路径 |
|------|------|
| SQLite 数据库 | `%LOCALAPPDATA%\kim\stats.db` |
| PID 文件 | `%LOCALAPPDATA%\kim\kimd.pid` |
| 日志文件 | `%LOCALAPPDATA%\kim\kim.log` |

---

## 关键约束提醒

1. **WinAPI 钩子线程**：钩子回调必须在 300ms 内返回，禁止任何 IO 或 Mutex 争用
2. **UIA COM 线程**：必须运行在 `STA`（`CoInitializeEx(COINIT_APARTMENTTHREADED)`），不可与其他线程混用
3. **隐私保护**：检测到密码字段时，字符计数禁止增加（键击计数正常）
4. **TDD 强制**：任何功能代码必须先有失败测试，再写实现

---

## 相关文档

- [spec.md](./spec.md) — 完整功能需求规格
- [research.md](./research.md) — 技术选型依据
- [data-model.md](./data-model.md) — 数据库结构与实体定义
- [contracts/cli.md](./contracts/cli.md) — CLI 命令接口规范
