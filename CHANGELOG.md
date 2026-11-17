# Changelog

All notable changes to Soplos GRUB Editor are documented in this file.

## [3.0.0] - 2026-11-17

First stable release.

## [3.0.0-5-beta] - 2026-11-15

### Fixed

- General: "Detect other operating systems" was shown on when `GRUB_DISABLE_OS_PROBER` was not in `/etc/default/grub`, but GRUB 2.06 and later leave os-prober off in that case, as on a fresh install. Saving with it on did not write the setting either, so os-prober stayed off until it was turned off and on again. A missing setting now reads as off, and turning it on writes `GRUB_DISABLE_OS_PROBER=false` the first time.

## [3.0.0-4-beta] - 2026-11-04

### Fixed

- The application metadata points to the launcher that the package installs, `soplos-grub-editor.desktop`, so the software centers link the entry of the menu with the program.
- The manual page of the program is the same text in the project and in the package.

## [3.0.0-3-beta] - 2026-10-29

### Changed

- The window no longer has the frame of the system: it is a card with rounded corners and its own minimize, maximize and close buttons at the top right. It can be moved from the top strip, the banner and the side bar, and double click on the strip maximizes it. The rounded corners need a compositing manager running, as the one of Xfce, Plasma or GNOME.

## [3.0.0-2-beta] - 2026-10-28

### Changed

- New application icon, in the sizes of the package, the window and the interface.
- New hero image, with the circuit style of the other Soplos 3.0 programs.
- New screenshots of every page, taken with the interface in English.

## [3.0.0-1-beta] - 2026-10-25

### Added

- On the Appearance page, the cards of the themes now show what the pending changes will do: a theme that will be used is marked "Will be used" with a stronger frame, the theme in use that will be turned off is marked "Will be turned off", and a theme that will be removed is marked "Will be removed", with a red frame and a faded picture. The marks go away when the change is taken off the list or discarded.

### Fixed

- Apply all did nothing when a change was prepared after a run that had finished, while the strip of that run was still on screen. A run that has finished no longer blocks the next one.
- The buttons of the dialog that offers a copy of `/etc/default/grub` were split over two rows. Cancel is now on the left and the two ways to go on are together on the right, in one row.

## [3.0.0-beta] - 2026-10-24

First beta, after the alpha was tested on real hardware.

### Changed

- Changes are no longer made one by one. Anything changed on any page waits in a bar at the bottom of the window, which lists the pending changes, lets each one be taken off and has Apply all and Discard. Apply all makes every change with a single password and runs `update-grub` once, instead of asking for the password and offering `update-grub` after each change. The offer to run `update-grub` after each change is gone.
- The Apply button of the General page is gone: any difference from what is in `/etc/default/grub` becomes a pending change, and is dropped again when the fields are back as they were.
- Switches, deletions and new entries on the Boot Entries page wait in the same list. The entries are shown as they will be once applied, and an entry that will be deleted is struck through.
- On the Appearance page, the buttons that applied a theme, the background and colors, or a font now add it to the changes. A theme and a custom look share one place on the list, since they exclude each other. Installing a theme and converting a font still happen at once, without `update-grub`.
- The three pages stay loaded when switching between them, so what was typed or chosen on one is still there when coming back.
- Closing the window with pending changes asks first.
- The hints of the Boot Entries and Appearance pages no longer say that changes are made as soon as they are asked for.

### Added

- A progress bar along the bottom edge of the window while the changes are applied, and a strip above it with the latest line of output and a log that can be unfolded, the same as in the Store. A failed run opens the log by itself.

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
