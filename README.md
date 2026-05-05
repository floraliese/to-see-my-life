# to-see-my-life

`to-see-my-life` 是一個本地優先的 Rust 個人管理工具，用來在終端和桌面 GUI 中管理時間和回顧：

- 記錄每日回顧，並生成 Markdown 文件（含自動 todo 摘要）
- 創建帶時間段的 todo，支持 cancel / delete / edit / reschedule
- 使用 `today` 作為 TODO / DOING / DONE 工作台管理當日任務
- 使用終端倒計時界面執行 todo timer 或 standalone timer
- 使用 `gui` 打開 native desktop 工作台
- 記錄 timer session history，並追加到每日回顧
- 查看本週統計

命令行入口名稱：

```powershell
tsml.exe
```

設計重點是簡單、可攜帶、無後台服務。

## Project Status

目前已實作：

- `init`：初始化本地配置
- `config show` / `config set-notes-dir`：配置管理
- `review`：互動式每日回顧 Markdown（含今日 todo 摘要）
- `todo add / list / done / cancel / delete / edit / reschedule`：完整 todo 管理
- `todo add / edit / reschedule --force`：繞過時間重疊檢查
- `today add / start / done / defer`：今日工作台管理，支持把未完成 todo 延到明天
- `timer`：為當前或下一個 todo 啟動倒計時 TUI
- `timer --duration 25m --plain --no-notify`：standalone timer，支持純文本模式
- `timer_sessions.json`：記錄每次專注 session，review 會自動追加摘要
- `today`：以 TODO / DOING / DONE 看板顯示今日工作台
- `gui`：native eframe/egui 桌面工作台，復用同一份本地數據
- `stats --week`：本週統計
- `TSML_HOME` 環境變量：覆蓋數據目錄

已落地的自動化測試：

- 10 個 unit tests（時間解析、格式、邊界錯誤）
- 11 個 E2E 整合測試（init、todo CRUD、overlap、today、stats、timer plain）

## Build

需要 Rust toolchain。

```powershell
cargo build --release
```

開發使用：

```powershell
cargo run -- <command>
```

例如：

```powershell
cargo run -- todo list
```

## Data Layout

### 默認（portable）模式

配置和數據寫在 `tsml.exe` 同目錄下的 `.config` 文件夾：

```text
<exe 目錄>/.config/
  config.toml
  todos.json
  timer_sessions.json
  notes/
```

### TSML_HOME 模式

設定環境變量 `TSML_HOME` 後，所有數據文件位於 `TSML_HOME` 目錄下，`.config` 子目錄不會附加：

```powershell
$env:TSML_HOME = "D:\my-tsml-data"
tsml.exe config show
# 讀取 D:\my-tsml-data\config.toml
```

此模式主要用於測試隔離或高級用戶。

### 文件格式

`config.toml`：

```toml
notes_dir = "D:\\path\\to\\notes"
```

`todos.json`：

```json
[
  {
    "id": "a1b2c3d4",
    "title": "寫 Rust CLI",
    "start": "2026-05-02T14:00:00+08:00",
    "end": "2026-05-02T15:00:00+08:00",
    "duration_minutes": 60,
    "status": "scheduled",
    "focused_minutes": 0,
    "created_at": "2026-05-02T13:00:00+08:00"
  }
]
```

todo 狀態：`scheduled`, `active`（即時計算）, `done`, `cancelled`, `expired`（即時計算）。

`today add` 或 `today defer` 產生的任務可以暫時沒有 `start` / `end`，代表它位於工作台 TODO 欄，直到 `today start <id>` 才排程到現在並啟動專注。

`timer_sessions.json`：

```json
[
  {
    "id": "e5f6a7b8",
    "todo_id": "a1b2c3d4",
    "title": "寫 Rust CLI",
    "started_at": "2026-05-02T14:00:00+08:00",
    "ended_at": "2026-05-02T15:00:00+08:00",
    "planned_seconds": 3600,
    "focused_seconds": 3600,
    "outcome": "completed"
  }
]
```

## Commands

### init

初始化配置和數據文件。

```powershell
tsml.exe init
```

### config

```powershell
tsml.exe config show
tsml.exe config set-notes-dir "D:\Obsidian vault\daily"
```

### review

互動式生成每日回顧，末尾自動追加今日 todo 摘要和 timer session 摘要。

```powershell
tsml.exe review
```

### gui

打開 native desktop GUI。GUI 不是 Tauri/WebView，而是 `eframe/egui` 桌面窗口，直接讀寫與 CLI 相同的 `todos.json`、`timer_sessions.json` 和 `config.toml`。

```powershell
tsml.exe gui
```

目前 GUI 包含：

- Today Workbench：TODO / DOING / DONE 三欄管理
- 快速新增今日 todo
- 從 todo 開始 GUI focus session，完成後寫入 `timer_sessions.json`
- standalone focus session
- 最近 session history
- review markdown 預覽與複製
- data directory / notes directory 管理

終端 TUI timer 仍然保留；`today start <id>` 和 `timer` 命令會繼續使用終端專注模式。

### todo

每個 todo 包含 title、start time、end time、duration、status、focused minutes。

```powershell
tsml.exe todo add                                    # 互動式
tsml.exe todo add "寫 Rust CLI" --start 14:00 --duration 1h
tsml.exe todo add "任務" --start now --duration 30m --force   # 繞過重疊檢查

tsml.exe todo list                                   # 未完成
tsml.exe todo list --all                             # 包含已完成和取消

tsml.exe todo done <id>
tsml.exe todo cancel <id>                            # 取消，保留歷史
tsml.exe todo delete <id>                            # 永久刪除（需確認）
tsml.exe todo delete <id> --yes                      # 跳過確認

tsml.exe todo edit <id> --title "新標題"
tsml.exe todo edit <id> --start 15:00 --duration 1h --force

tsml.exe todo reschedule <id> --start 15:00          # 只改時間
tsml.exe todo reschedule <id> --start now --duration 45m --force
```

支持的開始時間格式：`now`, `14:00`, `14.00`, `2026-05-02 14:00`。

支持的 duration 格式：`30`（分鐘）, `30m`, `1h`, `1h30m`, `10s`（用於測試）。

### timer

```powershell
tsml.exe timer                             # 自動選當前或下一個 todo
tsml.exe timer --duration 25m              # standalone
tsml.exe timer --title "休息" --duration 10m
tsml.exe timer --duration 1s --plain --no-notify   # 純文本 + 無通知（測試用）
```

選擇邏輯：若當前時間落在某 todo 的時間段內則使用該 todo，否則選下一個未來 todo。

TUI 顯示：標題、階段（Scheduled/Focus）、開始-結束時間、剩餘時間、已專注時間、當前時間。

操作：`q` 或 `Esc` 退出。

到截止時間後會發出系統通知；若綁定 todo，詢問是否標記完成。

### today

以 TODO / DOING / DONE 看板顯示今日工作台，並提供當日任務管理入口。

```powershell
tsml.exe today
tsml.exe today add "整理 README" --duration 25m
tsml.exe today start <id> --duration 25m
tsml.exe today done <id>
tsml.exe today defer <id> --to tomorrow
```

`today start <id>` 會把任務排程到現在，移入 DOING，然後啟動終端 timer。timer 結束後會寫入 `timer_sessions.json`，並在關聯 todo 時詢問是否標記完成。

手動 `today done` / `todo done` 只標記完成，不會自動補滿專注時間；專注時間只由 timer session 回寫。若 deferred 任務的目標日期已過，下一次打開 `today` 時會自動 carry over 到今天的 TODO 欄。

### stats --week

顯示本週統計：總任務數、完成/過期/未完成/取消、計畫時間、已專注、最常出現的任務標題。

```powershell
tsml.exe stats --week
```

## Current Limitations

- timer 不是後台服務，終端關閉後計時停止
- GUI focus session 目前只在 GUI 進程內計時，尚未做崩潰恢復或後台 daemon
- 自定義 review 模板尚未支持
