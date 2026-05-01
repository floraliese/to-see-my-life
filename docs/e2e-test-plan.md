# to-see-my-life E2E Test Plan

## 目標

這份文檔定義 `to-see-my-life` 第一版的端到端測試設計。測試目標不是覆蓋每個函數，而是確認用戶從 `tsml.exe` 入口執行主要工作流時，文件、配置、todo 排程、timer 倒計時和通知行為能按預期協同工作。

第一版 E2E 測試應覆蓋：

- 第一次初始化 `.config`
- 每日回顧 Markdown 生成
- todo 的創建、排序、持久化和完成標記
- timer 綁定 todo 的流程
- standalone timer 流程
- 錯誤輸入和缺失配置時的行為

## 測試原則

E2E 測試應該盡量跑 release binary，而不是直接調用 Rust 函數：

```powershell
cargo build --release
target\release\tsml.exe <command>
```

原因是這個工具的關鍵行為與 executable 所在目錄有關。配置文件和數據文件都會生成在 `.exe` 同目錄下的 `.config` 中，因此 E2E 測試必須驗證真實 binary 的行為。

測試不應污染開發者本地的真實 `target\release\.config`。應該把 `tsml.exe` 複製到臨時目錄，再從臨時目錄執行：

```text
temp/
  tsml.exe
```

執行後期望生成：

```text
temp/
  tsml.exe
  .config/
    config.toml
    todos.json
    notes/
```

## 建議測試工具

Rust 端建議使用：

- `assert_cmd`：執行 CLI binary 並檢查 stdout / stderr / exit code
- `assert_fs`：創建隔離的臨時目錄和檢查文件
- `predicates`：斷言輸出內容
- `rexpect` 或 `expectrl`：測試互動式輸入

`Cargo.toml` 後續可加入：

```toml
[dev-dependencies]
assert_cmd = "2"
assert_fs = "1"
predicates = "3"
expectrl = "0.7"
```

## 測試環境設計

每個 E2E 測試使用獨立臨時目錄：

1. 先執行 `cargo build --release`
2. 建立臨時目錄
3. 將 `target/release/tsml.exe` 複製到臨時目錄
4. 從臨時目錄執行 `.\tsml.exe`
5. 驗證臨時目錄下 `.config` 的內容
6. 測試結束後刪除臨時目錄

這樣可以避免測試互相影響，也能精確驗證「配置跟著 exe 走」這個設計。

## 測試案例

### E2E-001: 初始化配置

命令：

```powershell
.\tsml.exe init
```

互動輸入：

```text
Markdown notes directory: <temp>\notes
```

期望：

- exit code 為 `0`
- 生成 `.config/config.toml`
- 生成 `.config/todos.json`
- `config.toml` 中的 `notes_dir` 指向輸入的 notes 目錄
- `todos.json` 初始內容是空陣列

應檢查文件：

```text
.config/config.toml
.config/todos.json
notes/
```

### E2E-002: 缺失配置時自動初始化

命令：

```powershell
.\tsml.exe review
```

互動輸入：

```text
Markdown notes directory: <temp>\notes
今天做了什麼: 寫了 tsml
完成了什麼目標: 完成第一版 CLI
心情如何: 平靜
明天想做什麼: 補測試
```

期望：

- 沒有配置時會先提示初始化
- 生成 `.config/config.toml`
- 生成 Markdown 文件
- Markdown 包含四個回答內容

需要注意日期是不穩定值，測試中應用當天日期組合文件名：

```text
<notes_dir>/<YYYY-MM-DD>-daily-review.md
```

### E2E-003: 生成每日回顧 Markdown

前置：

- 已存在 `.config/config.toml`
- `notes_dir` 指向臨時 notes 目錄

命令：

```powershell
.\tsml.exe review
```

互動輸入：

```text
今天做了什麼: 閱讀 Rust 文檔
完成了什麼目標: 完成 todo JSON 存儲
心情如何: 專注
明天想做什麼: 改善 timer UI
```

期望：

- 生成 `<YYYY-MM-DD>-daily-review.md`
- 文件內容包含標題 `# Daily Review - <YYYY-MM-DD>`
- 文件內容包含四個 section
- 文件內容包含用戶輸入

### E2E-004: 創建預約 todo

前置：

- 已初始化配置

命令：

```powershell
.\tsml.exe todo add "寫 Rust CLI" --start 14:00 --duration 1h
```

期望：

- exit code 為 `0`
- stdout 包含 `Added 寫 Rust CLI`
- `.config/todos.json` 新增一條 todo
- `title` 為 `寫 Rust CLI`
- `duration_minutes` 為 `60`
- `status` 為 `scheduled`
- `start` 是今天 14:00
- `end` 是今天 15:00

### E2E-005: todo 按開始時間排序

前置：

- 已初始化配置

命令：

```powershell
.\tsml.exe todo add "下午任務" --start 15:00 --duration 30m
.\tsml.exe todo add "早點任務" --start 13:00 --duration 30m
.\tsml.exe todo list
```

期望：

- `todo list` 中 `早點任務` 出現在 `下午任務` 前面
- `.config/todos.json` 中存儲順序也按 start 排序

### E2E-006: 標記 todo 完成

前置：

- 已有一條 open todo

命令：

```powershell
.\tsml.exe todo done <todo-id>
```

期望：

- exit code 為 `0`
- stdout 包含 `Marked <todo-id> done`
- `.config/todos.json` 中該 todo 的 `status` 變為 `done`
- `focused_minutes` 等於 `duration_minutes`
- `todo list` 默認不顯示 done todo
- `todo list --all` 顯示 done todo

### E2E-007: standalone timer

命令：

```powershell
.\tsml.exe timer --title "休息" --duration 1m
```

期望：

- 進入 TUI
- 顯示標題 `休息`
- 顯示倒計時
- 到時間後退出 TUI
- 不修改 `.config/todos.json`

自動化建議：

- 使用 `expectrl` 啟動進程
- 等待輸出進入 alternate screen 可能不穩定，這部分可以先只做 smoke test
- 更穩定的做法是後續為 timer 增加 `--no-tui` 或 `--test-mode`，讓 E2E 測試可以用純文本模式驗證倒計時邏輯

### E2E-008: todo timer 選擇當前 todo

前置：

- 寫入一條 start <= now < end 的 todo

命令：

```powershell
.\tsml.exe timer
```

期望：

- timer 顯示當前 todo 的 title
- 到 end 時發出完成通知
- CLI 詢問是否標記完成
- 輸入 `yes` 後，該 todo 狀態變為 `done`

自動化建議：

- 直接構造 `.config/todos.json`，把 start 設為當前時間前 5 秒，end 設為當前時間後 2 秒
- 使用 `expectrl` 等待完成提示
- 輸入 `y`
- 檢查 JSON 狀態

### E2E-009: todo timer 選擇下一個 todo

前置：

- 寫入一條 start > now 的 todo

命令：

```powershell
.\tsml.exe timer
```

期望：

- timer 顯示下一個 todo
- TUI 顯示 `Starts in`
- 到 start 後進入 focus 狀態
- 到 end 後提示完成

自動化建議：

- start 設為當前時間後 2 秒
- end 設為當前時間後 4 秒
- 測試總時長控制在 6 秒內

### E2E-010: 沒有 todo 時啟動 todo timer

前置：

- 已初始化配置
- `todos.json` 為空

命令：

```powershell
.\tsml.exe timer
```

期望：

- exit code 非 `0`
- stderr 或錯誤訊息包含：

```text
no scheduled todo found
```

並提示可以使用：

```text
tsml timer --duration 25m --title Break
```

### E2E-011: 非法 duration

命令：

```powershell
.\tsml.exe todo add "錯誤任務" --start 14:00 --duration abc
```

期望：

- exit code 非 `0`
- 不新增 todo
- 錯誤訊息指出 duration 不合法

也應測：

```powershell
.\tsml.exe timer --duration 0m
.\tsml.exe timer --duration -5m
```

### E2E-012: 非法 start time

命令：

```powershell
.\tsml.exe todo add "錯誤任務" --start nope --duration 30m
```

期望：

- exit code 非 `0`
- 不新增 todo
- 錯誤訊息提示可用格式：

```text
HH:MM, HH.MM, now, YYYY-MM-DD HH:MM
```

## Timer 測試的可測性改造建議

目前 timer 是 TUI-first 設計，對真人使用是合理的，但對自動化 E2E 測試不夠友好。建議增加兩個測試輔助選項：

```bash
tsml timer --duration 10s --title Test --plain
tsml timer --duration 10s --title Test --no-notify
```

`--plain` 行為：

- 不進入 alternate screen
- 每秒輸出一行純文本
- 到時間後輸出 `Timer finished`

`--no-notify` 行為：

- 跳過系統通知
- 避免 CI 或無桌面環境測試失敗

這兩個選項可以隱藏在 help 之外，或標記為測試/調試用途。

## 系統通知測試策略

系統通知很難在 E2E 中穩定斷言。第一版不建議直接驗證 Windows 通知中心是否出現通知。

更可控的做法是把通知邏輯抽象為接口：

```text
Notifier
  RealNotifier
  NoopNotifier
  RecordingNotifier
```

普通運行使用 `RealNotifier`，E2E 測試使用 `--no-notify` 或 `RecordingNotifier`，只驗證「程序走到了通知分支」。

## CI 分層建議

E2E 測試可以分層執行：

### 必跑

- init
- review
- todo add
- todo list
- todo done
- 非法輸入

這些都不需要長時間等待，也不依賴桌面通知。

### 可選

- standalone timer
- todo timer

這些涉及時間等待、TUI 和通知，應該使用較短 duration，並且支持 `--plain` / `--no-notify` 後再放入 CI。

### 手動驗收

- 真實 TUI 顯示效果
- Windows 系統通知
- 長時間番茄鐘，例如 25 分鐘

## 第一輪落地順序

1. 增加 `assert_cmd`、`assert_fs`、`predicates`
2. 寫 `init` E2E
3. 寫 `todo add/list/done` E2E
4. 寫 `review` E2E
5. 為 timer 增加 `--plain` 和 `--no-notify`
6. 寫 standalone timer E2E
7. 寫 todo timer E2E

## 驗收標準

第一版 E2E 測試完成時，應能用一條命令跑完主要非 TUI 流程：

```powershell
cargo test --test e2e
```

測試通過時應能證明：

- `tsml.exe` 能在乾淨目錄中自初始化
- 配置和數據確實寫在 `.exe` 旁邊
- Markdown 文件生成正確
- todo JSON 持久化正確
- todo 排程順序正確
- done 狀態流轉正確
- timer 的核心選擇邏輯可以被自動驗證
