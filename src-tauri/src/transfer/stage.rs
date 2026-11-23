//! Where a phone's bytes wait for a device that is not there yet.
//!
//! Android hands the webview a `File`, not a path the Rust sender can open
//! later, so a file send that has to queue parks a copy in the app's config
//! directory. The outbox reads it from there when the peer comes back and
//! deletes it once it is across.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use uuid::Uuid;

/// Name of the staging folder inside the app's config directory.
const DIR: &str = "queued";
/// How much of the original name is kept for the peer to see.
const MAX_NAME_CHARS: usize = 60;

pub fn staged_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(DIR))
        .unwrap_or_else(|_| std::env::temp_dir().join("filedrop-queued"))
}

/// Only paths this module handed out are accepted: one file one level inside
/// the staging folder, so a string from the webview cannot reach anywhere else.
fn checked<'a>(root: &Path, path: &'a Path) -> Option<&'a Path> {
    let parent = path.parent()?;
    let mut rest = parent.strip_prefix(root).ok()?.components();
    match (rest.next(), rest.next()) {
        (Some(Component::Normal(_)), None) => Some(parent),
        _ => None,
    }
}

fn sanitize(name: &str) -> String {
    let flat = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();
    let trimmed = flat.trim().trim_matches('.');
    if trimmed.is_empty() {
        return "file".to_string();
    }
    trimmed.chars().take(MAX_NAME_CHARS).collect()
}

/// A zero-length copy named after the original file, ready for chunks. The id
/// folder keeps the name intact, because the peer is sent the file's own name.
pub fn open(root: &Path, name: &str) -> Result<PathBuf, String> {
    let stage = root.join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&stage).map_err(|error| format!("创建暂存目录失败：{error}"))?;
    let path = stage.join(sanitize(name));
    std::fs::File::create(&path).map_err(|error| format!("暂存文件失败：{error}"))?;
    Ok(path)
}

/// Adds one decoded chunk to a staged copy.
pub fn append(root: &Path, path: &str, chunk: &str) -> Result<(), String> {
    let file = Path::new(path);
    if checked(root, file).is_none() {
        return Err("暂存文件地址不可用".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(chunk)
        .map_err(|error| format!("暂存数据损坏：{error}"))?;
    let mut handle = std::fs::OpenOptions::new()
        .append(true)
        .open(file)
        .map_err(|error| format!("暂存写入失败：{error}"))?;
    handle
        .write_all(&bytes)
        .map_err(|error| format!("暂存写入失败：{error}"))?;
    handle.flush().map_err(|error| format!("暂存写入失败：{error}"))
}

/// Throws away a copy the sender gave up on, and its now-empty folder.
pub fn discard(root: &Path, path: &str) {
    let file = Path::new(path);
    let Some(stage) = checked(root, file) else {
        return;
    };
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_dir(stage);
}

/// Removes a staged copy once the peer has it — or never will. A desktop send
/// names the user's own file, so nothing is deleted there.
pub fn release(root: &Path, path: &str) {
    discard(root, path);
}

/// Anything left over from a staging run that never reached the queue.
pub fn purge(root: &Path, keep: &[String]) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let text = path.to_string_lossy().to_string();
        if !path.is_dir() {
            if !keep.contains(&text) {
                let _ = std::fs::remove_file(path);
            }
            continue;
        }
        if let Ok(files) = std::fs::read_dir(&path) {
            for file in files.flatten() {
                let nested = file.path();
                if !keep.contains(&nested.to_string_lossy().to_string()) {
                    let _ = std::fs::remove_file(nested);
                }
            }
        }
        let _ = std::fs::remove_dir(&path);
    }
}

// -------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("filedrop-stage-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn chunks_land_in_one_copy_that_keeps_its_name() {
        let dir = root("chunks");
        let file = open(&dir, "季度报告.docx").unwrap();
        assert_eq!(file.file_name().unwrap(), "季度报告.docx");
        append(&dir, file.to_str().unwrap(), &b64(b"first ")).unwrap();
        append(&dir, file.to_str().unwrap(), &b64(b"second")).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"first second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_outside_the_staging_folder_is_refused() {
        let dir = root("refuse");
        std::fs::create_dir_all(&dir).unwrap();
        let staged = open(&dir, "ok.txt").unwrap();
        let elsewhere = std::env::temp_dir().join("filedrop-stage-elsewhere.txt");
        std::fs::write(&elsewhere, b"do not touch").unwrap();
        for candidate in [
            elsewhere.to_str().unwrap().to_string(),
            dir.join("..").join("escape.txt").to_string_lossy().to_string(),
            staged.parent().unwrap().to_string_lossy().to_string(),
            "C:\\Windows\\notes.txt".to_string(),
        ] {
            assert!(
                append(&dir, &candidate, &b64(b"no")).is_err(),
                "{candidate} must not be writable"
            );
            discard(&dir, &candidate);
        }
        assert!(staged.exists(), "a refusal must not delete the real copy");
        assert!(elsewhere.exists(), "an outside file is never ours");
        std::fs::remove_file(elsewhere).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_release_only_spends_a_staged_copy() {
        let dir = root("release");
        let staged = open(&dir, "a.txt").unwrap();
        let stage = staged.parent().unwrap().to_path_buf();
        let original = std::env::temp_dir().join("filedrop-stage-original.txt");
        std::fs::write(&original, b"keep me").unwrap();

        release(&dir, staged.to_str().unwrap());
        release(&dir, original.to_str().unwrap());

        assert!(!staged.exists(), "a delivered copy is spent");
        assert!(!stage.exists(), "its folder goes with it");
        assert!(original.exists(), "the user's own file is not ours to delete");
        std::fs::remove_file(original).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_staging_run_that_died_leaves_no_bytes_behind() {
        let dir = root("purge");
        let kept = open(&dir, "kept.txt").unwrap();
        let orphan = open(&dir, "orphan.txt").unwrap();
        let empty = dir.join("dead-folder");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(dir.join("stray.txt"), b"x").unwrap();
        let keep = vec![kept.to_str().unwrap().to_string()];

        purge(&dir, &keep);

        assert!(kept.exists(), "a queued send still needs its bytes");
        assert!(!orphan.exists());
        assert!(!empty.exists());
        assert!(!dir.join("stray.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_is_stripped_of_directories_and_control_codes() {
        assert_eq!(sanitize("../../evil\r\n名.txt"), "evil名.txt");
        assert_eq!(sanitize("C:\\Users\\me\\photo.jpg"), "photo.jpg");
        assert_eq!(sanitize(""), "file");
        assert_eq!(sanitize("..."), "file");
        assert_eq!(sanitize(&"x".repeat(200)).chars().count(), MAX_NAME_CHARS);
    }
}
