# kim — Key Input Monitor

> Windows 后台输入统计工具。静默监控键盘敲击、鼠标点击和输入法字符提交，通过命令行查询每日、历史及按应用维度的统计数据。

[![Rust](https://img.shields.io/badge/language-Rust-orange)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-blue)](https://www.microsoft.com/windows)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

---

## 功能特性

- **键盘统计**：记录每日键盘总敲击次数
- **鼠标统计**：记录每日鼠标左键、右键、中键点击次数
- **打字字符数**：统计实际输入的字符数（中文输入法提交字符 + 英文/数字/标点直接输入），排除功能键、粘贴内容及密码字段
- **剪贴板操作**：统计每日 Ctrl+C（复制）和 Ctrl+V（粘贴）次数
- **按应用统计**：以进程名（如 `Code.exe`、`chrome.exe`）为维度统计各应用的输入量
- **编程语言统计**：通过活动窗口标题解析文件扩展名，追踪各编程语言的专注时间
- **历史查询**：支持查询最近 N 天（最多 30 天）的历史统计数据
- **开机自启**：可选的开机自启功能（默认关闭）
- **隐私保护**：密码输入字段不计入打字字符数；所有数据本地存储，不上传任何外部服务

---

## 系统要求

| 依赖 | 最低版本 |
|------|---------|
| Windows | 10 / 11（x86_64） |
| Rust | stable ≥ 1.75 |
| Windows SDK | Windows 10/11（随 Visual Studio Build Tools 安装） |

> **注意**：本工具**仅支持 Windows**，依赖 WinAPI 低级钩子（`WH_KEYBOARD_LL` / `WH_MOUSE_LL`）和 UI Automation，无法在 Linux/macOS 上编译或运行。

---

## 快速开始

### 构建

```powershell
git clone <repo-url> key-input-monitor
cd key-input-monitor

# Debug 构建
cargo build

# Release 构建（推荐，体积更小、性能更优）
cargo build --release
```

构建成功后二进制文件位于：
- `target\release\kimd.exe` — 后台监控 daemon（无控制台窗口）
- `target\release\kim.exe` — 命令行查询工具

### 启动监控

```powershell
# 启动后台 daemon
kim start
# 输出: kim started (PID: 12345)

# 查看 daemon 状态
kim status
# 输出: running  PID: 12345  uptime: 00:05:32

# 查看今日统计
kim today
```

### 停止监控

```powershell
kim stop
# 输出: kim stopped
```

---

## 命令参考

### `kim start`

启动后台监控 daemon。若 daemon 已在运行则提示并退出。

```
kim start
```

### `kim stop`

安全停止后台 daemon（数据落盘后退出，最长等待 5 秒）。

```
kim stop
```

### `kim status`

查询 daemon 运行状态及运行时长。

```
kim status
# 运行中: running  PID: 12345  uptime: 03:24:18
# 未运行: stopped
```

### `kim today`

显示今日输入统计摘要。

```
kim today

┌─────────────────────────────────────────────┐
│  今日输入统计  2026-03-06                    │
├─────────────────┬───────────────────────────┤
│ 键盘敲击次数    │ 12,345                    │
│ 鼠标点击次数    │  1,234                    │
│ 打字字符数      │  8,901                    │
│ 复制 (Ctrl+C)   │     45                    │
│ 粘贴 (Ctrl+V)   │     38                    │
└─────────────────┴───────────────────────────┘
（数据最后更新: 14:32:05，更新间隔 ≤30s）
```

支持 `--json` 输出用于脚本集成：

```
kim today --json
```

### `kim history`

查询历史统计数据。

```powershell
# 查看最近 7 天（默认）
kim history

# 查看最近 14 天
kim history --days 14

# 查看指定日期
kim history 2026-03-05

# 查看昨天
kim history yesterday
```

### `kim apps`

按应用（进程名）查看今日输入统计。

```powershell
kim apps
kim apps --date 2026-03-05
kim apps --days 7
```

### `kim langs`

查看编程语言专注时间统计（通过窗口标题文件扩展名推断）。

```powershell
kim langs
kim langs --days 30
```

### `kim autostart`

管理开机自启（默认关闭）。

```powershell
# 启用开机自启
kim autostart enable

# 禁用开机自启
kim autostart disable

# 查看当前状态
kim autostart status
```

---

## 数据存储

统计数据持久化至本地 SQLite 数据库：

```
%LOCALAPPDATA%\kim\stats.db
```

- 使用 WAL 模式，防止崩溃时写入冲突
- 每 30 秒批量写入一次（最多丢失 30 秒内的计数）
- 保留最近 30 天历史数据

---

## 架构概览

工具由两个独立可执行文件组成：

```
kimd.exe  ←  后台 daemon（无窗口，持续运行）
 ├── 钩子线程：WH_KEYBOARD_LL / WH_MOUSE_LL
 ├── UIA COM 线程：TextChanged 事件 → IME 字符计数
 ├── 事件处理线程：AtomicU64 计数器更新
 └── 写入线程：每 30s 批量写入 SQLite

kim.exe   ←  CLI 工具（与 daemon 共享同一 SQLite）
 └── 子命令 → 直接查询 SQLite → 格式化输出
```

**性能目标**：
- CPU 占用：空闲时 < 2%，活跃输入时 < 5%
- 内存占用：< 50 MB
- 钩子回调延迟：< 300ms
- CLI 查询响应：< 1s
- 连续运行 8 小时内存增长 < 50%

---

## 开发

### 运行测试

```powershell
# 运行所有测试（必须全部通过）
cargo test

# 带输出的测试（调试时）
cargo test -- --nocapture

# Lint 检查（0 警告要求）
cargo clippy -- -D warnings
```

### 项目结构

```
src/
├── bin/
│   ├── kim/main.rs         # CLI 入口
│   └── kimd/main.rs        # Daemon 入口
├── hooks/
│   ├── keyboard.rs         # WH_KEYBOARD_LL 钩子
│   ├── mouse.rs            # WH_MOUSE_LL 钩子
│   └── window.rs           # 窗口焦点追踪
├── ime/mod.rs              # UIA TextChanged 字符计数
├── db/
│   ├── schema.rs           # SQL DDL
│   └── writer.rs           # 批量写入逻辑
├── stats/
│   ├── counters.rs         # AtomicU64 全局计数器
│   ├── app_tracker.rs      # 应用维度统计
│   └── lang_tracker.rs     # 语言专注时间追踪
└── cli/
    ├── today.rs            # kim today
    ├── history.rs          # kim history
    ├── apps.rs             # kim apps
    └── langs.rs            # kim langs
tests/
└── integration/            # 集成测试（使用临时 SQLite 文件）
```

---

## 许可证

MIT
