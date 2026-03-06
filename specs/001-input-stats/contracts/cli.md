# CLI Contract: `kim` 命令行接口

**Tool**: `kim` / `kimd` | **Branch**: `001-input-stats` | **Date**: 2026-03-06
**Platform**: Windows 10/11 | **Binary**: `kim.exe`（CLI）+ `kimd.exe`（Daemon）

---

## 概述

`kim`（Key Input Monitor）是一个单一二进制 CLI 工具，通过子命令控制后台 daemon 并查询统计数据。所有监控能力由 `kimd.exe` 提供，`kim.exe` 作为用户交互入口。

---

## 全局选项

```
kim [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -h, --help       打印帮助信息
    -V, --version    打印版本号

SUBCOMMANDS:
    start       启动后台监控 daemon
    stop        安全停止后台 daemon（数据落盘后退出）
    status      查询 daemon 运行状态
    today       显示今日输入统计
    history     显示指定日期的统计
    apps        显示按应用分组的统计
    langs       显示编程语言输入统计
    autostart   管理开机自启设置
```

---

## 子命令详细规范

### `kim start`

启动后台监控 daemon。

```
USAGE:
    kim start

BEHAVIOR:
    1. 检查 PID 文件是否存在且进程存活
       - 若已在运行：输出 "kim is already running (PID: <pid>)" 并以退出码 1 退出
    2. 启动 kimd.exe（DETACHED_PROCESS，无控制台窗口）
    3. 等待最多 2 秒确认 PID 文件创建成功
    4. 输出 "kim started (PID: <pid>)"
    5. 以退出码 0 退出，命令提示符立即返回

EXIT CODES:
    0  成功启动
    1  已在运行
    2  启动失败（kimd.exe 不存在或权限问题）
```

**示例输出**:
```
$ kim start
kim started (PID: 12345)
```

---

### `kim stop`

安全停止后台 daemon。

```
USAGE:
    kim stop

BEHAVIOR:
    1. 读取 PID 文件，若不存在则输出 "kim is not running" 并退出码 1
    2. 向 daemon 发送停止信号（Named Event: "Local\kim-stop-event"）
    3. 等待最多 5 秒
    4. 若 daemon 在 5 秒内退出：输出 "kim stopped"，退出码 0
    5. 若超时：强制终止进程（TerminateProcess），输出 "kim force-stopped"，退出码 0
    6. 删除 PID 文件

EXIT CODES:
    0  成功停止（含强制停止）
    1  daemon 未在运行
```

**示例输出**:
```
$ kim stop
kim stopped
```

---

### `kim status`

查询 daemon 运行状态。

```
USAGE:
    kim status

OUTPUT FORMAT:
    Running:  "running  PID: <pid>  uptime: <HH:MM:SS>"
    Stopped:  "stopped"

EXIT CODES:
    0  正在运行
    1  未在运行
```

**示例输出**:
```
$ kim status
running  PID: 12345  uptime: 03:24:18
```

---

### `kim today`

显示今日输入统计摘要。

```
USAGE:
    kim today [OPTIONS]

OPTIONS:
    --json    以 JSON 格式输出（用于脚本集成）

OUTPUT FORMAT (默认表格):
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

OUTPUT FORMAT (--json):
    {
      "date": "2026-03-06",
      "keystrokes": 12345,
      "mouse_clicks": 1234,
      "characters": 8901,
      "ctrl_c": 45,
      "ctrl_v": 38,
      "last_updated": "2026-03-06T14:32:05"
    }

EXIT CODES:
    0  成功
    1  daemon 未运行（显示提示信息，提示执行 kim start）
    2  数据库访问失败
```

---

### `kim history`

查询历史日期的统计数据。

```
USAGE:
    kim history [DATE] [OPTIONS]

ARGS:
    DATE    目标日期，格式 YYYY-MM-DD 或快捷词 yesterday/last-week
            省略时默认为昨日

OPTIONS:
    --days <N>    显示最近 N 天的汇总列表（1-30，默认 7）
    --json        JSON 格式输出

BEHAVIOR:
    - 若指定 DATE，显示该日单日统计（格式同 kim today）
    - 若指定 --days N，显示最近 N 天每日数据的对比列表

OUTPUT FORMAT (--days 7, 默认):
    最近 7 天统计
    ┌────────────┬──────────┬────────┬────────┬───────┬───────┐
    │ 日期       │ 键盘     │ 鼠标   │ 字符   │ 复制  │ 粘贴  │
    ├────────────┼──────────┼────────┼────────┼───────┼───────┤
    │ 2026-03-06 │ 12,345   │  1,234 │  8,901 │    45 │    38 │
    │ 2026-03-05 │ 10,201   │    987 │  7,650 │    30 │    25 │
    │ ...        │          │        │        │       │       │
    └────────────┴──────────┴────────┴────────┴───────┴───────┘

EXIT CODES:
    0  成功
    1  指定日期无数据（显示 "No data for <date>"）
    2  日期格式无效
    3  数据库访问失败
```

---

### `kim apps`

显示按应用（进程名）分组的当日输入统计。

```
USAGE:
    kim apps [OPTIONS]

OPTIONS:
    --date <DATE>   指定日期（YYYY-MM-DD），默认今日
    --top <N>       只显示前 N 个应用（默认 10）
    --json          JSON 格式

OUTPUT FORMAT:
    今日应用输入排行  2026-03-06
    ┌────────────────┬──────────┬────────┬───────┬───────┐
    │ 应用           │ 键盘敲击 │ 打字数 │ 复制  │ 粘贴  │
    ├────────────────┼──────────┼────────┼───────┼───────┤
    │ code           │  5,432   │  3,210 │    20 │    15 │
    │ chrome         │  3,210   │  2,100 │    18 │    12 │
    │ windowsterminal│  2,100   │  1,890 │     5 │     8 │
    └────────────────┴──────────┴────────┴───────┴───────┘
    （排序：按键盘敲击数降序）

EXIT CODES:
    0  成功
    1  当日无应用数据
    2  数据库访问失败
```

---

### `kim langs`

显示编程语言输入量与专注时间统计。

```
USAGE:
    kim langs [OPTIONS]

OPTIONS:
    --date <DATE>   指定日期（YYYY-MM-DD），默认今日
    --json          JSON 格式

OUTPUT FORMAT:
    今日编程语言统计  2026-03-06
    ┌────────────────┬────────┬─────────────┐
    │ 语言           │ 字符数 │ 专注时间    │
    ├────────────────┼────────┼─────────────┤
    │ Rust           │  3,210 │ 2h 34m      │
    │ TypeScript     │  1,890 │ 1h 12m      │
    │ Python         │    450 │   18m       │
    │ Other          │    210 │    5m       │
    └────────────────┴────────┴─────────────┘
    （排序：按专注时间降序）

EXIT CODES:
    0  成功
    1  当日无语言数据
    2  数据库访问失败
```

---

### `kim autostart`

管理开机自启设置。

```
USAGE:
    kim autostart <enable|disable|status>

SUBCOMMANDS:
    enable    写入注册表开机自启（HKCU\...\Run\kim）
    disable   删除注册表键，关闭自启
    status    查询当前自启状态

EXIT CODES:
    0  成功
    1  操作失败（权限不足或注册表错误）

示例输出:
    $ kim autostart enable
    Autostart enabled. kim will start automatically on next login.

    $ kim autostart status
    Autostart: enabled
    Path: "C:\Users\...\AppData\Local\Programs\kim\kimd.exe" --autostart
```

---

## 错误处理规范

| 错误场景 | 用户可见消息 | 退出码 |
|----------|-------------|--------|
| Daemon 未运行（查询命令）| `kim is not running. Start it with: kim start` | 1 |
| 数据库文件不存在 | `Database not found at <path>. Start kim to initialize.` | 2 |
| 数据库版本不兼容 | `Database version mismatch. Run: kim migrate` | 3 |
| 权限错误 | `Permission denied: <detail>` | 4 |
| 未知错误 | `Internal error: <detail>. Check log at %LOCALAPPDATA%\kim\kim.log` | 99 |

**禁止的行为**:
- 不得输出 Rust panic backtrace 给最终用户
- 不得在非 `--debug` 模式下输出内部错误堆栈

---

## 数据格式约束

- **日期格式**: 所有日期统一使用 `YYYY-MM-DD`（ISO 8601 子集）
- **时间格式**: 日志和 JSON 使用 `YYYY-MM-DDTHH:MM:SS`（本地时间，无时区后缀）
- **数字显示**: 表格中数字超过 999 时使用逗号分隔符（如 `12,345`）
- **专注时间显示**: `Xh Ym`（不足 1 分钟显示 `<1m`，不足 1 小时不显示 h 部分）
- **进程名显示**: 小写，去掉 `.exe` 后缀（如 `code.exe` → `code`）
