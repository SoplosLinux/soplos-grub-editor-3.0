import { get } from "svelte/store";
import { _ } from "svelte-i18n";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { runJobDetailed } from "./job";
import { askBackupDialog } from "./dialogs";

// The window never writes a boot file itself: every change is a list of named
// operations that the privileged helper validates and runs (see
// src-tauri/src/commands/privileged.rs). This file holds their shape, the
// reading commands and the optional copy offered before the first change.
// The changes themselves are collected and applied together in pending.ts.

export interface Setting {
  key: string;
  value: string;
}

export type Op =
  | { op: "save-config"; settings: Setting[] }
  | { op: "update-grub" }
  | { op: "set-theme-scripts"; active: boolean }
  | { op: "write-colors"; normal: string; highlight: string }
  | { op: "install-theme"; archive: string }
  | { op: "remove-theme"; name: string }
  | { op: "convert-font"; source: string; size: number }
  | { op: "remove-font"; name: string }
  | { op: "append-custom-entry"; text: string }
  | { op: "set-entry-enabled"; script: string; title: string; enabled: boolean }
  | { op: "delete-entry"; script: string; title: string }
  | { op: "delete-memtest"; binary: string }
  | { op: "read-grub-cfg" };

export interface MenuEntry {
  name: string;
  displayName: string;
  kind: "system" | "recovery" | "memtest" | "firmware";
  enabled: boolean;
  sourceScript: string | null;
  path: string;
}

export interface EntriesResult {
  entries: MenuEntry[];
  unreadable: boolean;
}

export interface Presence {
  hasConfig: boolean;
  hasUpdateGrub: boolean;
}

export type Config = Record<string, string>;

export const t = (key: string, values?: Record<string, string | number>) => get(_)(key, { values });

export interface OpsResult {
  ok: boolean;
  output: string;
}

/** Runs the operations as administrator in one go (one password). */
export async function runOps(ops: Op[], onOutput?: (output: string) => void): Promise<OpsResult> {
  try {
    const status = await runJobDetailed("run_privileged_ops", { ops }, onOutput);
    return { ok: status.success, output: status.output };
  } catch (error) {
    return { ok: false, output: String(error) };
  }
}

/** The values of the `@@<tag> ` lines the helper printed. */
export function tagged(output: string, tag: string): string[] {
  const prefix = `@@${tag} `;
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith(prefix))
    .map((line) => line.slice(prefix.length));
}

/** What a failed command said, without the machine lines. */
export function visibleOutput(output: string): string {
  return output
    .split("\n")
    .filter((line) => line.trim() && !line.startsWith("@@"))
    .slice(-12)
    .join("\n");
}

const readConfig = () => api.invoke<Config>("read_grub_config");
export { readConfig };

// A copy of /etc/default/grub is offered before the first change of a
// session. It is saved where the user chooses, never beside the original, so
// nothing piles up in /etc/default. Once the user has saved one or chosen to
// go on without it, the rest of the session is not asked again: the copy
// holds the state before any change.
let backupDecided = false;

function stamp(): string {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}-${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
}

async function saveBackup(): Promise<{ ok: boolean; error?: string }> {
  const path = await save({ title: t("backup.saveTitle"), defaultPath: `grub-backup-${stamp()}` });
  if (!path) return { ok: false };
  try {
    await api.invoke("save_grub_backup", { path });
    return { ok: true };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

/** True when the change can go on. */
export async function askBackup(): Promise<boolean> {
  if (backupDecided) return true;
  const proceed = await askBackupDialog(t("backup.title"), t("backup.body"), async () => {
    const result = await saveBackup();
    return result.ok ? { ok: true } : result.error ? { ok: false, error: result.error } : { ok: false };
  });
  if (proceed) backupDecided = true;
  return proceed;
}

