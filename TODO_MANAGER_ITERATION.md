# Todo Manager Iteration Notes

這份文檔記錄下一輪迭代的改動思路，重點是先打磨 todo 管理體驗，暫時不大幅重構 timer，也不急著進入 GUI。

## 判斷

目前 `to-see-my-life` 已經有可用的 CLI 骨架：todo、timer、review、today、stats 都已落地。下一步最有價值的方向不是直接做 GUI，而是先把 todo 管理能力穩定下來，讓 CLI 本身形成每天可用的個人管理閉環。

GUI 需要的不是把現有 CLI 函數包一層，而是能調用一組穩定的 todo 管理接口。因此下一步抽象應該集中在 todo 管理，而不是一次性拆出過細的 domain / app / storage / ui 目錄。

## 不做的事

- 不把 timer 改成後台 daemon。
- 不把 todo、timer、review、stats 大幅拆散成多層架構。
- 不為了 GUI 預先引入新的框架或服務進程。
- 不改變現有數據文件格式，除非某個功能確實需要。

本輪 `today` 工作台需要支持未排程 TODO，因此 `Todo.start` / `Todo.end` 演進為可選字段；已有 JSON 中的時間字段仍可兼容讀取。

## 保留的產品假設

timer 繼續保留佔用終端的模式。這符合目前的工作流：終端本身就是專注入口，用戶需要其他終端時可以開 new pane。

後台計時可以作為未來方向，但它會引入進程生命週期、狀態恢復、多 timer 防重入、跨平台通知等問題。現在不應該為這件事付出過多架構成本。

## 核心抽象

下一步可以引入一個輕量的 `TodoManager`。它仍然可以留在 `src/todo.rs` 裡，不需要立刻拆文件。

目標是把「todo 管理能力」從 CLI 交互中分離出來：

```rust
pub struct TodoManager;

impl TodoManager {
    pub fn add(&self, req: AddTodoRequest) -> Result<Todo>;
    pub fn list(&self, filter: TodoFilter) -> Result<Vec<Todo>>;
    pub fn done(&self, id: &str) -> Result<Todo>;
    pub fn cancel(&self, id: &str) -> Result<Todo>;
    pub fn delete(&self, id: &str) -> Result<Todo>;
    pub fn edit(&self, id: &str, req: EditTodoRequest) -> Result<Todo>;
    pub fn reschedule(&self, id: &str, req: RescheduleTodoRequest) -> Result<Todo>;
    pub fn summarize_day(&self, date: NaiveDate) -> Result<DaySummary>;
    pub fn find_timer_todo(&self, now: DateTime<Local>) -> Result<Option<Todo>>;
    pub fn record_focus(&self, id: &str, focused_minutes: i64, done: bool) -> Result<Todo>;
}
```

CLI 函數負責收集輸入、打印結果、做互動確認；`TodoManager` 負責讀寫 todo、校驗規則、返回結構化結果。

## 第一輪改動範圍

第一輪建議控制在 `src/todo.rs` 內完成：

- 新增 request/filter 類型，例如 `AddTodoRequest`、`EditTodoRequest`、`RescheduleTodoRequest`、`TodoFilter`。
- 新增 `TodoManager`，先復用現有 `load_todos` / `save_todos`。
- 將 `add`、`done`、`cancel`、`edit`、`reschedule` 的核心邏輯搬入 manager 方法。
- 保留現有 CLI 函數名，讓 `main.rs` 暫時不需要大改。
- CLI 函數改成 thin wrapper：收集參數和互動輸入，調 manager，打印結果。

## 後續產品功能

抽象穩定後，優先打磨這三塊：

1. `today` 作為主入口
   - 展示今日時間線。
   - 支持快速 done / cancel / reschedule。
   - 支持開始當前或下一個 todo timer。

2. rollover / defer
   - 支持把過期或未完成任務移到今天晚些時候、明天、或指定日期。
   - 這會比單純 `reschedule` 更符合日常使用。

3. review 模板
   - 支持配置 daily review 問題。
   - 保留自動 todo 摘要。
   - 未來可以讓 review 根據今日任務狀態提出更具體的提示。

## GUI 兼容性

這個抽象方式對 GUI 友好的原因是：GUI 可以直接調 `TodoManager`，不用復用 CLI 的 `println`、`dialoguer`、`Select` 等終端交互。

例如 overlap 不應該長期只是一個錯誤字符串，而應該逐步變成結構化錯誤：

```rust
TodoError::TimeOverlap {
    conflicts: Vec<Todo>,
}
```

CLI 可以打印衝突列表，GUI 可以在時間線上高亮衝突區塊。這是後續演進點，不必第一輪就全部完成。

## Current Implementation Notes

本輪先實作工作台行為，不抽 `TodoManager`：

- `today` 顯示 TODO / DOING / DONE 三欄。
- `today add` 建立今日未排程 TODO。
- `today start` 把 TODO 排程到現在並啟動終端 timer。
- `today defer` 把未完成 TODO 延到 `tomorrow`、`today` 或指定日期。
- 打開 `today` 時，已過 deferred 日期的未完成 TODO 會自動 carry over 到今天。
- timer 每次結束或退出都寫入 `timer_sessions.json`。
- review 會追加今日 timer session 摘要。
