// Pure text work on the GRUB files: reading and editing /etc/default/grub,
// the checks every value goes through, and the parser of grub.cfg and of the
// /etc/grub.d scripts. Nothing here touches the disk, so all of it can be
// tested on plain strings; privileged.rs does the writing.
//
// /etc/default/grub is sourced by a shell as root (grub-mkconfig), so a value
// that reached it unchecked would be a command run as root. Every value is
// therefore refused when it holds a quote, a backslash, a dollar sign, a
// backtick or a control character, and written inside double quotes whenever
// it holds anything but plain characters.

use std::collections::HashMap;
use std::path::{Component, Path};

/// The keys this program writes. Anything else is refused.
pub const MANAGED_KEYS: &[&str] = &[
    "GRUB_DEFAULT",
    "GRUB_SAVEDEFAULT",
    "GRUB_TIMEOUT",
    "GRUB_TIMEOUT_STYLE",
    "GRUB_GFXMODE",
    "GRUB_CMDLINE_LINUX_DEFAULT",
    "GRUB_DISABLE_RECOVERY",
    "GRUB_DISABLE_OS_PROBER",
    "GRUB_DISABLE_LINUX_UUID",
    "GRUB_DISABLE_SUBMENU",
    "GRUB_THEME",
    "GRUB_BACKGROUND",
    "GRUB_COLOR_NORMAL",
    "GRUB_COLOR_HIGHLIGHT",
    "GRUB_FONT",
];

/// The sixteen colors GRUB knows by name.
pub const GRUB_COLORS: &[&str] = &[
    "black",
    "blue",
    "green",
    "cyan",
    "red",
    "magenta",
    "brown",
    "light-gray",
    "dark-gray",
    "light-blue",
    "light-green",
    "light-cyan",
    "light-red",
    "light-magenta",
    "yellow",
    "white",
];

const MARKER_MODIFIED: &str = "# Modified by Soplos GRUB Editor";

/// The settings of /etc/default/grub as key and value, without quotes.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut data = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim_matches('"').trim_matches('\'');
            data.insert(key.trim().to_string(), value.to_string());
        }
    }
    data
}

fn valid_color_pair(value: &str) -> bool {
    match value.split_once('/') {
        Some((text, background)) => GRUB_COLORS.contains(&text) && GRUB_COLORS.contains(&background),
        None => false,
    }
}

/// A path that is written to the file: absolute, with no `..`.
fn valid_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Whether a key may be written and its value is acceptable. An empty value
/// is always acceptable: it clears the key.
pub fn validate_setting(key: &str, value: &str) -> Result<(), String> {
    if !MANAGED_KEYS.contains(&key) {
        return Err(format!("Not a setting of this program: {key}"));
    }
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > 1024 {
        return Err(format!("The value of {key} is too long"));
    }
    if value.chars().any(|c| c.is_control() || matches!(c, '"' | '\\' | '$' | '`')) {
        return Err(format!("The value of {key} holds a character that is not allowed"));
    }
    let ok = match key {
        "GRUB_TIMEOUT" => {
            let digits = value.strip_prefix('-').unwrap_or(value);
            !digits.is_empty() && digits.len() <= 5 && digits.chars().all(|c| c.is_ascii_digit())
        }
        "GRUB_TIMEOUT_STYLE" => matches!(value, "menu" | "countdown" | "hidden"),
        "GRUB_SAVEDEFAULT" | "GRUB_DISABLE_RECOVERY" | "GRUB_DISABLE_OS_PROBER" | "GRUB_DISABLE_LINUX_UUID" | "GRUB_DISABLE_SUBMENU" => {
            matches!(value, "true" | "false" | "y" | "n" | "yes" | "no")
        }
        // A list of modes, each "auto", "keep" or WIDTHxHEIGHT[xDEPTH].
        "GRUB_GFXMODE" => value.split([',', ';']).all(|mode| matches!(mode, "auto" | "keep") || (mode.starts_with(|c: char| c.is_ascii_digit()) && mode.contains('x') && mode.chars().all(|c| c.is_ascii_digit() || c == 'x'))),
        "GRUB_THEME" | "GRUB_BACKGROUND" | "GRUB_FONT" => valid_path(value),
        "GRUB_COLOR_NORMAL" | "GRUB_COLOR_HIGHLIGHT" => valid_color_pair(value),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("The value of {key} is not valid"))
    }
}

/// The value as it is written after the key: bare when it only holds plain
/// characters, in double quotes otherwise. `validate_setting` has already
/// refused the characters that a double quote would not make safe.
pub fn render_value(value: &str) -> String {
    let plain = value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '=' | '+' | ',' | '@' | '%' | '-'));
    if plain {
        value.to_string()
    } else {
        format!("\"{value}\"")
    }
}

fn lookup<'a>(changes: &'a [(String, String)], key: &str) -> Option<&'a String> {
    changes.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The text of /etc/default/grub after the changes. Existing keys change in
/// place, a commented key that is set is uncommented, an empty value removes
/// the key, and keys the file never had are added at the end under a note.
/// Comments and the rest of the file are kept as they were.
pub fn apply_changes(original: &str, changes: &[(String, String)]) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut processed: Vec<String> = Vec::new();

    for line in original.split_inclusive('\n') {
        let stripped = line.trim();

        // Notes this program left on an earlier save.
        if stripped.contains(MARKER_MODIFIED) || stripped.contains("# Cleared by Soplos") || stripped.contains("# Removed by Soplos") {
            continue;
        }

        if stripped.starts_with('#') && stripped.contains('=') {
            let uncommented = stripped.trim_start_matches('#').trim();
            let key = uncommented.split('=').next().unwrap_or("").trim();
            if let Some(value) = lookup(changes, key) {
                if !processed.iter().any(|p| p == key) {
                    processed.push(key.to_string());
                    if value.is_empty() {
                        result.push(line.to_string());
                    } else {
                        result.push(format!("{key}={}\n", render_value(value)));
                    }
                    continue;
                }
            }
            result.push(line.to_string());
            continue;
        }

        if stripped.is_empty() || stripped.starts_with('#') {
            result.push(line.to_string());
            continue;
        }

        if stripped.contains('=') {
            let key = stripped.split('=').next().unwrap_or("").trim();
            // A duplicate of a key already handled is dropped.
            if processed.iter().any(|p| p == key) {
                continue;
            }
            if let Some(value) = lookup(changes, key) {
                processed.push(key.to_string());
                if value.is_empty() {
                    continue;
                }
                result.push(format!("{key}={}\n", render_value(value)));
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }

    let mut new_keys: Vec<String> = Vec::new();
    for (key, value) in changes {
        if !processed.iter().any(|p| p == key) && !value.is_empty() && !new_keys.iter().any(|n| n.starts_with(&format!("{key}="))) {
            new_keys.push(format!("{key}={}\n", render_value(value)));
        }
    }

    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }
    if let Some(last) = result.last_mut() {
        if !last.ends_with('\n') {
            last.push('\n');
        }
    }

    if !new_keys.is_empty() {
        if !result.is_empty() {
            result.push("\n".to_string());
        }
        result.push(format!("{MARKER_MODIFIED}\n"));
        result.extend(new_keys);
    }
    result.concat()
}

// ---------------------------------------------------------------------------
// grub.cfg and the scripts of /etc/grub.d

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuEntry {
    /// The name GRUB_DEFAULT takes: the titles of the submenus and of the
    /// entry joined by ">".
    pub name: String,
    pub display_name: String,
    /// "system", "recovery", "memtest" or "firmware".
    pub kind: String,
    pub enabled: bool,
    /// The /etc/grub.d script that generated it, when the file says so.
    pub source_script: Option<String>,
    /// The kernel or binary the entry loads.
    pub path: String,
}

fn words(line: &str) -> Vec<(usize, usize, &str)> {
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    for (index, c) in line.char_indices() {
        let word_char = c.is_alphanumeric() || c == '_';
        match (word_char, start) {
            (true, None) => start = Some(index),
            (false, Some(s)) => {
                found.push((s, index, &line[s..index]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        found.push((s, line.len(), &line[s..]));
    }
    found
}

// The platform tests in `condition`, as the platform each one names ("efi" or
// "pc") and whether the test is negated (`!=`). It reads the forms GRUB's
// generators write: `"$grub_platform" = "pc"`, `"${grub_platform}" == "efi"`
// and `!=`. Tests against anything else are ignored.
fn platform_tests(condition: &str) -> Vec<(&'static str, bool)> {
    let mut tests = Vec::new();
    let mut rest = condition;
    while let Some(position) = rest.find("grub_platform") {
        let after_name = &rest[position + "grub_platform".len()..];
        let tail = after_name.trim_start_matches(|c: char| matches!(c, '}' | '"' | '\'') || c.is_whitespace());
        let (negated, tail) = match tail.strip_prefix("!=") {
            Some(t) => (true, t),
            None => match tail.strip_prefix("==").or_else(|| tail.strip_prefix('=')) {
                Some(t) => (false, t),
                None => {
                    rest = after_name;
                    continue;
                }
            },
        };
        let value = tail.trim_start().trim_start_matches(['"', '\'']);
        let value = value.strip_prefix('x').unwrap_or(value);
        if value.starts_with("efi") {
            tests.push(("efi", negated));
        } else if value.starts_with("pc") {
            tests.push(("pc", negated));
        }
        rest = after_name;
    }
    tests
}

// Only the real pc/efi duplication is collapsed; different CPU architectures
// stay visible so entries such as the ia32 memtest can be managed.
fn condition_includes(condition: &str, platform: &str) -> bool {
    platform_tests(condition).into_iter().all(|(target, negated)| (platform == target) != negated)
}

// The title between the first quote after `keyword` and the next quote.
fn quoted_title(line: &str, keyword: &str) -> Option<String> {
    let mut from = 0;
    while let Some(position) = line[from..].find(keyword) {
        let at = from + position;
        let after = &line[at + keyword.len()..];
        let trimmed = after.trim_start();
        if trimmed.len() < after.len() {
            if let Some(quote) = trimmed.chars().next().filter(|c| *c == '\'' || *c == '"') {
                let body = &trimmed[quote.len_utf8()..];
                if let Some(end) = body.find(['\'', '"']) {
                    if end > 0 {
                        return Some(body[..end].to_string());
                    }
                }
            }
        }
        from = at + keyword.len();
    }
    None
}

fn boot_command_path(line: &str) -> Option<String> {
    for command in ["linux16", "linuxefi", "linux", "chainloader", "multiboot2", "multiboot"] {
        if let Some(rest) = line.strip_prefix(command) {
            if rest.starts_with(char::is_whitespace) {
                return rest.split_whitespace().next().map(str::to_string);
            }
        }
    }
    None
}

fn classify(title: &str) -> &'static str {
    let lower = title.to_lowercase();
    if lower.contains("recovery") {
        "recovery"
    } else if lower.contains("memtest") || lower.contains("memory test") || lower.contains("mt86") {
        "memtest"
    } else if lower.contains("uefi") || lower.contains("firmware") {
        "firmware"
    } else {
        "system"
    }
}

/// The entries of grub.cfg, submenus included. `platform` is "efi" or "pc":
/// only the branch of each platform test that matches it is kept.
pub fn parse_menu_entries(content: &str, platform: &str) -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut brace_depth: i64 = 0;
    let mut current: Option<usize> = None;
    let mut capture_depth: i64 = 0;
    let mut if_stack: Vec<bool> = Vec::new();
    let mut current_script: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();

        // Which /etc/grub.d script generated the entries that follow.
        if let Some(rest) = line.strip_prefix("###") {
            let rest = rest.trim();
            if let Some(after) = rest.strip_prefix("BEGIN") {
                if let Some(path) = after.trim().trim_end_matches('#').trim().split_whitespace().next() {
                    current_script = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned());
                }
                continue;
            }
            if rest.starts_with("END") {
                current_script = None;
                continue;
            }
        }

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // if/else/fi, left to right: several can share one line.
        let controls: Vec<(usize, usize, &str)> = words(line).into_iter().filter(|(_, _, w)| matches!(*w, "if" | "fi" | "else")).collect();
        if !controls.is_empty() {
            for (_, end, word) in controls {
                match word {
                    "if" => {
                        let rest = &line[end..];
                        let condition = match rest.find("then") {
                            Some(position) => &rest[..position],
                            None => rest,
                        };
                        if_stack.push(condition_includes(condition, platform));
                    }
                    "fi" => {
                        if_stack.pop();
                    }
                    _ => {
                        if let Some(top) = if_stack.last_mut() {
                            *top = !*top;
                        }
                    }
                }
            }
            continue;
        }

        if !if_stack.iter().all(|included| *included) {
            continue;
        }

        if line.contains('{') && quoted_title(line, "submenu").is_some() {
            stack.push(quoted_title(line, "submenu").unwrap_or_default());
            brace_depth += 1;
            continue;
        }

        if line.contains('{') {
            if let Some(title) = quoted_title(line, "menuentry") {
                let mut parts = stack.clone();
                parts.push(title.clone());
                entries.push(MenuEntry {
                    name: parts.join(">"),
                    display_name: title.clone(),
                    kind: classify(&title).to_string(),
                    enabled: true,
                    source_script: current_script.clone(),
                    path: String::new(),
                });
                brace_depth += 1;
                current = Some(entries.len() - 1);
                capture_depth = brace_depth;
                continue;
            }
        }

        if let Some(index) = current {
            if entries[index].path.is_empty() {
                if let Some(path) = boot_command_path(line) {
                    entries[index].path = path;
                }
            }
        }

        brace_depth += line.matches('{').count() as i64;
        for _ in 0..line.matches('}').count() {
            if !stack.is_empty() && brace_depth == stack.len() as i64 {
                stack.pop();
            }
            if brace_depth > 0 {
                brace_depth -= 1;
            }
        }
        if brace_depth < capture_depth {
            current = None;
        }
    }
    entries
}

/// Every menuentry title of a /etc/grub.d script, commented out or not, with
/// whether it is enabled. A title is enabled when any of its lines is not
/// commented out.
pub fn script_titles(content: &str) -> Vec<(String, bool)> {
    let mut found: Vec<(String, bool)> = Vec::new();
    for raw in content.lines() {
        let core = raw.trim_start_matches('#');
        if !core.contains('{') {
            continue;
        }
        if let Some(title) = quoted_title(core, "menuentry") {
            let enabled_here = !raw.trim_start().starts_with('#');
            match found.iter_mut().find(|(t, _)| *t == title) {
                Some((_, enabled)) => *enabled = *enabled || enabled_here,
                None => found.push((title, enabled_here)),
            }
        }
    }
    found
}

fn title_opens(core: &str, title: &str) -> bool {
    core.contains('{') && quoted_title(core, "menuentry").as_deref() == Some(title)
}

/// The script with the block of `title` commented out (`enabled` false) or
/// uncommented (`enabled` true). None when the entry is not in the script.
pub fn toggle_block(content: &str, title: &str, enabled: bool) -> Option<String> {
    let mut out = String::new();
    let mut depth: i64 = 0;
    let mut inside = false;
    let mut found = false;
    for line in content.split_inclusive('\n') {
        let core = line.trim_start_matches('#');
        if !inside {
            if title_opens(core, title) {
                inside = true;
                found = true;
                depth = core.matches('{').count() as i64 - core.matches('}').count() as i64;
                out.push_str(&if enabled { core.to_string() } else { format!("#{core}") });
                if depth <= 0 {
                    inside = false;
                }
                continue;
            }
            out.push_str(line);
            continue;
        }
        depth += core.matches('{').count() as i64 - core.matches('}').count() as i64;
        out.push_str(&if enabled { core.to_string() } else { format!("#{core}") });
        if depth <= 0 {
            inside = false;
        }
    }
    found.then_some(out)
}

/// The script without the block of `title`, commented out or not. None when
/// the entry is not in the script.
pub fn remove_block(content: &str, title: &str) -> Option<String> {
    let mut out = String::new();
    let mut depth: i64 = 0;
    let mut inside = false;
    let mut found = false;
    for line in content.split_inclusive('\n') {
        let core = line.trim_start_matches('#');
        if !inside {
            if title_opens(core, title) {
                inside = true;
                found = true;
                depth = core.matches('{').count() as i64 - core.matches('}').count() as i64;
                if depth <= 0 {
                    inside = false;
                }
                continue;
            }
            out.push_str(line);
            continue;
        }
        depth += core.matches('{').count() as i64 - core.matches('}').count() as i64;
        if depth <= 0 {
            inside = false;
        }
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const SAMPLE: &str = "# If you change this file, run 'update-grub' afterwards\n\nGRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_DISTRIBUTOR=`lsb_release -i -s 2> /dev/null || echo Debian`\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\nGRUB_CMDLINE_LINUX=\"\"\n#GRUB_TIMEOUT_STYLE=hidden\n#GRUB_TERMINAL=console\n";

    #[test]
    fn existing_keys_change_in_place_and_comments_stay() {
        let out = apply_changes(SAMPLE, &change(&[("GRUB_TIMEOUT", "10"), ("GRUB_CMDLINE_LINUX_DEFAULT", "quiet")]));
        assert!(out.contains("GRUB_TIMEOUT=10\n"));
        assert!(out.contains("GRUB_CMDLINE_LINUX_DEFAULT=quiet\n"));
        assert!(out.contains("# If you change this file"));
        assert!(out.contains("GRUB_DISTRIBUTOR=`lsb_release"));
        assert!(out.contains("#GRUB_TERMINAL=console\n"));
    }

    #[test]
    fn a_commented_key_is_uncommented_when_it_is_set() {
        let out = apply_changes(SAMPLE, &change(&[("GRUB_TIMEOUT_STYLE", "menu")]));
        assert!(out.contains("\nGRUB_TIMEOUT_STYLE=menu\n"));
        assert!(!out.contains("#GRUB_TIMEOUT_STYLE"));
    }

    #[test]
    fn an_empty_value_removes_the_key_and_keeps_a_commented_one() {
        let out = apply_changes(SAMPLE, &change(&[("GRUB_TIMEOUT", ""), ("GRUB_TIMEOUT_STYLE", "")]));
        assert!(!out.contains("GRUB_TIMEOUT=5"));
        assert!(out.contains("#GRUB_TIMEOUT_STYLE=hidden"));
    }

    #[test]
    fn new_keys_go_at_the_end_under_a_note_and_are_not_repeated_on_the_next_save() {
        let first = apply_changes(SAMPLE, &change(&[("GRUB_GFXMODE", "1920x1080"), ("GRUB_DISABLE_SUBMENU", "y")]));
        assert!(first.ends_with("# Modified by Soplos GRUB Editor\nGRUB_GFXMODE=1920x1080\nGRUB_DISABLE_SUBMENU=y\n"));
        let second = apply_changes(&first, &change(&[("GRUB_GFXMODE", "1280x720")]));
        assert_eq!(second.matches("# Modified by Soplos GRUB Editor").count(), 0);
        assert!(second.contains("GRUB_GFXMODE=1280x720\n"));
        assert!(second.contains("GRUB_DISABLE_SUBMENU=y\n"));
    }

    #[test]
    fn a_duplicate_key_is_dropped() {
        let out = apply_changes("GRUB_TIMEOUT=5\nGRUB_TIMEOUT=7\n", &change(&[("GRUB_TIMEOUT", "3")]));
        assert_eq!(out, "GRUB_TIMEOUT=3\n");
    }

    #[test]
    fn values_with_spaces_or_symbols_are_quoted() {
        assert_eq!(render_value("quiet"), "quiet");
        assert_eq!(render_value("quiet splash"), "\"quiet splash\"");
        assert_eq!(render_value("Sub menu>Entry"), "\"Sub menu>Entry\"");
        assert_eq!(render_value("A>B"), "\"A>B\"");
        assert_eq!(render_value("/boot/grub/themes/x/theme.txt"), "/boot/grub/themes/x/theme.txt");
    }

    #[test]
    fn dangerous_values_and_unknown_keys_are_refused() {
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "quiet splash").is_ok());
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "quiet $(id)").is_err());
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "quiet `id`").is_err());
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "a\"b").is_err());
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "a\nb=c").is_err());
        assert!(validate_setting("GRUB_CMDLINE_LINUX_DEFAULT", "a\\b").is_err());
        assert!(validate_setting("PATH", "/bin").is_err());
        assert!(validate_setting("GRUB_DISTRIBUTOR", "x").is_err());
        assert!(validate_setting("GRUB_TIMEOUT", "5").is_ok());
        assert!(validate_setting("GRUB_TIMEOUT", "-1").is_ok());
        assert!(validate_setting("GRUB_TIMEOUT", "five").is_err());
        assert!(validate_setting("GRUB_TIMEOUT_STYLE", "hidden").is_ok());
        assert!(validate_setting("GRUB_TIMEOUT_STYLE", "x").is_err());
        assert!(validate_setting("GRUB_GFXMODE", "1920x1080").is_ok());
        assert!(validate_setting("GRUB_GFXMODE", "auto").is_ok());
        assert!(validate_setting("GRUB_GFXMODE", "1920x1080;rm").is_err());
        assert!(validate_setting("GRUB_GFXMODE", "1920x1080,auto").is_ok());
        assert!(validate_setting("GRUB_GFXMODE", "1920x1080x32").is_ok());
        assert!(validate_setting("GRUB_GFXMODE", "1920x1080,").is_err());
        assert!(validate_setting("GRUB_GFXMODE", "x").is_err());
        assert!(validate_setting("GRUB_THEME", "/boot/grub/themes/a/theme.txt").is_ok());
        assert!(validate_setting("GRUB_THEME", "/boot/grub/themes/../../etc").is_err());
        assert!(validate_setting("GRUB_THEME", "relative/theme.txt").is_err());
        assert!(validate_setting("GRUB_COLOR_NORMAL", "white/black").is_ok());
        assert!(validate_setting("GRUB_COLOR_NORMAL", "white/pink").is_err());
        assert!(validate_setting("GRUB_DISABLE_SUBMENU", "y").is_ok());
        assert!(validate_setting("GRUB_DISABLE_SUBMENU", "maybe").is_err());
        assert!(validate_setting("GRUB_DEFAULT", "Advanced options for Debian GNU/Linux>Debian GNU/Linux, with Linux 6.1 (recovery mode)").is_ok());
        assert!(validate_setting("GRUB_BACKGROUND", "").is_ok());
    }

    #[test]
    fn the_settings_are_read_without_quotes() {
        let data = parse(SAMPLE);
        assert_eq!(data.get("GRUB_TIMEOUT").map(String::as_str), Some("5"));
        assert_eq!(data.get("GRUB_CMDLINE_LINUX_DEFAULT").map(String::as_str), Some("quiet splash"));
        assert!(!data.contains_key("GRUB_TIMEOUT_STYLE"));
    }

    const CFG: &str = "### BEGIN /etc/grub.d/10_linux ###\nmenuentry 'Debian GNU/Linux' --class debian $menuentry_id_option 'gnulinux-simple' {\n\tload_video\n\tlinux\t/boot/vmlinuz-6.1.0 root=/dev/sda2 ro quiet\n\tinitrd\t/boot/initrd.img-6.1.0\n}\nsubmenu 'Advanced options for Debian GNU/Linux' $menuentry_id_option 'gnulinux-advanced' {\n\tmenuentry 'Debian GNU/Linux, with Linux 6.1.0' --class debian {\n\t\tlinux\t/boot/vmlinuz-6.1.0 root=/dev/sda2 ro quiet\n\t}\n\tmenuentry 'Debian GNU/Linux, with Linux 6.1.0 (recovery mode)' --class debian {\n\t\tlinux\t/boot/vmlinuz-6.1.0 root=/dev/sda2 ro single\n\t}\n}\n### END /etc/grub.d/10_linux ###\n### BEGIN /etc/grub.d/20_memtest86+ ###\nif [ \"${grub_platform}\" == \"pc\" ]; then\nmenuentry \"Memory test (memtest86+x64.bin)\" {\n\tlinux16 /boot/memtest86+x64.bin\n}\nfi\nif [ \"${grub_platform}\" == \"efi\" ]; then\nmenuentry \"Memory test (memtest86+x64.efi)\" {\n\tchainloader /boot/memtest86+x64.efi\n}\nfi\n### END /etc/grub.d/20_memtest86+ ###\n";

    #[test]
    fn entries_keep_their_submenu_in_the_name_and_the_script_that_made_them() {
        let entries = parse_menu_entries(CFG, "efi");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], "Debian GNU/Linux");
        assert_eq!(names[1], "Advanced options for Debian GNU/Linux>Debian GNU/Linux, with Linux 6.1.0");
        assert_eq!(names[2], "Advanced options for Debian GNU/Linux>Debian GNU/Linux, with Linux 6.1.0 (recovery mode)");
        assert_eq!(entries[0].source_script.as_deref(), Some("10_linux"));
        assert_eq!(entries[0].path, "/boot/vmlinuz-6.1.0");
        assert_eq!(entries[2].kind, "recovery");
    }

    #[test]
    fn only_the_branch_of_the_real_platform_is_listed() {
        let efi = parse_menu_entries(CFG, "efi");
        assert!(efi.iter().any(|e| e.display_name.contains("memtest86+x64.efi")));
        assert!(!efi.iter().any(|e| e.display_name.contains("memtest86+x64.bin")));
        let pc = parse_menu_entries(CFG, "pc");
        assert!(pc.iter().any(|e| e.display_name.contains("memtest86+x64.bin")));
        assert!(!pc.iter().any(|e| e.display_name.contains("memtest86+x64.efi")));
        let memtest = pc.iter().find(|e| e.kind == "memtest").unwrap();
        assert_eq!(memtest.path, "/boot/memtest86+x64.bin");
    }

    const SCRIPT: &str = "#!/bin/sh\nexec tail -n +3 $0\nmenuentry 'First' {\n\tset root=(hd0,1)\n}\n#menuentry 'Second' {\n#\tset root=(hd0,2)\n#}\n";

    #[test]
    fn a_block_is_commented_out_and_back_by_its_title() {
        let off = toggle_block(SCRIPT, "First", false).unwrap();
        assert!(off.contains("#menuentry 'First' {\n#\tset root=(hd0,1)\n#}\n"));
        assert!(off.contains("#menuentry 'Second' {"));
        let on = toggle_block(&off, "First", true).unwrap();
        assert_eq!(on, SCRIPT);
        let second = toggle_block(SCRIPT, "Second", true).unwrap();
        assert!(second.contains("\nmenuentry 'Second' {\n\tset root=(hd0,2)\n}\n"));
        assert!(toggle_block(SCRIPT, "Missing", false).is_none());
    }

    #[test]
    fn the_titles_of_a_script_tell_which_are_disabled() {
        let titles = script_titles(SCRIPT);
        assert_eq!(titles, vec![("First".to_string(), true), ("Second".to_string(), false)]);
    }

    #[test]
    fn a_block_is_removed_even_when_it_is_commented_out() {
        let without = remove_block(SCRIPT, "Second").unwrap();
        assert_eq!(without, "#!/bin/sh\nexec tail -n +3 $0\nmenuentry 'First' {\n\tset root=(hd0,1)\n}\n");
        let without_first = remove_block(SCRIPT, "First").unwrap();
        assert!(!without_first.contains("First"));
        assert!(without_first.contains("Second"));
        assert!(remove_block(SCRIPT, "Third").is_none());
    }
}
