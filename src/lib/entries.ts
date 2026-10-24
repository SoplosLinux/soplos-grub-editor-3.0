import { get, writable } from "svelte/store";
import { api } from "./api";
import { runOps, tagged, type MenuEntry, type EntriesResult } from "./grub";

// The boot entries of grub.cfg, shared by the General and Boot Entries pages.
// grub.cfg is not readable by everyone on every system: when it is not, the
// page offers to read it as administrator.

export interface EntriesState {
  loaded: boolean;
  loading: boolean;
  entries: MenuEntry[];
  unreadable: boolean;
}

// grub.cfg as administrator is read once per session: the text is kept in the
// session storage, so reloading the window does not ask for the password
// again. A change that regenerates the menu reads it again.
const CFG_KEY = "soplos-grub-editor-cfg";

function cachedCfg(): string | null {
  try {
    return sessionStorage.getItem(CFG_KEY);
  } catch {
    return null;
  }
}

function cacheCfg(text: string) {
  try {
    sessionStorage.setItem(CFG_KEY, text);
  } catch {
    // Without storage the password is simply asked again after a reload.
  }
}

export const entriesState = writable<EntriesState>({ loaded: false, loading: false, entries: [], unreadable: false });

export async function loadEntries(cfg?: string): Promise<void> {
  entriesState.update((s) => ({ ...s, loading: true }));
  try {
    const result = await api.invoke<EntriesResult>("get_menu_entries", cfg === undefined ? {} : { cfg });
    entriesState.set({ loaded: true, loading: false, entries: result.entries, unreadable: result.unreadable });
  } catch (error) {
    console.error("Reading the boot entries failed", error);
    entriesState.set({ loaded: true, loading: false, entries: [], unreadable: false });
  }
}

/**
 * Reads the entries again; when grub.cfg needs administrator rights, reads it
 * that way. The authorization lasts a few minutes, so after the first password
 * this does not ask again.
 */
export async function reloadEntries(): Promise<void> {
  await loadEntries();
  if (get(entriesState).unreadable) await loadEntriesAsAdmin();
}

let firstRead: Promise<void> | null = null;

/**
 * The first read of the session, started by whichever page needs the entries
 * first. When grub.cfg needs administrator rights the password is asked right
 * away, once: the entries are the heart of the program, so they should not
 * wait behind a button. If the password is refused the pages offer the button.
 */
export function ensureEntries(): Promise<void> {
  if (!firstRead) {
    firstRead = (async () => {
      await loadEntries();
      if (!get(entriesState).unreadable) return;
      const cached = cachedCfg();
      if (cached) await loadEntries(cached);
      else await loadEntriesAsAdmin();
    })();
  }
  return firstRead;
}

/** Asks for the password, reads grub.cfg as administrator and lists its entries. */
export async function loadEntriesAsAdmin(): Promise<boolean> {
  entriesState.update((s) => ({ ...s, loading: true }));
  const result = await runOps([{ op: "read-grub-cfg" }]);
  if (!result.ok) {
    entriesState.update((s) => ({ ...s, loading: false }));
    return false;
  }
  const text = tagged(result.output, "cfg").join("\n");
  cacheCfg(text);
  await loadEntries(text);
  return true;
}
