# Data Model: 用户输入统计工具 (001-input-stats)

**Branch**: `001-input-stats` | **Date**: 2026-03-06

本文档定义所有持久化实体、SQLite 表结构、内存中间态及状态转换规则。

---

## 一、持久化实体（SQLite 表）

### 1. `daily_stats` — 日统计记录

代表某一**日期**的全局汇总输入统计。

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| `date` | `TEXT` | PRIMARY KEY | ISO 8601 日期，格式 `YYYY-MM-DD`，基于本地时间 |
| `keystrokes` | `INTEGER` | NOT NULL DEFAULT 0 | 全局键盘敲击总次数（含所有按键类型） |
| `mouse_clicks` | `INTEGER` | NOT NULL DEFAULT 0 | 鼠标按键点击总次数（左/右/中键均计） |
| `characters` | `INTEGER` | NOT NULL DEFAULT 0 | 打字字符总数（IME 提交 + 直接按键可见字符，不含粘贴） |
| `ctrl_c_count` | `INTEGER` | NOT NULL DEFAULT 0 | Ctrl+C 快捷键执行次数 |
| `ctrl_v_count` | `INTEGER` | NOT NULL DEFAULT 0 | Ctrl+V 快捷键执行次数 |
| `updated_at` | `INTEGER` | NOT NULL | 最后一次写入的 Unix 时间戳（秒）|

**DDL**:
```sql
CREATE TABLE IF NOT EXISTS daily_stats (
    date        TEXT    NOT NULL,
    keystrokes  INTEGER NOT NULL DEFAULT 0,
    mouse_clicks INTEGER NOT NULL DEFAULT 0,
    characters  INTEGER NOT NULL DEFAULT 0,
    ctrl_c_count INTEGER NOT NULL DEFAULT 0,
    ctrl_v_count INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (date)
);
```

**写入模式**: `INSERT INTO daily_stats (...) VALUES (...) ON CONFLICT(date) DO UPDATE SET keystrokes = keystrokes + excluded.keystrokes, ...`

---

### 2. `app_stats` — 应用维度统计

代表某**日期 × 进程名**组合下的输入量统计。

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| `date` | `TEXT` | NOT NULL | ISO 8601 日期，外键关联 `daily_stats.date` |
| `process_name` | `TEXT` | NOT NULL | 可执行文件名（去掉路径和 .exe 后缀后小写化），如 `code`、`chrome` |
| `keystrokes` | `INTEGER` | NOT NULL DEFAULT 0 | 该应用的键盘敲击次数 |
| `characters` | `INTEGER` | NOT NULL DEFAULT 0 | 该应用的打字字符数 |
| `ctrl_c_count` | `INTEGER` | NOT NULL DEFAULT 0 | |
| `ctrl_v_count` | `INTEGER` | NOT NULL DEFAULT 0 | |
| `updated_at` | `INTEGER` | NOT NULL | 最后写入时间戳 |

**约束**: PRIMARY KEY (`date`, `process_name`)

**DDL**:
```sql
CREATE TABLE IF NOT EXISTS app_stats (
    date         TEXT    NOT NULL,
    process_name TEXT    NOT NULL,
    keystrokes   INTEGER NOT NULL DEFAULT 0,
    characters   INTEGER NOT NULL DEFAULT 0,
    ctrl_c_count INTEGER NOT NULL DEFAULT 0,
    ctrl_v_count INTEGER NOT NULL DEFAULT 0,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (date, process_name)
);
CREATE INDEX IF NOT EXISTS idx_app_stats_date ON app_stats(date);
```

---

### 3. `language_stats` — 编程语言维度统计

代表某**日期 × 语言名称**组合下的打字量与专注时长。

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| `date` | `TEXT` | NOT NULL | ISO 8601 日期 |
| `language` | `TEXT` | NOT NULL | 语言名称，如 `Rust`、`TypeScript`，无法识别时为 `Other` |
| `characters` | `INTEGER` | NOT NULL DEFAULT 0 | 该语言下的打字字符数 |
| `focus_seconds` | `INTEGER` | NOT NULL DEFAULT 0 | 专注时长（秒），仅统计连续聚焦 > 5s 的时段 |
| `updated_at` | `INTEGER` | NOT NULL | |

**约束**: PRIMARY KEY (`date`, `language`)

**DDL**:
```sql
CREATE TABLE IF NOT EXISTS language_stats (
    date          TEXT    NOT NULL,
    language      TEXT    NOT NULL,
    characters    INTEGER NOT NULL DEFAULT 0,
    focus_seconds INTEGER NOT NULL DEFAULT 0,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (date, language)
);
CREATE INDEX IF NOT EXISTS idx_lang_stats_date ON language_stats(date);
```

---

### 4. `schema_version` — Schema 版本控制

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `version` | `INTEGER` | Schema 版本号，当前为 1 |
| `applied_at` | `INTEGER` | 首次建库时间戳 |

**DDL**:
```sql
CREATE TABLE IF NOT EXISTS schema_version (
    version    INTEGER NOT NULL,
    applied_at INTEGER NOT NULL
);
INSERT OR IGNORE INTO schema_version VALUES (1, unixepoch());
```

---

## 二、内存中间态

这些结构存在于 `kimd.exe` 运行期间，**不持久化**，每 30 秒批量写入 SQLite 后归零。

### `GlobalCounters`（原子计数器组）

```rust
pub struct GlobalCounters {
    pub keystrokes:   AtomicU64,
    pub mouse_clicks: AtomicU64,
    pub characters:   AtomicU64,
    pub ctrl_c:       AtomicU64,
    pub ctrl_v:       AtomicU64,
}

pub static COUNTERS: GlobalCounters = GlobalCounters { ... };
```

**操作原则**:
- 钩子回调：仅 `fetch_add(1, Ordering::Relaxed)`
- 写入线程：`swap(0, Ordering::Relaxed)` 原子读取并归零
- `Ordering::Relaxed` 足够（只需原子性，无跨线程同步顺序要求）

---

### `AppCounterMap`（进程维度计数，带 Mutex）

```rust
pub struct AppEntry {
    pub keystrokes: u64,
    pub characters: u64,
    pub ctrl_c:     u64,
    pub ctrl_v:     u64,
}

// 事件处理线程独有（避免竞争），写入线程通过 channel 请求快照
pub type AppCounterMap = HashMap<String, AppEntry>;  // key: process_name
```

**更新时机**: 事件处理线程在收到每次输入事件时，根据 `current_process` 更新对应 entry

---

### `LanguageFocusTracker`（专注时间跟踪器）

```rust
pub struct FocusSession {
    pub language:   String,
    pub start_time: Instant,
    pub stable:     bool,          // 是否已超过 5 秒阈值
}

pub struct LanguageFocusTracker {
    current:      Option<FocusSession>,
    /// 本周期内各语言累计专注秒数（写入时清零）
    accumulated:  HashMap<String, u64>,
}
```

**状态转换**:
```
None ──[窗口切换到编辑器]──→ Some(FocusSession { start, stable: false })
                                    │
                            [超过 5 秒]
                                    ↓
                              stable: true → 开始计时
                                    │
                            [窗口离开]
                                    ↓
                    stable=true: accumulated[lang] += elapsed
                    stable=false: 丢弃（噪声过滤）
```

---

### `WindowInfo`（当前活动窗口信息，共享只读）

```rust
pub struct WindowInfo {
    pub process_name: String,   // 小写化的可执行文件名，无 .exe 后缀
    pub window_title: String,   // 完整窗口标题
    pub active_ext:   Option<String>,  // 从标题解析的文件扩展名（小写）
    pub language:     Option<String>,  // 映射后的语言名
    pub is_password:  bool,     // 当前焦点是否为密码字段（由 UIA 检测）
}

// 钩子线程更新，事件处理线程读取
pub static CURRENT_WINDOW: RwLock<WindowInfo> = RwLock::new(...);
```

---

## 三、实体关系图

```
                  daily_stats
                  ┌──────────┐
                  │ date (PK)│
                  │ keystrokes│
                  │ mouse_clicks│
                  │ characters│
                  │ ctrl_c   │
                  │ ctrl_v   │
                  └────┬─────┘
                       │ 1
                       │
             ┌─────────┴──────────┐
             │ N                  │ N
      app_stats              language_stats
  ┌─────────────────┐    ┌──────────────────┐
  │ date (PK)       │    │ date (PK)        │
  │ process_name(PK)│    │ language (PK)    │
  │ keystrokes      │    │ characters       │
  │ characters      │    │ focus_seconds    │
  │ ctrl_c          │    └──────────────────┘
  │ ctrl_v          │
  └─────────────────┘
```

---

## 四、数据生命周期

| 阶段 | 行为 |
|------|------|
| **写入周期（每 30s）** | 写入线程 swap 内存计数器 → 一个事务写入当日 3 张表 |
| **正常退出** | 收到停止信号 → 立即 final flush → 进程退出 |
| **日期切换（00:00）** | 事件处理线程检测本地日期变化 → 次日统计从零开始累积（写入继续到新日期行） |
| **历史清理** | 每次启动时检查：删除 30 天前的所有 3 张表数据（按 `date` 字段）|
| **意外崩溃** | 最多丢失最近 30 秒未写入的计数；已写入 SQLite 的数据因 WAL 模式不丢失 |

---

## 五、SQLite 配置

```sql
-- 必须在每次连接后立即执行
PRAGMA journal_mode = WAL;       -- 读写并发（CLI 查询不阻塞 daemon 写入）
PRAGMA synchronous = NORMAL;     -- 性能与崩溃安全的平衡
PRAGMA foreign_keys = ON;
PRAGMA cache_size = -2000;       -- 2MB 缓存
```

**数据库文件位置**: `%LOCALAPPDATA%\kim\stats.db`
**PID 文件位置**: `%LOCALAPPDATA%\kim\kimd.pid`
**日志文件位置**: `%LOCALAPPDATA%\kim\kim.log`
