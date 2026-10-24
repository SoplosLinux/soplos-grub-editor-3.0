# Changelog

All notable changes to Soplos GRUB Editor are documented in this file.

## [3.0.0-alpha] - 2026-10-24

Full rewrite of Soplos GRUB Editor, from Python 3 and GTK3 to Tauri 2 with a Rust backend and a Svelte frontend. See the Architecture section of the README.

### Added

- Rust backend (`src-tauri/`) replacing the Python 3 and GTK3 codebase. The window no longer runs as root: every change to the boot files is a list of named operations sent to a single Polkit-gated helper, which accepts only that closed list, checks every value again as root and runs each operation with a fixed command line.
- Values written to `/etc/default/grub` are checked before they are written. The file is read by a shell as root when GRUB builds its menu, so a value with a quote, a backslash, a dollar sign, a backtick or a control character is refused, and only the settings this program manages can be written. Values are quoted whenever they hold anything but plain characters.
- Safe writing: every file is written to a temporary file beside it and renamed over the old one, `/etc/default/grub` is checked to be valid shell first, and custom boot entries are checked with `grub-script-check` before they reach `/etc/grub.d/40_custom`.
- Theme archives are unpacked as root in a private folder and refused when they hold anything but plain files or are too large, then copied without links into `/boot/grub/themes`.
- Svelte 4 and Tailwind CSS frontend (`src/`) with three views: General, Boot Entries and Appearance, the same three as the Python version.
- General: default boot mode (fixed entry, last installed kernel or last booted), default entry, timeout, screen resolution, detection of other operating systems, recovery entries, UUIDs, submenus and kernel parameters. The entry is only written when it is chosen. A resolution or a list of modes that is not one of the listed ones stays selectable instead of being replaced by `auto`.
- Boot Entries: the entries of `grub.cfg`, submenus included, with switches for the memtest entries and the custom ones, deletion, and a dialog to add custom entries. The parser now reads the real form of the platform tests (`"${grub_platform}" == "pc"`), so the entries of the other platform are no longer listed twice.
- Appearance: install, apply, disable and remove themes; background image and colors chosen from the sixteen colors GRUB knows; conversion of TrueType and OpenType fonts to PF2, and applying and removing fonts. A preview shows the theme or the background.
- A copy of `/etc/default/grub` is offered before the first change of a session, saved wherever the user chooses. The program leaves no backup files of its own.
- Notes, as in Sys Cleaner and System Service: a panel beside each page and a short line under each setting. Off by default; the Notes button of the header and Ctrl+I turn them on.
- After a change the program offers to run `update-grub`, and shows what a failed command said.
- The window follows the light or dark theme of the desktop, with a button in the sidebar to choose.
- On a system without `/etc/default/grub` or `update-grub` the program says so and changes nothing.
- Interface in eight languages: English, Spanish, German, French, Italian, Portuguese, Romanian and Russian. It starts in the system language and falls back to English.
- Seven screenshots of the program, installed to `/usr/share/doc/soplos-grub-editor/screenshots/` and listed in the AppStream metainfo and the README.
- Unix manual page (`soplos-grub-editor.1`) installed to `/usr/share/man/man1/`, AppStream metainfo and desktop entry in the eight languages, Polkit policy, Debian copyright (declaring Lucide, ISC) and this `CHANGELOG.md` installed as `/usr/share/doc/soplos-grub-editor/changelog.gz`.
