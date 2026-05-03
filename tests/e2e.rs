// tests/e2e.rs — CLI end-to-end 整合測試。
// 所有測試透過 TSML_HOME 隔離數據目錄，不污染 portable 預設配置。
// 測試前手動構造 config.toml 和 todos.json 等文件，避免依賴互動式初始化。

#![allow(deprecated)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// 在 TSML_HOME 臨時目錄中建立 tsml 所需的配置文件。
/// TSML_HOME 本身即為數據根目錄（不附加 .config 子目錄）。
fn setup(tmp: &Path) {
    let notes_dir = tmp.join("notes");
    fs::create_dir_all(&notes_dir).unwrap();
    // Path::to_string_lossy 處理 Windows 路徑中的反斜槓
    let notes_str = notes_dir.to_string_lossy().replace('\\', "\\\\");
    let config_toml = format!("notes_dir = \"{notes_str}\"\n");
    fs::write(tmp.join("config.toml"), &config_toml).unwrap();
    fs::write(tmp.join("todos.json"), "[]\n").unwrap();
}

/// 建立 tsml 指令並配置 TSML_HOME 隔離目錄，返回 (Command, 隔離目錄路徑)。
fn tsml_cmd() -> (Command, PathBuf) {
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    let tmp = tempfile::tempdir().unwrap().into_path();
    cmd.env("TSML_HOME", &tmp);
    setup(&tmp);
    (cmd, tmp)
}

// ── config show ──────────────────────────────────────────────────

#[test]
fn e2e_config_show() {
    let tmp = tempfile::tempdir().unwrap().into_path();
    setup(&tmp);
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("config:"))
        .stdout(predicate::str::contains("notes_dir:"))
        .stdout(predicate::str::contains("todos:"));
}

// ── todo add / list / done ──────────────────────────────────────

#[test]
fn e2e_todo_add_list_done() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("測試任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("1m")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("測試任務"));

    // list
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("測試任務"));

    // done
    let id = read_first_id(&tmp);
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("done")
        .arg(&id)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Marked {id} done")));

    // done 後 list 默認不顯示
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No todos."));

    // --all 顯示
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("list")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

// ── todo cancel / delete ────────────────────────────────────────

#[test]
fn e2e_todo_cancel_delete() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("可取消任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("1m")
        .arg("--force")
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("add")
        .arg("可刪除任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("1m")
        .arg("--force")
        .assert()
        .success();

    let id1 = read_first_id(&tmp);
    let id2 = read_second_id(&tmp);

    // cancel
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("cancel")
        .arg(&id1)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Cancelled {id1}")));

    // delete with --yes
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("delete")
        .arg(&id2)
        .arg("--yes")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Deleted {id2}")));

    // 只剩一個 cancelled
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("list")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains("cancelled"));
}

// ── todo edit / reschedule ──────────────────────────────────────

#[test]
fn e2e_todo_edit_reschedule() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("舊標題")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("25m")
        .arg("--force")
        .assert()
        .success();

    let id = read_first_id(&tmp);

    // edit
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("edit")
        .arg(&id)
        .arg("--title")
        .arg("新標題")
        .arg("--duration")
        .arg("30m")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Edited {id}")));

    // verify JSON
    let todos = read_todos(&tmp);
    let edited = todos.into_iter().find(|t| t["id"] == id).unwrap();
    assert_eq!(edited["title"], "新標題");
    assert_eq!(edited["duration_minutes"], 30);

    // reschedule
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("reschedule")
        .arg(&id)
        .arg("--start")
        .arg("now")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Rescheduled {id}")));
}

// ── overlap 檢查 ─────────────────────────────────────────────────

#[test]
fn e2e_overlap_rejected() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("第一件")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("30m")
        .arg("--force")
        .assert()
        .success();

    // without --force → fail
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("add")
        .arg("第二件")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("30m")
        .assert()
        .failure()
        .stderr(predicate::str::contains("overlap"));

    // with --force → success
    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("todo")
        .arg("add")
        .arg("第二件")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("30m")
        .arg("--force")
        .assert()
        .success();
}

// ── today ───────────────────────────────────────────────────────

#[test]
fn e2e_today() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("今日任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("1m")
        .arg("--force")
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Today"))
        .stdout(predicate::str::contains("今日任務"));
}

// ── timer --plain --no-notify ────────────────────────────────────

#[test]
fn e2e_timer_plain() {
    let (mut cmd, _tmp) = tsml_cmd();
    cmd.arg("timer")
        .arg("--title")
        .arg("測試定時器")
        .arg("--duration")
        .arg("1s")
        .arg("--plain")
        .arg("--no-notify")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("Timer finished: 測試定時器"));
}

// ── stats --week ────────────────────────────────────────────────

#[test]
fn e2e_stats_week() {
    let (mut cmd, tmp) = tsml_cmd();

    cmd.arg("todo")
        .arg("add")
        .arg("統計任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("1m")
        .arg("--force")
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("tsml").unwrap();
    cmd.env("TSML_HOME", &tmp)
        .arg("stats")
        .arg("--week")
        .assert()
        .success()
        .stdout(predicate::str::contains("Todos:"))
        .stdout(predicate::str::contains("統計任務"));
}

// ── 非法輸入 ─────────────────────────────────────────────────────

#[test]
fn e2e_invalid_duration() {
    let (mut cmd, _tmp) = tsml_cmd();
    cmd.arg("todo")
        .arg("add")
        .arg("錯誤任務")
        .arg("--start")
        .arg("now")
        .arg("--duration")
        .arg("abc")
        .assert()
        .failure();
}

#[test]
fn e2e_invalid_start() {
    let (mut cmd, _tmp) = tsml_cmd();
    cmd.arg("todo")
        .arg("add")
        .arg("錯誤任務")
        .arg("--start")
        .arg("nope")
        .arg("--duration")
        .arg("30m")
        .arg("--force")
        .assert()
        .failure()
        .stderr(predicate::str::contains("HH:MM"));
}

#[test]
fn e2e_zero_duration() {
    let (mut cmd, _tmp) = tsml_cmd();
    cmd.arg("timer")
        .arg("--duration")
        .arg("0m")
        .arg("--plain")
        .arg("--no-notify")
        .assert()
        .failure();
}

// ── 輔助函數 ─────────────────────────────────────────────────────

fn read_todos(tmp: &Path) -> Vec<serde_json::Value> {
    let raw = fs::read_to_string(tmp.join("todos.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn read_first_id(tmp: &Path) -> String {
    read_todos(tmp)[0]["id"].as_str().unwrap().to_string()
}

fn read_second_id(tmp: &Path) -> String {
    read_todos(tmp)[1]["id"].as_str().unwrap().to_string()
}
