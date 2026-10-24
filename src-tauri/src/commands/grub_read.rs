// What the window reads without any privilege: the settings, the entries of
// grub.cfg, the installed themes and fonts, the colors of custom.cfg, the
// preview images, and the backup it saves where the user chooses. Nothing
// here writes to the boot files; privileged.rs does that.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::grub_config::{self, MenuEntry};

const GRUB_DEFAULT_PATH: &str = "/etc/default/grub";
const GRUB_CFG_PATH: &str = "/boot/grub/grub.cfg";
const GRUB_D: &str = "/etc/grub.d";
const THEMES_DIR: &str = "/boot/grub/themes";
const FONTS_DIR: &str = "/boot/grub/fonts";
const CUSTOM_CFG_PATH: &str = "/boot/grub/custom.cfg";
const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrubPresence {
    pub has_config: bool,
    pub has_update_grub: bool,
}

/// Whether this system has GRUB to edit: a system that boots another way
/// (systemd-boot, for example) has no /etc/default/grub.
#[tauri::command]
pub fn grub_present() -> GrubPresence {
    let has_update_grub = ["/usr/sbin/update-grub", "/sbin/update-grub", "/usr/bin/update-grub"].iter().any(|p| Path::new(p).is_file());
    GrubPresence { has_config: Path::new(GRUB_DEFAULT_PATH).is_file(), has_update_grub }
}

#[tauri::command]
pub fn read_grub_config() -> Result<HashMap<String, String>, String> {
    std::fs::read_to_string(GRUB_DEFAULT_PATH).map(|text| grub_config::parse(&text)).map_err(|e| e.to_string())
}

fn current_platform() -> &'static str {
    if Path::new("/sys/firmware/efi").exists() {
        "efi"
    } else {
        "pc"
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntriesResult {
    pub entries: Vec<MenuEntry>,
    /// grub.cfg could not be read without a password.
    pub unreadable: bool,
}

// Disabled (commented out) memtest and custom entries do not appear in
// grub.cfg at all, so they would vanish from the list once switched off and
// could never be switched on or deleted. They are read from their script.
fn disabled_script_entries() -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Ok(dir) = std::fs::read_dir(GRUB_D) else { return found };
    let mut scripts: Vec<PathBuf> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().map(|n| n.to_string_lossy().contains("memtest") || n == "40_custom").unwrap_or(false))
        .collect();
    scripts.sort();
    for path in scripts {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let script = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for (title, enabled) in grub_config::script_titles(&text) {
            if !enabled {
                found.push((title, script.clone()));
            }
        }
    }
    found
}

/// The entries of grub.cfg. `cfg` is its text when the window already has it
/// (read as administrator); without it the file is read here.
#[tauri::command]
pub fn get_menu_entries(cfg: Option<String>) -> EntriesResult {
    let text = match cfg {
        Some(text) => text,
        None => match std::fs::read_to_string(GRUB_CFG_PATH) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return EntriesResult { entries: Vec::new(), unreadable: true },
            Err(_) => return EntriesResult { entries: Vec::new(), unreadable: false },
        },
    };
    let mut entries = grub_config::parse_menu_entries(&text, current_platform());
    for (title, script) in disabled_script_entries() {
        if entries.iter().any(|e| e.display_name == title) {
            continue;
        }
        let lower = title.to_lowercase();
        let path = if script.contains("memtest") {
            if lower.contains("ia32") {
                "/mt86+ia32"
            } else if lower.contains("x64") {
                "/mt86+x64"
            } else {
                ""
            }
        } else {
            ""
        };
        entries.push(MenuEntry {
            name: title.clone(),
            display_name: title,
            kind: if script.contains("memtest") { "memtest" } else { "system" }.to_string(),
            enabled: false,
            source_script: Some(script),
            path: path.to_string(),
        });
    }
    EntriesResult { entries, unreadable: false }
}

/// The themes installed in /boot/grub/themes (folders with a theme.txt).
#[tauri::command]
pub fn list_themes() -> Vec<String> {
    let mut themes: Vec<String> = std::fs::read_dir(THEMES_DIR)
        .map(|dir| dir.flatten().filter(|e| e.path().is_dir() && e.path().join("theme.txt").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    themes.sort();
    themes
}

/// The fonts installed in /boot/grub/fonts.
#[tauri::command]
pub fn list_fonts() -> Vec<String> {
    let mut fonts: Vec<String> = std::fs::read_dir(FONTS_DIR)
        .map(|dir| dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".pf2")).collect())
        .unwrap_or_default();
    fonts.sort();
    fonts
}

#[derive(Serialize, Default)]
pub struct CustomColors {
    pub normal: String,
    pub highlight: String,
}

/// The colors this program wrote to /boot/grub/custom.cfg.
#[tauri::command]
pub fn read_custom_colors() -> CustomColors {
    let mut colors = CustomColors::default();
    let Ok(text) = std::fs::read_to_string(CUSTOM_CFG_PATH) else { return colors };
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("set color_normal=") {
            colors.normal = value.trim().trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("set color_highlight=") {
            colors.highlight = value.trim().trim_matches('"').to_string();
        }
    }
    colors
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// An image the window can show, as a data URL: a PNG or JPEG the user can
/// read, of a reasonable size. None for anything else.
#[tauri::command]
pub fn read_image_preview(path: String) -> Option<String> {
    let real = std::fs::canonicalize(&path).ok()?;
    let mime = match real.extension()?.to_string_lossy().to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return None,
    };
    let meta = std::fs::metadata(&real).ok()?;
    if !meta.is_file() || meta.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let bytes = std::fs::read(&real).ok()?;
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

/// The preview of an installed theme: its preview image, or its background.
#[tauri::command]
pub fn read_theme_preview(name: String) -> Option<String> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return None;
    }
    let dir = Path::new(THEMES_DIR).join(&name);
    for file in ["preview.png", "preview.jpg", "background.png", "background.jpg", "background.jpeg"] {
        if let Some(url) = read_image_preview(dir.join(file).to_string_lossy().into_owned()) {
            return Some(url);
        }
    }
    None
}

/// A copy of /etc/default/grub in the file the user chose. It is the file
/// that holds the settings; it is readable by everyone, so no password is
/// asked.
#[tauri::command]
pub fn save_grub_backup(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if !target.is_absolute() || path.contains('\0') || target.is_dir() {
        return Err("Not a file".to_string());
    }
    if !target.parent().map(|p| p.is_dir()).unwrap_or(false) {
        return Err("The folder does not exist".to_string());
    }
    let text = std::fs::read_to_string(GRUB_DEFAULT_PATH).map_err(|e| e.to_string())?;
    std::fs::write(target, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_known_encodings() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn only_png_and_jpeg_are_previewed() {
        let dir = std::env::temp_dir().join(format!("soplos-grub-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), b"x").unwrap();
        std::fs::write(dir.join("a.tga"), b"x").unwrap();
        assert!(read_image_preview(dir.join("a.png").to_string_lossy().into_owned()).unwrap().starts_with("data:image/png;base64,"));
        assert!(read_image_preview(dir.join("a.tga").to_string_lossy().into_owned()).is_none());
        assert!(read_image_preview(dir.join("missing.png").to_string_lossy().into_owned()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_name_cannot_leave_the_themes_folder() {
        assert!(read_theme_preview("../../etc".to_string()).is_none());
        assert!(read_theme_preview(".hidden".to_string()).is_none());
        assert!(read_theme_preview(String::new()).is_none());
    }
}
