use std::path::Path;

use grain_id::GrainId;

use crate::error::{Error, Result};
use crate::model::{Activity, Stamp};

pub fn activity_path(root: &Path, id: GrainId) -> std::path::PathBuf {
    root.join("activities").join(format!("{id}.toml"))
}

pub fn stamp_path(root: &Path, id: GrainId) -> std::path::PathBuf {
    root.join("stamps").join(format!("{id}.toml"))
}

pub fn read_activity(root: &Path, id: GrainId) -> Result<Activity> {
    let path = activity_path(root, id);
    let text = std::fs::read_to_string(&path)?;
    let mut a: Activity = toml::from_str(&text).map_err(|e| Error::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    if a.id != id {
        tracing::warn!(
            path = %path.display(),
            file_id = %a.id,
            filename_id = %id,
            "activity id mismatch; using filename id"
        );
        a.id = id;
    }
    Ok(a)
}

pub fn read_stamp(root: &Path, id: GrainId) -> Result<Stamp> {
    let path = stamp_path(root, id);
    let text = std::fs::read_to_string(&path)?;
    let mut s: Stamp = toml::from_str(&text).map_err(|e| Error::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    if s.id != id {
        tracing::warn!(
            path = %path.display(),
            file_id = %s.id,
            filename_id = %id,
            "stamp id mismatch; using filename id"
        );
        s.id = id;
    }
    Ok(s)
}

/// 新規ファイルのみ書く。既存があれば AlreadyExists。
pub fn write_activity(root: &Path, a: &Activity) -> Result<()> {
    let path = activity_path(root, a.id);
    if path.exists() {
        return Err(Error::AlreadyExists(path.display().to_string()));
    }
    std::fs::write(&path, toml::to_string_pretty(a).unwrap() + "\n")?;
    Ok(())
}

pub fn write_stamp(root: &Path, s: &Stamp) -> Result<()> {
    let path = stamp_path(root, s.id);
    if path.exists() {
        return Err(Error::AlreadyExists(path.display().to_string()));
    }
    std::fs::write(&path, toml::to_string_pretty(s).unwrap() + "\n")?;
    Ok(())
}
