// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod desktop;
mod ipc;
mod storage;

fn main() {
    if let Err(err) = app::run() {
        // 关键平台初始化失败不能被静默忽略（spec §20）
        eprintln!("fatal: {err}");
        std::process::exit(1);
    }
}
