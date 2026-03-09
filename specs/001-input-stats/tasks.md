# Tasks: 用户输入统计工具 (kim)

**Feature Branch**: `001-input-stats` | **Generated**: 2026-03-06
**Input**: Design documents from `/specs/001-input-stats/`
**Prerequisites**: plan.md ✅ · spec.md ✅ · research.md ✅ · data-model.md ✅ · contracts/cli.md ✅ · quickstart.md ✅

> **TDD 强制要求**: 每个用户故事的测试任务必须先写、先确认失败（RED），再写实现使其通过（GREEN）。
> 见 quickstart.md 和 plan.md Constitution Check IV。

## Format: `[ID] [P?] [Story?] Description — file path`

- **[P]**: 可并行执行（操作不同文件，无未完成依赖）
- **[Story]**: 所属用户故事（US1–US6，对应 spec.md 优先级）
- **无 Story 标签**: Setup / Foundational / Polish 阶段

---

## Phase 1: Setup（共享基础设施）

**Purpose**: Rust 项目初始化，双二进制目标与目录结构

- [X] T001 创建 Cargo.toml：双 binary target（kim + kimd）、windows 子系统标注、所有依赖（windows=0.56、rusqlite=0.31 bundled、crossbeam-channel=0.5、clap=4 derive、chrono=0.4、log=0.4、simplelog=0.12）及所有 windows crate features（Win32_UI_WindowsAndMessaging、Win32_UI_Accessibility、Win32_System_Registry 等）——文件 Cargo.toml
- [X] T002 [P] 创建源码目录结构及空模块占位文件：src/lib.rs、src/state.rs、src/hooks/mod.rs、src/hooks/keyboard.rs、src/hooks/mouse.rs、src/hooks/window.rs、src/ime/mod.rs、src/db/mod.rs、src/db/schema.rs、src/db/writer.rs、src/stats/mod.rs、src/stats/counters.rs、src/stats/app_tracker.rs、src/stats/lang_tracker.rs、src/cli/mod.rs、src/cli/today.rs、src/cli/history.rs、src/cli/apps.rs、src/cli/langs.rs、src/cli/autostart.rs、src/bin/kim/main.rs、src/bin/kimd/main.rs、tests/integration/db_writer_test.rs、tests/integration/cli_output_test.rs、tests/integration/autostart_test.rs

---

## Phase 2: Foundational（阻塞性前置条件）

**Purpose**: 所有用户故事共同依赖的核心基础设施

**⚠️ CRITICAL**: 此阶段完成前，任何用户故事均不可开始实现

- [X] T003 实现全部 4 张表的 SQL DDL（daily_stats、app_stats、language_stats、schema_version）及 `initialize_db()` 函数——文件 src/db/schema.rs
- [X] T004 实现 DB 连接管理：`open_connection()` 函数、WAL 模式 PRAGMA、NORMAL 同步级别、AppData 路径解析（`%LOCALAPPDATA%\kim\stats.db`）——文件 src/db/mod.rs
- [X] T005 [P] 实现 `GlobalCounters` 结构体（5 个 AtomicU64 字段：keystrokes、mouse_clicks、characters、ctrl_c、ctrl_v）及 `swap_all()` 原子快照方法——文件 src/stats/counters.rs
- [X] T006 [P] 实现数据目录创建和 PID 文件读写删除工具函数（路径：`%LOCALAPPDATA%\kim\kimd.pid`）——文件 src/state.rs
- [X] T007 实现 `WindowInfo` 共享状态结构体（进程名、窗口标题）与 `IS_PASSWORD_FIELD: AtomicBool` 全局变量——文件 **src/state.rs**（与 PID 工具函数同文件，统一管理全局共享状态；其他模块通过 `crate::state::IS_PASSWORD_FIELD` 引用）

**Checkpoint**: Foundation ready — 用户故事实现可以开始

---

## Phase 3: User Story 1 — 后台静默监控输入（Priority: P1）🎯 MVP

**Goal**: `kimd.exe` 静默后台运行，持续捕获全局键盘敲击与鼠标点击，每 30 秒批量写入 SQLite

**Independent Test**: 启动 kimd，在任意程序中敲击键盘 / 点击鼠标，等待 30s 后直接用 sqlite3 查询
`%LOCALAPPDATA%\kim\stats.db` 中 daily_stats，验证 keystrokes 和 mouse_clicks 已正确增加

### Tests for User Story 1 ⚠️ 先写测试，确认 FAIL 后再实现

- [X] T008 [P] [US1] 编写 GlobalCounters 单元测试：验证 `fetch_add` 累加与 `swap_all` 原子读零——文件 src/stats/counters.rs（`#[cfg(test)]`）
- [X] T009 [P] [US1] 编写 DB 写入集成测试：给定计数器增量，验证 daily_stats UPSERT 结果正确（使用临时 SQLite 文件）——文件 tests/integration/db_writer_test.rs

### Implementation for User Story 1

- [X] T010 [P] [US1] 实现 WH_KEYBOARD_LL 钩子回调：递增 keystrokes、通过 `try_send` 推送 InputEvent 到有界 channel——文件 src/hooks/keyboard.rs
- [X] T011 [P] [US1] 实现 WH_MOUSE_LL 钩子回调：检测 WM_LBUTTONDOWN / WM_RBUTTONDOWN / WM_MBUTTONDOWN，递增 mouse_clicks，`try_send` 入 channel——文件 src/hooks/mouse.rs
- [X] T012 [US1] 实现 SetWinEventHook（EVENT_SYSTEM_FOREGROUND + EVENT_OBJECT_FOCUS）：在回调中调用 `GetWindowTextW` + `QueryFullProcessImageNameW` 更新共享 WindowInfo——文件 src/hooks/window.rs
- [X] T013 [US1] 实现钩子线程主循环：接收从 kimd/main.rs 传入的 channel `tx` 端（channel 在 main.rs 中创建并 move），调用 `SetWindowsHookExW`（键盘 + 鼠标两个钩子）、`SetWinEventHook`，进入 `GetMessageW` 消息循环——文件 src/hooks/mod.rs
- [X] T014 [US1] 实现事件处理线程：接收从 kimd/main.rs 传入的 channel `rx` 端（与 T013 共享同一 bounded channel，容量 1024，由 main.rs 在 spawn 前创建），从 `rx` 循环 `recv()`，根据事件类型分派到 `COUNTERS.fetch_add`——文件 src/hooks/mod.rs
- [X] T015 [US1] 实现 daily_stats 批量写入逻辑：调用 `COUNTERS.swap_all()` 获取增量，执行包含**全部 7 个字段**的 UPSERT：`ON CONFLICT(date) DO UPDATE SET keystrokes = keystrokes + excluded.keystrokes, mouse_clicks = mouse_clicks + excluded.mouse_clicks, characters = characters + excluded.characters, ctrl_c_count = ctrl_c_count + excluded.ctrl_c_count, ctrl_v_count = ctrl_v_count + excluded.ctrl_v_count, updated_at = excluded.updated_at`——文件 src/db/writer.rs
- [X] T015a [US1] 编写午夜 rollover 单元测试：将 `current_date()` 抽取为可注入函数，测试写入循环跨日时正确切换 date 键并为新日期创建记录——文件 src/db/writer.rs（`#[cfg(test)]`）
- [X] T016 [US1] 实现 30 秒写入定时循环与午夜日期切换检测（跨日时插入新日期记录）；停止信号通过 kimd/main.rs 注入的 `Arc<AtomicBool> stop_flag` 检测（主线程监听 `Local\kim-stop-event` 后设 flag 为 true，写入线程轮询 flag），收到信号后执行最终 flush 并退出——文件 src/db/writer.rs
- [X] T017 [US1] 实现 kimd/main.rs：创建有界 crossbeam-channel（容量 1024，`tx` move 入钩子线程，`rx` move 入事件处理线程）；创建 `Arc<AtomicBool> stop_flag`（共享给写入线程）；创建 stop 命名事件 `Local\kim-stop-event`；写入 PID 文件；spawn 4 个线程（钩子线程接收 `tx`，事件处理线程接收 `rx`，UIA 占位线程，写入线程接收 `Arc<stop_flag>`）；主线程调用 `WaitForSingleObject(stop_event)` 阻塞；收到信号后设 `stop_flag = true`，等待线程退出，删除 PID 文件；解析 `--autostart` 标志：若存在则在 hook 注册前 `std::thread::sleep(Duration::from_secs(3))`——文件 src/bin/kimd/main.rs

**Checkpoint**: `kimd.exe` 可在后台运行，30 秒后 daily_stats 中出现当日 keystrokes 与 mouse_clicks 数据

---

## Phase 4: User Story 4 — CLI 查看统计信息（Priority: P1）

**Goal**: `kim.exe` 提供 start / stop / status / today / history / autostart 子命令，表格格式正确

**Independent Test**: 向预填充了数据的 SQLite 运行 `kim today`，验证表格格式与字段值；运行 `kim start` → `kim status` → `kim stop` 完整生命周期

### Tests for User Story 4 ⚠️ 先写测试，确认 FAIL 后再实现

- [X] T018 [P] [US4] 编写 CLI 输出格式集成测试：向临时 SQLite 写入已知数据，执行 `kim today` 验证表格含正确字段与千位分隔符——文件 tests/integration/cli_output_test.rs
- [X] T019 [P] [US4] 编写 autostart 集成测试：使用隔离临时注册表键验证 enable / disable / status 操作——文件 tests/integration/autostart_test.rs

### Implementation for User Story 4

- [X] T020 [US4] 实现 kim/main.rs：使用 clap derive 定义全部子命令（start、stop、status、today、history、apps、langs、autostart + sub-subcommands），分派到各处理函数——文件 src/bin/kim/main.rs
- [X] T021 [US4] 实现 `kim start`：检查 PID 文件是否已存在且进程存活（退出码 1），spawn `kimd.exe`（DETACHED_PROCESS），最多等待 2 秒确认 PID 文件创建成功，输出 `kim started (PID: <pid>)`——文件 src/bin/kim/main.rs
- [X] T022 [P] [US4] 实现 `kim stop`：读取 PID 文件，发送命名事件 `Local\kim-stop-event`，等待最多 5 秒，超时则 TerminateProcess，最终删除 PID 文件——文件 src/bin/kim/main.rs
- [X] T023 [P] [US4] 实现 `kim status`：读取 PID 文件，调用 `OpenProcess` 检测进程是否存活，计算 uptime，输出 `running  PID: <pid>  uptime: HH:MM:SS` 或 `stopped`——文件 src/bin/kim/main.rs
- [X] T024 [US4] 实现 `kim today`：从 daily_stats 查询当日数据并渲染带边框的格式化表格（千位分隔符、最后更新时间戳）——文件 src/cli/today.rs
- [X] T025 [P] [US4] 实现 `kim history`：支持 DATE 参数（YYYY-MM-DD / yesterday）显示单日统计，支持 `--days N` 显示最近 N 天对比列表——文件 src/cli/history.rs
- [X] T026 [P] [US4] 实现 `kim autostart enable/disable/status`：操作 `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` 下的 `kim` 值，值内容为 `"<path>\kimd.exe" --autostart`；`--autostart` 标志由 kimd/main.rs（T017）解析并触发 3 秒延迟启动——文件 src/cli/autostart.rs

**Checkpoint**: CLI 完整生命周期可用：start daemon、`kim today` 查询当日数据、`kim history --days 7` 查历史、stop daemon、autostart 开关

---

## Phase 5: User Story 2 — 准确统计实际打字字符数（Priority: P2）

**Goal**: UIA TextChanged 捕获 IME 提交字符；直接可见按键计入字符数；密码字段不计打字数

**Independent Test**: 使用中文输入法输入 3 个中文字 + 直接键入 5 个英文字母 → `kim today` 显示 characters: 8

### Tests for User Story 2 ⚠️ 先写测试，确认 FAIL 后再实现

- [X] T027 [P] [US2] 编写单元测试：验证 `IS_PASSWORD_FIELD` AtomicBool 在密码字段焦点时为 true，离开后为 false——文件 src/state.rs（`#[cfg(test)]`，与 T007 的声明位置一致）
- [X] T028 [P] [US2] 编写单元测试：验证可见字符 VK 码过滤逻辑（字母/数字/标点 → 计入；F1-F12、方向键、Esc → 不计入；VK_PROCESSKEY → 由 UIA 处理，不重复计入）——文件 src/hooks/keyboard.rs（`#[cfg(test)]`）

### Implementation for User Story 2

- [X] T029 [US2] 实现 UIA STA COM 线程：`CoInitializeEx(COINIT_APARTMENTTHREADED)`，注册一次全局根元素 TextChanged 事件监听器（`UIA_Text_TextChangedEventId`），在回调中计算文本增量并原子递增 `COUNTERS.characters`——文件 src/ime/mod.rs
- [X] T030 [US2] 实现密码字段检测：在 UIA 线程上监听 EVENT_OBJECT_FOCUS，异步查询 `UIA_IsPasswordPropertyId`，将结果写入 `crate::state::IS_PASSWORD_FIELD` AtomicBool（声明在 src/state.rs，见 T007）——文件 src/ime/mod.rs
- [X] T031 [US2] 更新键盘钩子：对直接可见字符（VK 非 `VK_PROCESSKEY`、非功能键）且 `crate::state::IS_PASSWORD_FIELD.load(Ordering::Relaxed) == false` 时递增 `COUNTERS.characters`；`VK_PROCESSKEY` 跳过（避免与 UIA 重复计数）——文件 src/hooks/keyboard.rs
- [X] T032 [US2] 在 kimd/main.rs 中 spawn UIA STA 线程，替换 Phase 3 的 UIA 占位线程——文件 src/bin/kimd/main.rs

**Checkpoint**: `kim today` characters 字段准确反映 IME 提交字符 + 直接可见键入，密码字段输入不被计入

---

## Phase 6: User Story 3 — 统计剪贴板快捷键操作次数（Priority: P2）

**Goal**: Ctrl+C 递增 ctrl_c；Ctrl+V 递增 ctrl_v；Ctrl+V 不影响 characters 计数

**Independent Test**: 执行 3 次 Ctrl+C 和 2 次 Ctrl+V → `kim today` 显示 ctrl_c: 3、ctrl_v: 2，characters 不变

### Tests for User Story 3 ⚠️ 先写测试，确认 FAIL 后再实现

- [X] T033 [P] [US3] 编写单元测试：验证 Ctrl 修饰键 + C/V 检测逻辑（使用保存的键盘状态模拟，非 Ctrl 组合不触发计数）——文件 src/hooks/keyboard.rs（`#[cfg(test)]`）

### Implementation for User Story 3

- [X] T034 [US3] 在键盘钩子回调中增加 Ctrl+C/V 检测：检查 `GetKeyState(VK_CONTROL)` 最高位 + vkCode == VK_C / VK_V，分别递增 `COUNTERS.ctrl_c` / `COUNTERS.ctrl_v`；Ctrl+V 路径确保不调用 characters 递增分支——文件 src/hooks/keyboard.rs
- [X] T035 [US3] 验证 T015 的 daily_stats UPSERT 已包含 ctrl_c_count 和 ctrl_v_count（T015 已明确枚举全部 7 字段，此任务为确认性检查，无需修改代码）——文件 src/db/writer.rs
- [X] T036 [US3] 在 `kim today` 表格渲染中添加「复制 (Ctrl+C)」和「粘贴 (Ctrl+V)」行——文件 src/cli/today.rs

**Checkpoint**: `kim today` 显示完整 5 项统计（键盘、鼠标、字符、复制、粘贴），粘贴操作不膨胀字符数

---

## Phase 7: User Story 5 — 按应用统计输入量（Priority: P3）

**Goal**: 每次输入事件同时归属到当前前台进程；`kim apps` 按应用展示分项统计

**Independent Test**: 在 VS Code 和记事本中各输入若干字符 → `kim apps` 分别显示 `code` 和 `notepad` 的正确键击数与字符数

### Tests for User Story 5 ⚠️ 先写测试，确认 FAIL 后再实现

- [ ] T037 [P] [US5] 编写 AppCounterMap 单元测试：验证多进程聚合计数、快照后清零——文件 src/stats/app_tracker.rs（`#[cfg(test)]`）
- [ ] T038 [P] [US5] 扩展 DB 写入集成测试：给定多进程增量，验证 app_stats UPSERT 正确（追加到 tests/integration/db_writer_test.rs）

### Implementation for User Story 5

- [ ] T039 [P] [US5] 实现 `AppCounterMap` 和 `AppEntry`（keystrokes、characters、ctrl_c、ctrl_v 字段）及快照方法——文件 src/stats/app_tracker.rs
- [ ] T040 [US5] 在窗口追踪器中补充进程名规范化处理（T012 已通过 `QueryFullProcessImageNameW` 获取原始完整路径；此任务负责从路径中提取文件名、转小写、去掉 .exe 后缀），将规范化结果写入 `WindowInfo.process_name` 字段——文件 src/hooks/window.rs（注：T012 产出原始路径，T040 产出规范化 process_name，职责不重叠）
- [ ] T041 [US5] 在事件处理线程中：每次收到输入事件时，读取当前 `WindowInfo.process_name`，更新 `AppCounterMap` 对应 entry——文件 src/hooks/mod.rs
- [ ] T042 [US5] 在 DB 写入线程中：快照 `AppCounterMap`，对每个进程条目执行 app_stats UPSERT（`ON CONFLICT(date, process_name) DO UPDATE SET ...`）——文件 src/db/writer.rs
- [ ] T043 [US5] 实现 `kim apps` 子命令：查询 app_stats，渲染排行表格（应用、键盘敲击、打字数、复制、粘贴，`--date`、`--top N` 选项，按键盘敲击降序）——文件 src/cli/apps.rs

**Checkpoint**: `kim apps` 正确显示各应用分项输入统计，切换进程时计数无丢失（SC-008）

---

## Phase 8: User Story 6 — 统计编程语言输入量与专注时间（Priority: P3）

**Goal**: 从活动窗口标题解析文件扩展名推断语言；连续聚焦 > 5 秒才计入专注时间；`kim langs` 展示结果

**Independent Test**: 在编辑器中打开 test.py 输入若干字符，再切换到 index.ts，等待各超过 5 秒 → `kim langs` 显示 Python 和 TypeScript 各自的字符数与专注时长

### Tests for User Story 6 ⚠️ 先写测试，确认 FAIL 后再实现

- [ ] T044 [P] [US6] 编写 LanguageFocusTracker 单元测试：验证 5 秒阈值过滤（< 5s 丢弃、>= 5s 计入）、窗口切换时停止计时、accumulated 秒数累加——文件 src/stats/lang_tracker.rs（`#[cfg(test)]`）
- [ ] T045 [P] [US6] 编写扩展名→语言映射单元测试：验证全部 20+ 已知扩展名（py/js/ts/java/go/rs/c/cpp/cs/rb/php/swift/kt/html/css/sql/sh/vue/jsx/tsx）解析正确，未知扩展名返回 "Other"——文件 src/stats/lang_tracker.rs（`#[cfg(test)]`）

### Implementation for User Story 6

- [ ] T046 [P] [US6] 实现扩展名→语言映射表（20+ 条目，对应 FR-020，未知返回 "Other"）——文件 src/stats/lang_tracker.rs
- [ ] T047 [US6] 实现 `LanguageFocusTracker`：`FocusSession`（language、start_time: Instant、stable: bool）+ `accumulated: HashMap<String, u64>`，`on_window_change()` 和 `tick()` 方法（每秒或写入时调用）——文件 src/stats/lang_tracker.rs
- [ ] T048 [US6] 在窗口追踪器中解析窗口标题提取语言：使用 `rsplitn(2, " - ")` 取文件名部分，`Path::extension()` 取扩展名，查映射表——文件 src/hooks/window.rs
- [ ] T049 [US6] 在事件处理线程中：窗口切换事件到达时调用 `LanguageFocusTracker.on_window_change()`；输入字符事件同时更新语言维度字符计数——文件 src/hooks/mod.rs
- [ ] T050 [US6] 在 DB 写入线程中：快照 `LanguageFocusTracker.accumulated`，对每个语言条目 UPSERT language_stats（`ON CONFLICT(date, language) DO UPDATE SET focus_seconds = focus_seconds + excluded.focus_seconds, ...`）——文件 src/db/writer.rs
- [ ] T051 [US6] 实现 `kim langs` 子命令：查询 language_stats，渲染表格（语言、字符数、专注时间格式 `Xh Ym`，`--date` 选项，按 focus_seconds 降序）——文件 src/cli/langs.rs

**Checkpoint**: `kim langs` 正确显示各语言字符数与专注时长（精度满足 SC-009：累积误差 ≤ 5s/小时）

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: 日志、数据保留、JSON 输出、最终验收

- [ ] T052 [P] 在 kimd/main.rs 中初始化 simplelog 文件日志（路径 `%LOCALAPPDATA%\kim\kim.log`，级别 Info），在各模块关键路径添加 `log::info!` / `log::warn!` / `log::error!` 宏——文件 src/bin/kimd/main.rs（及各模块）
- [ ] T053 [P] 在 DB 写入线程每次 flush 后执行 30 天数据保留清理：`DELETE FROM daily_stats / app_stats / language_stats WHERE date < date('now', '-30 days')`——文件 src/db/writer.rs
- [ ] T054 [P] 为 `kim today`、`kim history`、`kim apps`、`kim langs` 实现 `--json` 输出选项，以符合 contracts/cli.md 规定的 JSON 格式——文件 src/cli/today.rs · src/cli/history.rs · src/cli/apps.rs · src/cli/langs.rs
- [ ] T055 运行 `cargo clippy -- -D warnings`，修复全部 lint 警告（0 警告约束）——全源码文件
- [ ] T056 运行 `cargo test`，确认全部单元测试与集成测试通过；按 quickstart.md 步骤执行完整 smoke test（start → 敲键 → today → stop）

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: 无依赖，立即开始
- **Phase 2 (Foundational)**: 依赖 Phase 1 完成 — **阻塞所有用户故事**
- **Phase 3 (US1 P1)**: 依赖 Phase 2 完成
- **Phase 4 (US4 P1)**: 依赖 Phase 2 完成（CLI 查询逻辑可与 Phase 3 并行开发，但完整测试需 Phase 3 完成的 daemon）
- **Phase 5 (US2 P2)**: 依赖 Phase 3 完成（需要 hooks + kimd 基础线程）
- **Phase 6 (US3 P2)**: 依赖 Phase 3 完成（扩展键盘钩子已有的计数逻辑）
- **Phase 7 (US5 P3)**: 依赖 Phase 3 完成（依赖窗口追踪 + hooks mod 事件路由）
- **Phase 8 (US6 P3)**: 依赖 Phase 7 完成（依赖 app 追踪基础能力与 AppCounterMap 模式）
- **Phase 9 (Polish)**: 依赖所有目标用户故事完成

### User Story Dependencies

- **US1 (P1)**: Phase 2 完成后可开始，无其他故事依赖
- **US4 (P1)**: Phase 2 完成后可开始；CLI 查询部分（today/history）与 US1 并行开发；start/stop/status 需 US1 完成才能端到端测试
- **US2 (P2)**: 依赖 US1（hooks 线程 + kimd 框架已存在）
- **US3 (P2)**: 依赖 US1（键盘钩子已实现，US3 为增量修改）
- **US5 (P3)**: 依赖 US1（窗口追踪 + 事件处理线程已建立）
- **US6 (P3)**: 依赖 US5（AppCounterMap 模式复用；窗口追踪已有进程名）

### Within Each User Story

```
测试 (RED) → 核心数据结构 → 业务逻辑 → DB 集成 → CLI 展示 → 测试通过 (GREEN)
```

### Parallel Opportunities

- Phase 1 内：T001 和 T002 互不依赖（T002 在 T001 完成后可立即并行展开）
- Phase 2 内：T005 (GlobalCounters) 和 T006 (state.rs) 可并行
- Phase 3 内：T008/T009（测试）、T010/T011（两个钩子）可并行
- Phase 4 内：T018/T019（测试）、T022/T023（stop/status）、T025/T026 可并行
- Phase 3 和 Phase 4 的查询部分（today/history）可并行开发

---

## Parallel Example: User Story 1

```powershell
# 同时写两个钩子的测试（RED 阶段）:
Task T008: "GlobalCounters 单元测试 in src/stats/counters.rs"
Task T009: "DB writer 集成测试 in tests/integration/db_writer_test.rs"

# 同时实现两个钩子（GREEN 阶段）:
Task T010: "WH_KEYBOARD_LL hook in src/hooks/keyboard.rs"
Task T011: "WH_MOUSE_LL hook in src/hooks/mouse.rs"
```

## Parallel Example: User Story 4

```powershell
# 同时实现独立 CLI 子命令:
Task T022: "kim stop in src/bin/kim/main.rs"
Task T023: "kim status in src/bin/kim/main.rs"
Task T025: "kim history in src/cli/history.rs"
Task T026: "kim autostart in src/cli/autostart.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 + 4 Only)

1. 完成 Phase 1: Setup
2. 完成 Phase 2: Foundational（阻塞点）
3. 完成 Phase 3: US1 — daemon 后台运行 + 键鼠计数写入
4. 完成 Phase 4: US4 — CLI start/stop/status/today/history
5. **STOP and VALIDATE**: 用 quickstart.md smoke test 验证核心流程
6. 此时已满足 **所有 P1 用户故事**，可交付 MVP

### Incremental Delivery

1. Phase 1+2 → Foundation ready
2. Phase 3 (US1) → Daemon 可运行，数据写入 SQLite
3. Phase 4 (US4) → CLI 可查询，MVP 完整
4. Phase 5 (US2) → 字符计数功能上线
5. Phase 6 (US3) → Ctrl+C/V 统计上线
6. Phase 7 (US5) → 应用维度分析上线
7. Phase 8 (US6) → 编程语言专注时间上线
8. Phase 9 → Polish 与最终验收

---

## Summary

| 阶段 | 任务范围 | 任务数 | 标注 |
|------|----------|--------|------|
| Phase 1: Setup | T001–T002 | 2 | — |
| Phase 2: Foundational | T003–T007 | 5 | 阻塞所有故事 |
| Phase 3: US1 (P1) | T008–T017 + T015a | 11 | 含 3 个测试任务（含午夜 rollover 测试） |
| Phase 4: US4 (P1) | T018–T026 | 9 | 含 2 个测试任务 |
| Phase 5: US2 (P2) | T027–T032 | 6 | 含 2 个测试任务 |
| Phase 6: US3 (P2) | T033–T036 | 4 | 含 1 个测试任务 |
| Phase 7: US5 (P3) | T037–T043 | 7 | 含 2 个测试任务 |
| Phase 8: US6 (P3) | T044–T051 | 8 | 含 2 个测试任务 |
| Phase 9: Polish | T052–T056 | 5 | — |
| **合计** | T001–T056 + T015a | **57** | 其中 12 个测试任务 |

**并行机会**: 18 个任务标注 [P]（Phase 2、3、4、5、6、7、8 内部均有并行任务）

**MVP 范围**: Phase 1 + Phase 2 + Phase 3 (US1) + Phase 4 (US4) — 共 26 个任务，交付完整 P1 功能
