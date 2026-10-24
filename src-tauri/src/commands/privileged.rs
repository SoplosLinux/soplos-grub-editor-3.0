// Privileged operations. The app never hands a shell script to root: the
// frontend sends a list of named operations, this module validates them on
// the user side, writes them as JSON to a private file and re-invokes this
// same binary under pkexec as `--run-ops <file>`. The root side validates
// every operation again, against the live system, before running it (the
// file is only a request) and executes each one with a fixed argv, never
// through a shell. Several operations travel in one pkexec call, so a batch
// asks for the password once.
//
// Every file this module writes is written to a temporary file next to it
// and renamed over the target, so a failure never leaves a half-written
// boot file. /etc/default/grub is also checked to be valid shell before it
// replaces the old one, and the custom boot entries are checked with
// grub-script-check. No backup files are left behind: the window offers to
// save a copy where the user chooses.
//
// The root side prints one `@@op <index> ok|fail` line per operation so the
// frontend can tell which ones succeeded, plus `@@theme`, `@@font` and
// `@@cfg` lines for the operations that answer with data.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::grub_config;

const GRUB_DEFAULT_PATH: &str = "/etc/default/grub";
const GRUB_CFG_PATH: &str = "/boot/grub/grub.cfg";
const CUSTOM_CFG_PATH: &str = "/boot/grub/custom.cfg";
const GRUB_D: &str = "/etc/grub.d";
const THEMES_DIR: &str = "/boot/grub/themes";
const FONTS_DIR: &str = "/boot/grub/fonts";
const THEME_SCRIPTS: &[&str] = &["/etc/grub.d/05_debian_theme", "/etc/grub.d/05_soplos_theme"];
const BACKGROUND_CACHE: &[&str] = &[
    "/boot/grub/.background_cache.jpeg",
    "/boot/grub/.background_cache.png",
    "/boot/grub/.background_cache.tga",
    "/boot/grub/.background_cache.jpg",
];
const DEFAULT_CUSTOM_SCRIPT: &str = "#!/bin/sh\nexec tail -n +3 $0\n# This file provides an easy way to add custom menu entries.  Simply type the\n# menu entries you want to add after this comment.  Be careful not to change\n# the 'exec tail' line above.\n";

const MAX_OPS_FILE_BYTES: u64 = 1_000_000;
const MAX_ARCHIVE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_THEME_BYTES: u64 = 300 * 1024 * 1024;
const MAX_THEME_FILES: usize = 5000;
const MAX_FONT_BYTES: u64 = 50 * 1024 * 1024;
const MAX_CUSTOM_ENTRY_BYTES: usize = 16 * 1024;
const MAX_CFG_BYTES: u64 = 4 * 1024 * 1024;

const ARCHIVE_EXTENSIONS: &[&str] = &[".zip", ".tar.gz", ".tgz", ".tar.xz", ".tar.bz2"];
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "pcf"];

#[derive(Serialize, Deserialize, Clone)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Op {
    // Keys of /etc/default/grub; an empty value removes the key.
    SaveConfig { settings: Vec<Setting> },
    UpdateGrub,
    // Turns the 05_debian_theme and 05_soplos_theme scripts on or off; off
    // also removes the background images they cache.
    SetThemeScripts { active: bool },
    // The colors of /boot/grub/custom.cfg, as "text/background"; both empty
    // clears the file.
    WriteColors { normal: String, highlight: String },
    // Unpacks a theme archive as root in a private folder, checks it and
    // copies it into /boot/grub/themes. Prints `@@theme <name>`.
    InstallTheme { archive: String },
    RemoveTheme { name: String },
    // Converts a font to GRUB's format. Prints `@@font <name>`.
    ConvertFont { source: String, size: u32 },
    RemoveFont { name: String },
    // Appends a menuentry block to /etc/grub.d/40_custom.
    AppendCustomEntry { text: String },
    SetEntryEnabled { script: String, title: String, enabled: bool },
    DeleteEntry { script: String, title: String },
    // The memtest binary in /boot ("mt86+x64"); its entries disappear at the
    // next update-grub.
    DeleteMemtest { binary: String },
    // grub.cfg is not readable by everyone on every system: one `@@cfg
    // <line>` for each of its lines.
    ReadGrubCfg,
}

// ---------------------------------------------------------------------------
// Checks of a request that need no disk

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && !name.starts_with('.')
        && !name.chars().any(|c| c == '/' || c.is_control())
}

fn valid_title(title: &str) -> bool {
    !title.is_empty() && title.len() <= 256 && !title.chars().any(|c| c.is_control())
}

fn absolute_without_parent(raw: &str) -> bool {
    raw.len() <= 4096 && !raw.contains('\0') && Path::new(raw).is_absolute() && !Path::new(raw).components().any(|c| matches!(c, std::path::Component::ParentDir))
}

fn memtest_binary(name: &str) -> bool {
    match name.strip_prefix("mt86+") {
        Some(rest) => !rest.is_empty() && rest.len() <= 12 && rest.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

fn entry_script(script: &str) -> bool {
    valid_file_name(script) && (script == "40_custom" || script.contains("memtest"))
}

fn archive_extension(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    ARCHIVE_EXTENSIONS.iter().copied().find(|ext| lower.ends_with(ext))
}

fn font_extension_ok(path: &str) -> bool {
    Path::new(path).extension().map(|e| FONT_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false)
}

/// User-side check: the shape of a request.
pub fn validate(op: &Op) -> Result<(), String> {
    match op {
        Op::SaveConfig { settings } => {
            if settings.is_empty() {
                return Err("No settings given".into());
            }
            for (index, setting) in settings.iter().enumerate() {
                grub_config::validate_setting(&setting.key, &setting.value)?;
                if settings[..index].iter().any(|s| s.key == setting.key) {
                    return Err(format!("{} is given twice", setting.key));
                }
            }
            Ok(())
        }
        Op::UpdateGrub | Op::SetThemeScripts { .. } | Op::ReadGrubCfg => Ok(()),
        Op::WriteColors { normal, highlight } => {
            let ok = |value: &str| value.is_empty() || grub_config::validate_setting("GRUB_COLOR_NORMAL", value).is_ok();
            if ok(normal) && ok(highlight) && normal.is_empty() == highlight.is_empty() {
                Ok(())
            } else {
                Err("Not a pair of GRUB colors".into())
            }
        }
        Op::InstallTheme { archive } => {
            if absolute_without_parent(archive) && archive_extension(archive).is_some() {
                Ok(())
            } else {
                Err("Not a theme archive".into())
            }
        }
        Op::RemoveTheme { name } => {
            if valid_file_name(name) {
                Ok(())
            } else {
                Err("Not a theme name".into())
            }
        }
        Op::ConvertFont { source, size } => {
            if absolute_without_parent(source) && font_extension_ok(source) && (8..=72).contains(size) {
                Ok(())
            } else {
                Err("Not a font file or size".into())
            }
        }
        Op::RemoveFont { name } => {
            if valid_file_name(name) && name.ends_with(".pf2") {
                Ok(())
            } else {
                Err("Not a GRUB font".into())
            }
        }
        Op::AppendCustomEntry { text } => {
            if !text.trim().is_empty() && text.len() <= MAX_CUSTOM_ENTRY_BYTES && !text.contains('\0') && text.contains("menuentry") {
                Ok(())
            } else {
                Err("Not a menu entry".into())
            }
        }
        Op::SetEntryEnabled { script, title, .. } => {
            if entry_script(script) && valid_title(title) {
                Ok(())
            } else {
                Err("Not an entry this program manages".into())
            }
        }
        Op::DeleteEntry { script, title } => {
            if script == "40_custom" && valid_title(title) {
                Ok(())
            } else {
                Err("Only the entries of 40_custom can be deleted".into())
            }
        }
        Op::DeleteMemtest { binary } => {
            if memtest_binary(binary) {
                Ok(())
            } else {
                Err("Not a memtest binary".into())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers of the root side

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

fn is_real_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn tool(names: &[&str]) -> Option<PathBuf> {
    ["/usr/sbin", "/usr/bin", "/sbin", "/bin"].iter().flat_map(|dir| names.iter().map(move |name| Path::new(dir).join(name))).find(|p| p.is_file())
}

/// A folder only root can read, removed when it goes out of scope.
struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new(label: &str) -> std::io::Result<PrivateDir> {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path = Path::new("/var/tmp").join(format!("soplos-grub-editor-{label}-{}-{nanos}", std::process::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(PrivateDir(path))
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Writes `content` to a temporary file beside `path` and renames it over
/// the target. The target keeps its mode; a new file gets `default_mode`.
fn write_atomic(path: &Path, content: &str, default_mode: u32) -> Result<(), String> {
    let mode = std::fs::metadata(path).map(|m| m.permissions().mode() & 0o7777).unwrap_or(default_mode);
    let dir = path.parent().ok_or("No folder")?;
    let name = path.file_name().ok_or("No file name")?.to_string_lossy().into_owned();
    let temp = dir.join(format!(".{name}.soplos-tmp-{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(mode))?;
        std::fs::rename(&temp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("could not write {}: {e}", path.display()));
    }
    Ok(())
}

/// Whether `text` is valid shell: /etc/default/grub is sourced by one.
fn shell_syntax_ok(text: &str) -> bool {
    let Ok(mut child) = Command::new("sh").arg("-n").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() {
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn run(program: &Path, args: &[&str]) -> bool {
    match Command::new(program).args(args).status() {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("soplos-grub-editor: could not run {}: {e}", program.display());
            false
        }
    }
}

// ---------------------------------------------------------------------------
// The operations

fn save_config(settings: &[Setting]) -> bool {
    let original = std::fs::read_to_string(GRUB_DEFAULT_PATH).unwrap_or_default();
    let changes: Vec<(String, String)> = settings.iter().map(|s| (s.key.clone(), s.value.clone())).collect();
    let updated = grub_config::apply_changes(&original, &changes);
    if !shell_syntax_ok(&updated) {
        eprintln!("soplos-grub-editor: the new {GRUB_DEFAULT_PATH} is not valid shell, nothing was written");
        return false;
    }
    match write_atomic(Path::new(GRUB_DEFAULT_PATH), &updated, 0o644) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("soplos-grub-editor: {e}");
            false
        }
    }
}

fn update_grub() -> bool {
    match tool(&["update-grub"]) {
        Some(path) => run(&path, &[]),
        None => {
            eprintln!("soplos-grub-editor: update-grub was not found");
            false
        }
    }
}

fn set_theme_scripts(active: bool) -> bool {
    let mut ok = true;
    for script in THEME_SCRIPTS {
        let path = Path::new(script);
        if !is_real_file(path) {
            continue;
        }
        let result = std::fs::metadata(path).and_then(|m| {
            let mode = m.permissions().mode() & 0o7777;
            let wanted = if active { mode | 0o111 } else { mode & !0o111 };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(wanted))
        });
        if let Err(e) = result {
            eprintln!("soplos-grub-editor: could not change {script}: {e}");
            ok = false;
        }
    }
    // The images the scripts cache can make GRUB refuse the background (a
    // progressive JPEG gives "invalid argument"), so they go with the scripts.
    if !active {
        for cache in BACKGROUND_CACHE {
            let _ = std::fs::remove_file(cache);
        }
    }
    ok
}

fn write_colors(normal: &str, highlight: &str) -> bool {
    let content = if normal.is_empty() {
        "# Custom settings cleared by Soplos GRUB Editor\n".to_string()
    } else {
        format!("# Generated by Soplos GRUB Editor\n# Custom appearance settings\n\nset color_normal={normal}\nset color_highlight={highlight}\n")
    };
    if !is_real_dir(Path::new("/boot/grub")) {
        eprintln!("soplos-grub-editor: /boot/grub does not exist");
        return false;
    }
    match write_atomic(Path::new(CUSTOM_CFG_PATH), &content, 0o644) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("soplos-grub-editor: {e}");
            false
        }
    }
}

/// A name made only of letters, digits and . _ + -; anything else becomes _.
fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') { c } else { '_' }).collect();
    let trimmed = cleaned.trim_start_matches(['.', '-']);
    trimmed.chars().take(64).collect()
}

fn archive_stem(path: &str) -> String {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match archive_extension(&name) {
        Some(ext) => name[..name.len() - ext.len()].to_string(),
        None => name,
    }
}

// What an unpacked theme is made of, or why it is refused: only regular files
// and folders, a bounded size and count.
fn check_tree(root: &Path) -> Result<(u64, usize), String> {
    fn walk(dir: &Path, total: &mut u64, count: &mut usize) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.is_dir() {
                walk(&entry.path(), total, count)?;
            } else if meta.is_file() {
                *total += meta.len();
                *count += 1;
            } else {
                return Err(format!("The archive holds something that is not a plain file: {}", entry.path().display()));
            }
            if *total > MAX_THEME_BYTES || *count > MAX_THEME_FILES {
                return Err("The archive is too large for a theme".to_string());
            }
        }
        Ok(())
    }
    let mut total = 0;
    let mut count = 0;
    walk(root, &mut total, &mut count)?;
    Ok((total, count))
}

// The folder with theme.txt closest to the root of the unpacked archive.
fn find_theme_dir(root: &Path) -> Option<PathBuf> {
    let mut level = vec![root.to_path_buf()];
    for _ in 0..6 {
        let mut next = Vec::new();
        for dir in &level {
            if is_real_file(&dir.join("theme.txt")) {
                return Some(dir.clone());
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| is_real_dir(p)).collect();
                dirs.sort();
                next.extend(dirs);
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    None
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755))?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&source)?;
        if meta.is_dir() {
            copy_tree(&source, &target)?;
        } else if meta.is_file() {
            std::fs::copy(&source, &target)?;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))?;
        }
    }
    Ok(())
}

fn install_theme(archive: &str) -> bool {
    let archive_path = Path::new(archive);
    let Ok(meta) = std::fs::metadata(archive_path) else {
        eprintln!("soplos-grub-editor: cannot read the archive");
        return false;
    };
    if !meta.is_file() || meta.len() > MAX_ARCHIVE_BYTES {
        eprintln!("soplos-grub-editor: the archive is not a file or is too large");
        return false;
    }
    let Some(extension) = archive_extension(archive) else { return false };
    let Ok(work) = PrivateDir::new("theme") else {
        eprintln!("soplos-grub-editor: could not create a private folder");
        return false;
    };
    let unpacked = work.0.join("unpacked");
    if std::fs::create_dir(&unpacked).is_err() {
        return false;
    }
    let unpacked_text = unpacked.to_string_lossy().into_owned();
    let Some(timeout) = tool(&["timeout"]) else { return false };
    let extracted = if extension == ".zip" {
        match tool(&["unzip"]) {
            Some(unzip) => run(&timeout, &["120", &unzip.to_string_lossy(), "-q", "-o", archive, "-d", &unpacked_text]),
            None => false,
        }
    } else {
        match tool(&["tar"]) {
            Some(tar) => run(&timeout, &["120", &tar.to_string_lossy(), "-xf", archive, "-C", &unpacked_text, "--no-same-owner", "--no-same-permissions"]),
            None => false,
        }
    };
    if !extracted {
        eprintln!("soplos-grub-editor: the archive could not be unpacked");
        return false;
    }
    if let Err(e) = check_tree(&unpacked) {
        eprintln!("soplos-grub-editor: {e}");
        return false;
    }
    let Some(theme_dir) = find_theme_dir(&unpacked) else {
        eprintln!("soplos-grub-editor: no theme.txt found in the archive");
        return false;
    };
    let raw_name = if theme_dir == unpacked { archive_stem(archive) } else { theme_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
    let name = sanitize_name(&raw_name);
    if name.is_empty() {
        eprintln!("soplos-grub-editor: the theme has no usable name");
        return false;
    }
    if std::fs::create_dir_all(THEMES_DIR).is_err() {
        return false;
    }
    let destination = Path::new(THEMES_DIR).join(&name);
    if std::fs::symlink_metadata(&destination).is_ok() {
        if !is_real_dir(&destination) || std::fs::remove_dir_all(&destination).is_err() {
            eprintln!("soplos-grub-editor: could not replace the installed theme {name}");
            return false;
        }
    }
    if let Err(e) = copy_tree(&theme_dir, &destination) {
        let _ = std::fs::remove_dir_all(&destination);
        eprintln!("soplos-grub-editor: could not copy the theme: {e}");
        return false;
    }
    println!("@@theme {name}");
    true
}

fn remove_theme(name: &str) -> bool {
    let path = Path::new(THEMES_DIR).join(name);
    if !is_real_dir(&path) {
        eprintln!("soplos-grub-editor: {name} is not an installed theme");
        return false;
    }
    std::fs::remove_dir_all(&path).is_ok()
}

fn convert_font(source: &str, size: u32) -> bool {
    let Ok(real) = std::fs::canonicalize(source) else { return false };
    let Ok(meta) = std::fs::metadata(&real) else { return false };
    if !meta.is_file() || meta.len() > MAX_FONT_BYTES {
        eprintln!("soplos-grub-editor: the font is not a file or is too large");
        return false;
    }
    let stem = real.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = format!("{}_{size}.pf2", sanitize_name(&stem));
    if std::fs::create_dir_all(FONTS_DIR).is_err() {
        return false;
    }
    let Some(mkfont) = tool(&["grub-mkfont", "grub2-mkfont"]) else {
        eprintln!("soplos-grub-editor: grub-mkfont was not found");
        return false;
    };
    let output = Path::new(FONTS_DIR).join(&name);
    let ok = run(&mkfont, &["-s", &size.to_string(), "-o", &output.to_string_lossy(), &real.to_string_lossy()]);
    if ok {
        println!("@@font {name}");
    }
    ok
}

fn remove_font(name: &str) -> bool {
    let path = Path::new(FONTS_DIR).join(name);
    if !is_real_file(&path) {
        eprintln!("soplos-grub-editor: {name} is not an installed font");
        return false;
    }
    std::fs::remove_file(&path).is_ok()
}

/// grub-script-check on the entries part of 40_custom (what follows the
/// `exec tail` line), so a block GRUB cannot parse never reaches the file.
fn custom_entries_check(script: &str) -> bool {
    let entries: String = script.lines().skip(2).collect::<Vec<_>>().join("\n");
    let Some(check) = tool(&["grub-script-check", "grub2-script-check"]) else {
        eprintln!("soplos-grub-editor: grub-script-check was not found");
        return false;
    };
    let Ok(work) = PrivateDir::new("check") else { return false };
    let file = work.0.join("entries.cfg");
    if std::fs::write(&file, format!("{entries}\n")).is_err() {
        return false;
    }
    run(&check, &[&file.to_string_lossy()])
}

fn append_custom_entry(text: &str) -> bool {
    let path = Path::new(GRUB_D).join("40_custom");
    let mut content = std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CUSTOM_SCRIPT.to_string());
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(text.trim());
    content.push('\n');
    if !custom_entries_check(&content) {
        eprintln!("soplos-grub-editor: GRUB does not accept that entry, nothing was written");
        return false;
    }
    match write_atomic(&path, &content, 0o755) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("soplos-grub-editor: {e}");
            false
        }
    }
}

fn change_script(script: &str, change: impl Fn(&str) -> Option<String>) -> bool {
    let path = Path::new(GRUB_D).join(script);
    if !is_real_file(&path) {
        eprintln!("soplos-grub-editor: {script} is not a script of /etc/grub.d");
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&path) else { return false };
    let Some(updated) = change(&content) else {
        eprintln!("soplos-grub-editor: the entry is not in {script}");
        return false;
    };
    match write_atomic(&path, &updated, 0o755) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("soplos-grub-editor: {e}");
            false
        }
    }
}

fn delete_memtest(binary: &str) -> bool {
    let path = Path::new("/boot").join(binary);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => std::fs::remove_file(&path).is_ok(),
        Ok(_) => false,
        // Already gone.
        Err(_) => true,
    }
}

fn read_grub_cfg() -> bool {
    let Ok(meta) = std::fs::metadata(GRUB_CFG_PATH) else { return false };
    if meta.len() > MAX_CFG_BYTES {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(GRUB_CFG_PATH) else { return false };
    let mut out = std::io::stdout().lock();
    for line in text.lines() {
        let _ = writeln!(out, "@@cfg {line}");
    }
    true
}

/// Root side: the live system is checked here, after the request was
/// accepted, before anything runs.
fn validate_system(op: &Op) -> Result<(), String> {
    match op {
        Op::SetEntryEnabled { script, .. } | Op::DeleteEntry { script, .. } => {
            if is_real_file(&Path::new(GRUB_D).join(script)) {
                Ok(())
            } else {
                Err(format!("{script} is not in /etc/grub.d"))
            }
        }
        Op::InstallTheme { archive } => match std::fs::metadata(archive) {
            Ok(m) if m.is_file() => Ok(()),
            _ => Err("The archive is not there".into()),
        },
        Op::ConvertFont { source, .. } => match std::fs::metadata(source) {
            Ok(m) if m.is_file() => Ok(()),
            _ => Err("The font is not there".into()),
        },
        _ => Ok(()),
    }
}

fn execute(op: &Op) -> bool {
    match op {
        Op::SaveConfig { settings } => save_config(settings),
        Op::UpdateGrub => update_grub(),
        Op::SetThemeScripts { active } => set_theme_scripts(*active),
        Op::WriteColors { normal, highlight } => write_colors(normal, highlight),
        Op::InstallTheme { archive } => install_theme(archive),
        Op::RemoveTheme { name } => remove_theme(name),
        Op::ConvertFont { source, size } => convert_font(source, *size),
        Op::RemoveFont { name } => remove_font(name),
        Op::AppendCustomEntry { text } => append_custom_entry(text),
        Op::SetEntryEnabled { script, title, enabled } => change_script(script, |content| grub_config::toggle_block(content, title, *enabled)),
        Op::DeleteEntry { script, title } => change_script(script, |content| grub_config::remove_block(content, title)),
        Op::DeleteMemtest { binary } => delete_memtest(binary),
        Op::ReadGrubCfg => read_grub_cfg(),
    }
}

/// Root side, called from main() before Tauri starts when pkexec re-invokes
/// this binary as `--run-ops <file>`. Returns the process exit code: 0 only
/// when every operation succeeded.
pub fn run_ops_file(path: &str) -> i32 {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) => {
            eprintln!("soplos-grub-editor: cannot read the operations file: {e}");
            return 1;
        }
    };
    // Only a regular file written by the user who authenticated is honoured.
    if !meta.file_type().is_file() || meta.len() > MAX_OPS_FILE_BYTES {
        eprintln!("soplos-grub-editor: refusing the operations file");
        return 1;
    }
    if let Ok(uid) = std::env::var("PKEXEC_UID") {
        if meta.uid().to_string() != uid {
            eprintln!("soplos-grub-editor: the operations file does not belong to the requesting user");
            return 1;
        }
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("soplos-grub-editor: cannot read the operations file: {e}");
            return 1;
        }
    };
    let _ = std::fs::remove_file(path);

    let ops: Vec<Op> = match serde_json::from_str(&text) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("soplos-grub-editor: invalid operations: {e}");
            return 1;
        }
    };

    let mut results: Vec<bool> = Vec::with_capacity(ops.len());
    for (index, op) in ops.iter().enumerate() {
        let ok = match validate(op).and_then(|()| validate_system(op)) {
            Ok(()) => execute(op),
            Err(e) => {
                eprintln!("soplos-grub-editor: refused operation {index}: {e}");
                false
            }
        };
        results.push(ok);
        // A failed step stops the batch: the later ones were meant to follow it.
        if !ok {
            break;
        }
    }
    for (index, ok) in results.iter().enumerate() {
        println!("@@op {index} {}", if *ok { "ok" } else { "fail" });
    }
    let _ = std::io::stdout().flush();
    if results.len() == ops.len() && results.iter().all(|ok| *ok) {
        0
    } else {
        1
    }
}

// ---- Tauri side: the user's window asks for a batch of operations.

use std::sync::atomic::{AtomicU64, Ordering};

use super::jobs::{failed_job, start_job};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

// The request file lives in the user's runtime directory when there is one
// (private to the user), and is created with create_new and mode 0600 so
// nobody can pre-create or read it.
fn write_ops_file(json: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::SeqCst);
    let path = dir.join(format!("soplos-grub-editor-ops-{}-{id}-{nanos}.json", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
    file.write_all(json.as_bytes())?;
    Ok(path)
}

/// Starts one pkexec job that runs every given operation, in order.
#[tauri::command]
pub fn run_privileged_ops(ops: Vec<Op>) -> u64 {
    if ops.is_empty() {
        return failed_job("No operations given".to_string());
    }
    if let Some(error) = ops.iter().find_map(|op| validate(op).err()) {
        return failed_job(error);
    }
    let json = match serde_json::to_string(&ops) {
        Ok(json) => json,
        Err(e) => return failed_job(format!("Could not encode the operations: {e}")),
    };
    let file = match write_ops_file(&json) {
        Ok(file) => file,
        Err(e) => return failed_job(format!("Could not write the request: {e}")),
    };
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "soplos-grub-editor".to_string());
    start_job("pkexec", vec![exe, "--run-ops".to_string(), file.to_string_lossy().into_owned()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("soplos-grub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn setting(key: &str, value: &str) -> Setting {
        Setting { key: key.into(), value: value.into() }
    }

    #[test]
    fn settings_are_checked_key_by_key() {
        assert!(validate(&Op::SaveConfig { settings: vec![setting("GRUB_TIMEOUT", "5")] }).is_ok());
        assert!(validate(&Op::SaveConfig { settings: vec![] }).is_err());
        assert!(validate(&Op::SaveConfig { settings: vec![setting("GRUB_CMDLINE_LINUX_DEFAULT", "quiet $(reboot)")] }).is_err());
        assert!(validate(&Op::SaveConfig { settings: vec![setting("PATH", "/tmp")] }).is_err());
        assert!(validate(&Op::SaveConfig { settings: vec![setting("GRUB_TIMEOUT", "5"), setting("GRUB_TIMEOUT", "6")] }).is_err());
    }

    #[test]
    fn names_paths_and_titles_are_whitelisted() {
        assert!(validate(&Op::RemoveTheme { name: "Soplos-Dark".into() }).is_ok());
        assert!(validate(&Op::RemoveTheme { name: "../etc".into() }).is_err());
        assert!(validate(&Op::RemoveTheme { name: "a/b".into() }).is_err());
        assert!(validate(&Op::RemoveTheme { name: ".hidden".into() }).is_err());
        assert!(validate(&Op::RemoveTheme { name: "-rf".into() }).is_err());
        assert!(validate(&Op::RemoveFont { name: "dejavu_16.pf2".into() }).is_ok());
        assert!(validate(&Op::RemoveFont { name: "dejavu_16.ttf".into() }).is_err());
        assert!(validate(&Op::InstallTheme { archive: "/home/u/theme.tar.gz".into() }).is_ok());
        assert!(validate(&Op::InstallTheme { archive: "/home/u/../../etc/passwd.zip".into() }).is_err());
        assert!(validate(&Op::InstallTheme { archive: "theme.zip".into() }).is_err());
        assert!(validate(&Op::InstallTheme { archive: "/home/u/notes.txt".into() }).is_err());
        assert!(validate(&Op::ConvertFont { source: "/usr/share/fonts/a.TTF".into(), size: 16 }).is_ok());
        assert!(validate(&Op::ConvertFont { source: "/usr/share/fonts/a.ttf".into(), size: 4 }).is_err());
        assert!(validate(&Op::ConvertFont { source: "/etc/shadow".into(), size: 16 }).is_err());
        assert!(validate(&Op::DeleteMemtest { binary: "mt86+x64".into() }).is_ok());
        assert!(validate(&Op::DeleteMemtest { binary: "mt86+../../etc".into() }).is_err());
        assert!(validate(&Op::DeleteMemtest { binary: "vmlinuz".into() }).is_err());
        assert!(validate(&Op::SetEntryEnabled { script: "20_memtest86+".into(), title: "Memory test".into(), enabled: false }).is_ok());
        assert!(validate(&Op::SetEntryEnabled { script: "10_linux".into(), title: "x".into(), enabled: false }).is_err());
        assert!(validate(&Op::SetEntryEnabled { script: "../40_custom".into(), title: "x".into(), enabled: false }).is_err());
        assert!(validate(&Op::DeleteEntry { script: "40_custom".into(), title: "Mine".into() }).is_ok());
        assert!(validate(&Op::DeleteEntry { script: "20_memtest86+".into(), title: "Mine".into() }).is_err());
        assert!(validate(&Op::DeleteEntry { script: "40_custom".into(), title: "a\nb".into() }).is_err());
        assert!(validate(&Op::AppendCustomEntry { text: "menuentry 'x' {\n}\n".into() }).is_ok());
        assert!(validate(&Op::AppendCustomEntry { text: "rm -rf /".into() }).is_err());
        assert!(validate(&Op::AppendCustomEntry { text: "".into() }).is_err());
    }

    #[test]
    fn colors_come_in_pairs() {
        assert!(validate(&Op::WriteColors { normal: "white/black".into(), highlight: "black/white".into() }).is_ok());
        assert!(validate(&Op::WriteColors { normal: "".into(), highlight: "".into() }).is_ok());
        assert!(validate(&Op::WriteColors { normal: "white/black".into(), highlight: "".into() }).is_err());
        assert!(validate(&Op::WriteColors { normal: "white/pink".into(), highlight: "black/white".into() }).is_err());
        assert!(validate(&Op::WriteColors { normal: "white/black\nset x=1".into(), highlight: "black/white".into() }).is_err());
    }

    #[test]
    fn operations_round_trip_through_json() {
        let json = r#"[{"op":"save-config","settings":[{"key":"GRUB_TIMEOUT","value":"5"}]},{"op":"update-grub"},{"op":"set-theme-scripts","active":false},{"op":"delete-memtest","binary":"mt86+x64"}]"#;
        let ops: Vec<Op> = serde_json::from_str(json).unwrap();
        assert_eq!(ops.len(), 4);
        assert_eq!(serde_json::to_string(&ops).unwrap(), json);
        assert!(serde_json::from_str::<Vec<Op>>(r#"[{"op":"run-script","script":"id"}]"#).is_err());
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_keeps_its_mode() {
        let dir = scratch("atomic");
        let file = dir.join("grub");
        std::fs::write(&file, "old\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&file, "new\n", 0o644).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o640);
        // A new file gets the default mode, and no temporary file stays.
        let fresh = dir.join("fresh");
        write_atomic(&fresh, "x\n", 0o755).unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_shell_is_caught_before_it_replaces_the_file() {
        assert!(shell_syntax_ok("GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n"));
        assert!(!shell_syntax_ok("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet\n"));
    }

    #[test]
    fn a_theme_with_a_link_inside_is_refused() {
        let dir = scratch("tree");
        std::fs::write(dir.join("theme.txt"), "x").unwrap();
        assert!(check_tree(&dir).is_ok());
        std::os::unix::fs::symlink("/etc/shadow", dir.join("background.png")).unwrap();
        assert!(check_tree(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_theme_folder_is_found_and_copied_without_links() {
        let dir = scratch("theme");
        std::fs::create_dir_all(dir.join("pack/soplos/icons")).unwrap();
        std::fs::write(dir.join("pack/soplos/theme.txt"), "t").unwrap();
        std::fs::write(dir.join("pack/soplos/icons/a.png"), "p").unwrap();
        let found = find_theme_dir(&dir).unwrap();
        assert!(found.ends_with("pack/soplos"));
        let target = dir.join("out/soplos");
        copy_tree(&found, &target).unwrap();
        assert!(target.join("theme.txt").is_file());
        assert!(target.join("icons/a.png").is_file());
        assert_eq!(std::fs::metadata(target.join("theme.txt")).unwrap().permissions().mode() & 0o777, 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_names_are_made_safe() {
        assert_eq!(sanitize_name("Soplos Dark (v2)"), "Soplos_Dark__v2_");
        assert_eq!(sanitize_name("../../etc"), "_.._etc");
        assert_eq!(sanitize_name(".hidden"), "hidden");
        assert_eq!(sanitize_name(""), "");
        assert_eq!(archive_stem("/home/u/Soplos.tar.gz"), "Soplos");
        assert_eq!(archive_stem("/home/u/Soplos.zip"), "Soplos");
    }

    #[test]
    fn an_entry_is_changed_through_its_script() {
        let script = "#!/bin/sh\nexec tail -n +3 $0\nmenuentry 'Mine' {\n}\n";
        assert!(grub_config::toggle_block(script, "Mine", false).unwrap().contains("#menuentry 'Mine' {"));
        assert!(custom_entries_check_text_only(script));
    }

    // grub-script-check may not exist where the tests run, so this only checks
    // what the check is given: the entries without the two header lines.
    fn custom_entries_check_text_only(script: &str) -> bool {
        script.lines().skip(2).collect::<Vec<_>>().join("\n").starts_with("menuentry")
    }
}
