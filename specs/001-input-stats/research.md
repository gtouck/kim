# Research: 用户输入统计工具 (001-input-stats)

**Branch**: `001-input-stats` | **Date**: 2026-03-06

本文档记录 Phase 0 研究阶段对所有技术未知项的调查结果，所有 NEEDS CLARIFICATION 均已在此解决。

---

## 决策 1: Rust WinAPI 绑定 crate 选型

**决策**: 使用 `windows` crate（windows-rs，Microsoft 官方first-party）

**理由**: `winapi` crate 社区维护趋于停滞；`windows-rs` 是 Microsoft 官方维护，类型安全更完善，IDE 补全覆盖全，持续跟进 Windows SDK 更新。

**关键实现细节**:
- 在 `Cargo.toml` 中声明所需 feature，例如：`windows = { version = "0.56", features = ["Win32_UI_WindowsAndMessaging", "Win32_System_Registry", ...] }`
- WH_KEYBOARD_LL 和 WH_MOUSE_LL 钩子在**同一个独立钩子线程**上注册，该线程维持 `GetMessageW` 消息循环
- 钩子回调内部只做 `try_send` 将事件推入 `crossbeam-channel`，严格控制在 300ms 超时内返回

**替代方案排除**: `winapi` crate 维护停滞，不选用

---

## 决策 2: 全局键鼠钩子线程架构

**决策**: 单独的**钩子线程**持有键盘和鼠标低级钩子，通过 `crossbeam-channel` (bounded) 将事件发送至**事件处理线程**，由处理线程更新原子计数器

**理由**: 钩子回调必须在同一线程的消息循环上下文中运行，且必须在 300ms 内返回（否则 Windows 自动卸载钩子）；channel 模式将回调与业务逻辑解耦，保证回调零阻塞

**关键实现细节**:
```
┌─────────────────────────────────────────────────────────────┐
│  kimd 进程                                                   │
│                                                             │
│  [钩子线程]                                                  │
│   SetWindowsHookExW(WH_KEYBOARD_LL)                        │
│   SetWindowsHookExW(WH_MOUSE_LL)                           │
│   SetWinEventHook(EVENT_SYSTEM_FOREGROUND)                 │
│   GetMessageW() 消息循环                                    │
│       │ try_send(InputEvent) ──────→ bounded channel       │
│                                           │                 │
│  [事件处理线程]  ←─────────────────────────┘                │
│   recv() → update AtomicU64 counters                       │
│   UIA 文本变化回调 → characters AtomicU64++                 │
│                                                             │
│  [写入线程]                                                  │
│   每 30s: swap counters → INSERT INTO SQLite               │
│                                                             │
│  [CLI 进程 (kim.exe)]                                       │
│   直接打开同一 SQLite 文件进行只读查询                        │
└─────────────────────────────────────────────────────────────┘
```

**风险与规避**: channel 设置 bounded 大小 (如 1024)，回调使用 `try_send` 丢弃而非阻塞；事件处理线程应无 IO 操作

---

## 决策 3: IME 字符计数方案

**决策**: UIA `TextChanged` 事件（`UIA_Text_TextChangedEventId`）监听全局文本变化，通过新旧文本长度差推算已提交字符数

**理由**: WH_KEYBOARD_LL **无法捕获** IME 合成字符（IME 绕过低级钩子直接发 WM_CHAR）；WM_IME_COMPOSITION 需要 DLL 注入（32/64 位混合问题严重）；UIA TextChanged 是真正的跨进程无注入方案，Chrome/Edge/Win32/UWP 均有良好支持

**关键实现细节**:
- UIA 事件监听必须运行在 **STA COM 线程**（与钩子线程分离）
- 注册一次全局根元素事件监听器，不要每次焦点变化重新注册
- 非 IME 按键（VK 码非 `VK_PROCESSKEY`）直接在钩子回调中 `characters++`
- IME 按键触发的字符变化由 UIA 回调统计，避免重复计数

**回退策略**: 若某些应用 UIA provider 不暴露 TextChanged，该应用内的字符数降级为"仅键击数估算"，在报告中标注"近似值"

**替代方案排除**:
- WM_IME_COMPOSITION: 需要 DLL 注入，不可行
- 剪贴板监听: 只能检测粘贴，无法检测 IME 提交

---

## 决策 4: 密码字段检测

**决策**: UIA `UIA_IsPasswordPropertyId`（属性 ID 30019），在**焦点变化时**缓存查询结果（而非每次按键都查询）

**理由**: `UIA_IsPasswordPropertyId` 是跨框架标准，Chrome/Edge/WPF PasswordBox/UWP PasswordBox 均正确暴露此属性；`GetClassName` 检测 "PasswordBox" 仅覆盖 WPF/UWP 原生控件，对 Electron/Chrome 无效

**关键实现细节**:
- 监听 `EVENT_OBJECT_FOCUS`（WinEventHook），焦点变化时异步查询并更新 `AtomicBool IS_PASSWORD_FIELD`
- 键盘钩子回调直接读取该原子布尔变量（零开销）
- UIA 查询必须在 STA COM 线程，不在钩子回调直接调用

**风险与规避**: 部分旧版 Electron 应用 UIA provider 不完整，`IsPassword` 可能误报 false；此时保守策略是打字字符数不计入（已满足"密码字段不计打字数"的要求），键击数仍正常统计

---

## 决策 5: 活动窗口追踪

**决策**: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ...)` 事件驱动，在回调中调用 `GetWindowThreadProcessId → QueryFullProcessImageNameW + GetWindowTextW`

**理由**: 轮询 `GetForegroundWindow` 浪费 CPU；WinEventHook 是系统原生窗口切换通知，延迟 < 1ms，与钩子线程复用同一消息循环无额外线程开销

**关键实现细节**:
```
窗口标题解析策略（字符串操作，无需正则）：
  "main.rs - Visual Studio Code"
    → rsplitn(2, " - ") → ["Visual Studio Code", "main.rs"]
    → Path::extension("main.rs") → "rs"
    → 映射表查找 "rs" → "Rust"

聚焦时间阈值：连续聚焦 > 5 秒才计入编程语言专注时间
```

**已知扩展名→语言映射表（初始集合）**:
```
py→Python, js→JavaScript, ts→TypeScript, java→Java,
go→Go, rs→Rust, c→C, cpp→C++, cs→C#, rb→Ruby,
php→PHP, swift→Swift, kt→Kotlin, html→HTML, css→CSS,
sql→SQL, sh→Shell, vue→Vue, jsx→JSX, tsx→TSX
```

**风险与规避**: 标题快速切换（< 5s）不计入专注时间，避免毫秒级窗口切换产生噪声；`QueryFullProcessImageNameW` 对系统进程可能失败，用 `Option` 处理

---

## 决策 6: Daemon 进程架构

**决策**: **两个二进制目标**：`kim.exe`（console 子系统，CLI 入口）+ `kimd.exe`（windows 子系统，后台 daemon）；通过**命名事件（Named Event）**优雅通信

**理由**: `#![windows_subsystem = "windows"]` 标注在主 binary 会导致 CLI 输出失效；分开两个 target 架构最干净；Named Event `"Local\kim-stop-event"` 比 PID + `TerminateProcess` 更优雅，支持 graceful shutdown（flush 数据后退出）

**关键实现细节**:
```toml
[[bin]]
name = "kim"    # CLI，console 子系统
path = "src/bin/kim/main.rs"

[[bin]]
name = "kimd"   # Daemon，windows 子系统（无控制台窗口）
path = "src/bin/kimd/main.rs"
```

- PID 文件路径：`%LOCALAPPDATA%\kim\kimd.pid`
- SQLite 数据库路径：`%LOCALAPPDATA%\kim\stats.db`
- 停止流程：`kim stop` → SetEvent(`kim-stop-event`) → 等待 5s → 若进程仍存活则 TerminateProcess
  （注：超时时长以 contracts/cli.md 为权威，此处已与 CLI 规范对齐）

**流程图**:
```
kim start → 检查 PID 文件 → 若已运行报错 → 否则 spawn kimd.exe (DETACHED_PROCESS)
kim stop  → 读 PID 文件 → SetEvent(stop-event) → 等 graceful exit → 超时强杀
kim status → 读 PID 文件 → OpenProcess 检测是否存活 → 输出状态
```

**替代方案排除**:
- Windows Service: 需要管理员权限安装，过度工程化
- `GenerateConsoleCtrlEvent`: 仅对控制台进程有效，daemon 无控制台窗口故不适用

---

## 决策 7: 开机自启

**决策**: `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` 注册表键，用户级自启，无需管理员权限

**理由**: 用户级注册表自启是 Windows 最成熟的轻量自启方案，在任务管理器"启动"选项卡可见且可由用户禁用，符合用户预期；Task Scheduler 对此场景过度工程化

**关键实现细节**:
- 值名称：`kim`，值内容：`"C:\path\to\kimd.exe" --autostart`（路径含引号防空格问题）
- `--autostart` 标志让 daemon 延迟 3 秒启动（等待用户桌面完全加载）
- 禁用自启：删除对应注册表值 `RegDeleteValueW`

---

## 决策 8: 数据持久化

**决策**: `rusqlite`（bundled feature，静态链接）+ `AtomicU64` 内存计数器 + 独立**写线程**（`std::thread`，非 async）

**理由**: `sqlx` 引入 async runtime 对 30s 批量写入过度工程；`rusqlite` bundled 静态链接无运行时依赖；`AtomicU64::fetch_add` 使键盘事件处理路径完全无锁，满足 300ms 钩子超时约束

**关键实现细节**:
```
内存计数器（AtomicU64，fetch_add in hook callback）：
  keystrokes, mouse_clicks, characters, ctrl_c, ctrl_v

写线程（每 30s + 退出时）：
  swap(0) 读取并归零 → 开启 SQLite 事务 → INSERT/UPDATE

SQLite PRAGMA：
  journal_mode=WAL   -- 允许读写并发（CLI 查询不阻塞写入）
  synchronous=NORMAL -- 性能与安全的平衡点

应用维度计数：
  HashMap<ProcessName, AppCounters> 在事件处理线程中维护
  写入时按进程名 INSERT OR REPLACE INTO app_stats
```

---

## 推荐 Cargo.toml 核心依赖

```toml
[dependencies]
# Windows API 绑定
windows = { version = "0.56", features = [
    "Win32_UI_WindowsAndMessaging",          # SetWindowsHookExW, GetMessageW, WinEvent
    "Win32_UI_Accessibility",                # IUIAutomation, UIA properties
    "Win32_System_Threading",                # GetCurrentProcessId, OpenProcess, etc.
    "Win32_System_Registry",                 # RegSetValueExW, RegDeleteValueW
    "Win32_System_Diagnostics_ToolHelp",    # CreateToolhelp32Snapshot (可选)
    "Win32_Foundation",                      # HWND, BOOL, LPARAM, WPARAM, etc.
] }

# SQLite
rusqlite = { version = "0.31", features = ["bundled"] }

# 并发
crossbeam-channel = "0.5"

# CLI 解析
clap = { version = "4", features = ["derive"] }

# 日志
log = "0.4"
simplelog = "0.12"

# 时间处理
chrono = "0.4"

[dev-dependencies]
tempfile = "3"   # 测试用临时 SQLite 数据库
```

**刻意不引入**:
- `tokio` / `async-std`：无异步 IO 需求
- `serde`：数据结构简单，直接 SQL 参数绑定
- `regex`：窗口标题解析用字符串操作足够

---

## 整体线程模型汇总

```
kimd.exe 进程（4 个线程）：

Thread-1: 钩子+WinEvent 线程（STA）
  - SetWindowsHookExW(WH_KEYBOARD_LL)
  - SetWindowsHookExW(WH_MOUSE_LL)
  - SetWinEventHook(EVENT_SYSTEM_FOREGROUND)
  - SetWinEventHook(EVENT_OBJECT_FOCUS)
  - GetMessageW() 消息循环
  → 发送 InputEvent 到 bounded channel

Thread-2: 事件处理线程
  - recv() from channel
  - 更新 AtomicU64 全局计数器
  - 维护 HashMap<ProcessName, AppCounters>（含 Mutex）
  - 响应命名事件停止信号

Thread-3: UIA COM 线程（STA）
  - CoInitializeEx(COINIT_APARTMENTTHREADED)
  - AddAutomationEventHandler(TextChanged)
  → 字符计数 AtomicU64 fetch_add

Thread-4: 写入线程
  - sleep(30s) 循环
  - swap AtomicU64 计数器
  - 写入 SQLite（WAL 模式）
  - 收到停止信号时立即 final flush
```
