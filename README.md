# to-see-my-life

`to-see-my-life` 是一個本地優先的 Rust CLI 工具，用來在終端中完成三件事：

- 記錄每日回顧，並生成 Markdown 文件
- 創建帶開始時間和持續時間的 todo
- 使用終端倒計時界面執行 todo timer 或 standalone timer

命令行入口名稱是：

```powershell
tsml.exe
```

第一版的設計重點是簡單、可攜帶、無後台服務。timer 只在終端程序運行時生效；程序退出後不會繼續在後台計時。

## Project Status

目前已實作：

- `init`：初始化本地配置
- `config show`：查看配置
- `config set-notes-dir`：修改 Markdown 輸出目錄
- `review`：互動式生成每日回顧 Markdown
- `todo add`：創建可預約 todo
- `todo list`：按開始時間查看 todo
- `todo done`：標記 todo 完成
- `timer`：為當前/下一個 todo 啟動倒計時
- `timer --duration`：啟動 standalone timer

目前尚未實作：

- 後台常駐計時
- 開機自啟或系統任務調度
- todo 編輯、取消、刪除
- 自定義 review 模板
- 完整 E2E 自動化測試

## Build

開發環境需要 Rust toolchain。

```powershell
cargo build --release
```

成功後會生成：

```text
target/release/tsml.exe
```

也可以在開發時直接使用：

```powershell
cargo run -- <command>
```

例如：

```powershell
cargo run -- todo list
```

## Data Layout

本項目不把配置寫入系統級配置目錄，而是寫在 `tsml.exe` 同目錄下的 `.config` 文件夾。

如果 binary 位於：

```text
D:/Mycodes/tsml/target/release/tsml.exe
```

則配置和數據位於：

```text
D:/Mycodes/tsml/target/release/.config/
```

目錄結構：

```text
.config/
  config.toml
  todos.json
  notes/
```

`config.toml` 保存 Markdown 輸出目錄：

```toml
notes_dir = "D:\\Mycodes\\tsml\\target\\release\\.config\\notes"
```

`todos.json` 保存 todo 數據：

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

todo 狀態包括：

```text
scheduled
active
done
cancelled
expired
```

## Commands

查看所有命令：

```powershell
tsml.exe --help
```

目前只保留子命令，不提供短參數別名。

## init

初始化 `.config`、`config.toml` 和 `todos.json`。

```powershell
tsml.exe init
```

程序會詢問 Markdown notes 目錄：

```text
Markdown notes directory:
```

如果直接按 Enter，會使用默認目錄：

```text
<tsml.exe 所在目錄>/.config/notes
```

生成內容：

```text
.config/config.toml
.config/todos.json
```

## config

### config show

查看當前配置和 todo 數據文件位置。

```powershell
tsml.exe config show
```

輸出示例：

```text
config: D:\...\target\release\.config\config.toml
notes_dir: D:\...\target\release\.config\notes
todos: D:\...\target\release\.config\todos.json
```

### config set-notes-dir

修改每日回顧 Markdown 的輸出目錄。

```powershell
tsml.exe config set-notes-dir "D:\Obsidian vault\daily"
```

如果目錄不存在，程序會詢問是否創建。

## review

互動式每日回顧，回答幾個問題後生成 Markdown。

```powershell
tsml.exe review
```

問題包括：

```text
今天做了什麼
完成了什麼目標
心情如何
明天想做什麼
```

生成文件名：

```text
YYYY-MM-DD-daily-review.md
```

生成內容示例：

```markdown
# Daily Review - 2026-05-02

## 今天做了什麼？

寫了 tsml 第一版。

## 完成了什麼目標？

完成 CLI 骨架、todo JSON 和 timer。

## 心情如何？

平靜。

## 明天想做什麼？

補 E2E 測試。
```

如果尚未初始化配置，`review` 會先觸發初始化流程。

## todo

`todo` 用於管理帶時間段的任務。它更接近 time blocking，而不是普通任務清單。

每個 todo 都有：

- title
- start time
- end time
- duration
- status
- focused minutes

### todo add

創建一個 todo。

```powershell
tsml.exe todo add
```

不帶參數時會互動式詢問：

```text
Todo title:
Start time:
Duration:
```

也可以直接傳參：

```powershell
tsml.exe todo add "寫 Rust CLI" --start 14:00 --duration 1h
```

支持的開始時間格式：

```text
now
14:00
14.00
2026-05-02 14:00
```

支持的 duration 格式：

```text
30
30m
1h
1h30m
```

如果只輸入數字，會被當作分鐘。

示例：

```powershell
tsml.exe todo add "午睡" --start 13:45 --duration 30m
```

這會創建一個從 `13:45` 到 `14:15` 的 todo。

### todo list

查看未完成 todo。

```powershell
tsml.exe todo list
```

輸出會按開始時間排序：

```text
a1b2c3d4  scheduled  2026-05-02 13:45-14:15  30m      午睡
b2c3d4e5  scheduled  2026-05-02 14:30-15:30  1h00m    寫 Rust CLI
```

默認不顯示 `done` 和 `cancelled`。

查看全部 todo：

```powershell
tsml.exe todo list --all
```

### todo done

標記 todo 完成。

```powershell
tsml.exe todo done <todo-id>
```

示例：

```powershell
tsml.exe todo done a1b2c3d4
```

如果不提供 id，程序會讓你從未完成 todo 中選擇：

```powershell
tsml.exe todo done
```

完成後：

- `status` 變為 `done`
- `focused_minutes` 設為 `duration_minutes`

## timer

`timer` 有兩種模式：

1. todo timer
2. standalone timer

### todo timer

不提供 `--duration` 時，`timer` 會從 `todos.json` 中尋找要執行的 todo。

```powershell
tsml.exe timer
```

選擇邏輯：

1. 如果當前時間位於某個 todo 的 `start` 和 `end` 之間，使用該 todo
2. 否則選擇下一個尚未開始的 todo
3. 如果沒有可用 todo，返回錯誤

如果選中的是未來 todo，界面會顯示距離開始還有多久。

如果選中的是當前 todo，界面會顯示剩餘時間和已專注時間。

TUI 操作：

```text
q   退出
Esc 退出
```

到截止時間後：

- 發出系統通知
- 離開 TUI
- 詢問是否把 todo 標記完成

### standalone timer

沒有 todo 時，也可以啟動一個單獨 timer。

```powershell
tsml.exe timer --title "休息" --duration 10m
```

也可以省略 title：

```powershell
tsml.exe timer --duration 25m
```

這會啟動一個不寫入 `todos.json` 的倒計時。

standalone timer 適合：

- 休息
- 臨時專注
- 等待一小段時間
- 不想創建 todo 的短時間段

## Timer UI

timer 會進入終端 TUI，大致內容包括：

```text
to-see-my-life

寫 Rust CLI
Focus  14:00 - 15:00

progress

Remaining: 00:42:18
Focused:   00:17:42
Now:       14:17:42

[q] quit
```

如果 todo 尚未開始：

```text
Starts in: 00:03:12
Remaining: 01:03:12
Focused:   00:00:00
```

## Current Limitations

第一版有意保持簡單，因此有以下限制：

- timer 不是後台服務，終端關閉後計時停止
- 系統通知目前不做可配置開關
- todo 不能編輯或刪除
- todo 衝突不會被阻止，例如可以創建重疊時間段
- Markdown 模板暫時固定
- TUI timer 還沒有純文本測試模式
- E2E 測試文檔已設計，但測試代碼尚未落地

## Development Notes

主要源碼文件：

```text
src/main.rs    command dispatch
src/cli.rs     clap command definitions
src/config.rs  .config/config.toml and todos.json paths
src/review.rs  daily review Markdown generation
src/todo.rs    todo model, JSON persistence, scheduling
src/timer.rs   TUI countdown and notification
src/util.rs    time and duration parsing helpers
```

目前已通過的驗證：

```powershell
cargo check
cargo test
cargo build --release
target\release\tsml.exe --help
```

E2E 測試設計見：

```text
docs/e2e-test-plan.md
```
