//! Windows shell integration: the `FileDrop 传送` verb in Explorer's context
//! menu, and the file list that arrives through it.
//!
//! The verb lives under HKCU, so registering it needs no elevation and removing
//! it is a single key delete. The registry itself is the source of truth for
//! whether the menu is on — the state survives a moved executable and a
//! settings file written by another machine.

use std::path::Path;
use std::sync::Mutex;

// Only the registry writes name these, and those live behind the Windows cfg.
#[cfg(target_os = "windows")]
pub const VERB_KEY: &str = r"Software\Classes\*\shell\FileDrop.Send";
#[cfg(target_os = "windows")]
pub const VERB_LABEL: &str = "FileDrop 传送";

/// Files handed over by Explorer, before checking they still exist. Kept separate
/// from `send_paths` so tests can assert on the parsing without touching disk.
pub fn parse_send_args(args: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut collecting = false;
    for arg in args.iter().skip(1) {
        if arg == "--send-to" {
            collecting = true;
            continue;
        }
        if arg.starts_with("--") {
            collecting = false;
            continue;
        }
        if collecting {
            paths.push(arg.clone());
        }
    }
    paths
}

/// Files handed over by Explorer. A file can be renamed or deleted between the
/// right-click and the second process waking the tray instance, so anything that
/// no longer resolves to a real file is dropped here.
pub fn send_paths(args: &[String]) -> Vec<String> {
    parse_send_args(args)
        .into_iter()
        .filter(|arg| Path::new(arg).is_file())
        .collect()
}

/// Paths waiting for the window to ask for them: a cold start cannot emit an
/// event before the webview has subscribed, so the queue is drained instead.
#[derive(Default)]
pub struct SendQueue {
    paths: Mutex<Vec<String>>,
}

impl SendQueue {
    pub fn push(&self, paths: Vec<String>) -> usize {
        let Ok(mut queue) = self.paths.lock() else {
            return 0;
        };
        for path in paths {
            if !queue.contains(&path) {
                queue.push(path);
            }
        }
        queue.len()
    }

    pub fn take(&self) -> Vec<String> {
        self.paths.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default()
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuState {
    pub supported: bool,
    pub installed: bool,
    pub command: String,
}

pub fn state(exe: &Path) -> MenuState {
    #[cfg(windows)]
    {
        let command = read_command();
        MenuState {
            supported: true,
            installed: command
                .as_deref()
                .map(|text| text.contains(&exe.to_string_lossy().to_string()))
                .unwrap_or(false),
            command: command.unwrap_or_default(),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        MenuState { supported: false, installed: false, command: String::new() }
    }
}

/// Points the verb at this executable. Re-running it is how a moved or updated
/// install refreshes the command line.
pub fn install(exe: &Path) -> Result<MenuState, String> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let root = RegKey::predef(HKEY_CURRENT_USER);
        let path = exe.to_string_lossy().to_string();
        let (verb, _) = root
            .create_subkey(VERB_KEY)
            .map_err(|error| format!("写入右键菜单失败：{error}"))?;
        verb
            .set_value("", &format!("{VERB_LABEL}"))
            .map_err(|error| format!("写入菜单名称失败：{error}"))?;
        // Both: Explorer appends every selected file instead of launching one
        // process per file, which is what a multi-file send needs.
        verb.set_value("MultiSelectModel", &"Both")
            .map_err(|error| format!("写入多选支持失败：{error}"))?;
        verb.set_value("Icon", &format!("\"{path}\",0"))
            .map_err(|error| format!("写入图标失败：{error}"))?;
        let (command, _) = verb
            .create_subkey("Command")
            .map_err(|error| format!("写入命令失败：{error}"))?;
        command
            .set_value("", &format!("\"{path}\" --send-to \"%1\""))
            .map_err(|error| format!("写入命令失败：{error}"))?;
        Ok(state(exe))
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err("当前平台没有文件右键菜单".to_string())
    }
}

pub fn remove(exe: &Path) -> Result<MenuState, String> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let root = RegKey::predef(HKEY_CURRENT_USER);
        // Only this app's verb, and only when it is ours: never delete a key a
        // different program put in the same place.
        if let Some(text) = read_command() {
            let ours = text.contains(&exe.to_string_lossy().to_string());
            if ours {
                // Takes the verb and its Command subkey together.
                root.delete_subkey_all(VERB_KEY)
                    .map_err(|error| format!("删除菜单项失败：{error}"))?;
            }
        }
        Ok(state(exe))
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err("当前平台没有文件右键菜单".to_string())
    }
}

#[cfg(windows)]
fn read_command() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!("{VERB_KEY}\\Command"))
        .ok()?
        .get_value::<String, _>("")
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn verb_arguments_become_the_file_list() {
        let this = r"C:\apps\filedrop.exe".to_string();
        let paths = parse_send_args(&args(&[
            &this,
            "--send-to",
            "C:\\dir\\a.txt",
            "C:\\dir\\b.txt",
        ]));
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("a.txt"));
    }

    #[test]
    fn ordinary_launch_has_no_files() {
        assert!(parse_send_args(&args(&["filedrop.exe"])).is_empty());
        assert!(parse_send_args(&args(&["filedrop.exe", "--flag"])).is_empty());
    }

    #[test]
    fn missing_files_are_dropped() {
        let paths = send_paths(&args(&["filedrop.exe", "--send-to", "Z:\\nope.bin"]));
        assert!(paths.is_empty());
    }
}
