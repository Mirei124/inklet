//! scene.json / settings.json 的原子存储。
//!
//! 数据目录（Linux）：~/.local/share/inklet/
//! 原子写入：先写 scene.json.tmp -> flush -> rename 到 scene.json，
//! 保证中断不会损坏现有文件。scene.json 损坏时保留原文件、返回错误，
//! 上层回退到空 scene 启动。

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Storage {
    dir: PathBuf,
}

#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    /// 内容是非法 JSON（save 时前端发来的数据无效）。
    InvalidJson(serde_json::Error),
    /// 已有 scene.json 是损坏 JSON（load 时）。
    Corrupt,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Io(e) => write!(f, "io error: {e}"),
            StorageError::InvalidJson(e) => write!(f, "invalid json: {e}"),
            StorageError::Corrupt => write!(f, "scene.json is corrupt"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e)
    }
}

impl Storage {
    /// 显式指定数据目录（测试用）。
    #[allow(dead_code)]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// 默认数据目录：~/.local/share/inklet/
    pub fn default() -> Self {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("inklet");
        Self { dir }
    }

    pub fn scene_path(&self) -> PathBuf {
        self.dir.join("scene.json")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    /// 读取 settings.json；不存在返回 `Ok(None)`，内容损坏返回 `Err(Corrupt)`。
    pub fn load_settings(&self) -> Result<Option<String>, StorageError> {
        let path = self.settings_path();
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StorageError::Io(e)),
        };
        serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| StorageError::Corrupt)?;
        Ok(Some(raw))
    }

    /// 原子保存 settings.json。
    pub fn save_settings(&self, json: &str) -> Result<(), StorageError> {
        serde_json::from_str::<serde_json::Value>(json).map_err(StorageError::InvalidJson)?;
        self.atomic_write(&self.settings_path(), json.as_bytes())
    }

    /// 读取 scene.json；文件不存在返回 `Ok(None)`，内容损坏返回 `Err(Corrupt)`。
    pub fn load_scene(&self) -> Result<Option<String>, StorageError> {
        let path = self.scene_path();
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StorageError::Io(e)),
        };
        // 只做轻量 JSON 校验；损坏时保留原文件，让上层回退空 scene。
        serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| StorageError::Corrupt)?;
        Ok(Some(raw))
    }

    /// 原子保存 scene.json：tmp -> flush -> rename。
    pub fn save_scene(&self, json: &str) -> Result<(), StorageError> {
        serde_json::from_str::<serde_json::Value>(json).map_err(StorageError::InvalidJson)?;
        self.atomic_write(&self.scene_path(), json.as_bytes())
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> Result<(), StorageError> {
        fs::create_dir_all(&self.dir)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("scene.json");
        let tmp = self.dir.join(format!("{file_name}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_storage() -> (Storage, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = Storage::new(dir.path().to_path_buf());
        (storage, dir)
    }

    #[test]
    fn missing_scene_returns_none() {
        let (storage, _dir) = temp_storage();
        assert!(matches!(storage.load_scene(), Ok(None)));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let (storage, _dir) = temp_storage();
        let json = r#"{"version":1,"elements":[{"id":"a","type":"rectangle"}],"appState":{}}"#;
        storage.save_scene(json).expect("save");
        assert_eq!(storage.load_scene().expect("load").as_deref(), Some(json));
        // 数据目录里只应有 scene.json，没有残留 tmp
        assert!(!storage.dir.join("scene.json.tmp").exists());
    }

    #[test]
    fn corrupt_scene_is_detected_and_preserved() {
        let (storage, _dir) = temp_storage();
        fs::write(storage.scene_path(), "{ not valid json").expect("write");
        assert!(matches!(storage.load_scene(), Err(StorageError::Corrupt)));
        // 原损坏文件保留，不覆盖
        let raw = fs::read_to_string(storage.scene_path()).expect("read");
        assert_eq!(raw, "{ not valid json");
    }

    #[test]
    fn invalid_json_is_rejected_on_save() {
        let (storage, _dir) = temp_storage();
        assert!(matches!(
            storage.save_scene("not json"),
            Err(StorageError::InvalidJson(_))
        ));
        assert!(!storage.scene_path().exists());
    }
}
