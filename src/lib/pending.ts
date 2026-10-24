import { get, writable } from "svelte/store";
import { askBackup, runOps, t, type Op } from "./grub";
import { askConfirm } from "./dialogs";

// The changes the user asks for on any page are not made one by one: they wait
// here, and the bar at the bottom applies all of them together with a single
// password and a single update-grub. Each change has a key, so asking for the
// same thing again replaces it instead of piling up.

export interface PendingItem {
  key: string;
  label: string;
  ops: Op[];
}

export const pending = writable<PendingItem[]>([]);

export const entryToggleKey = (script: string, title: string) => `entry:${script}:${title}`;
export const entryDeleteKey = (script: string, title: string) => `delete:${script}:${title}`;
export const memtestDeleteKey = (binary: string) => `memtest:${binary}`;

/** Adds the change, or replaces the one with the same key. */
export function queue(item: PendingItem): void {
  pending.update((list) => {
    const index = list.findIndex((other) => other.key === item.key);
    if (index < 0) return [...list, item];
    const copy = [...list];
    copy[index] = item;
    return copy;
  });
}

/** Drops a change the page itself took back (a toggle switched back, for example). */
export function unqueue(key: string): void {
  pending.update((list) => (list.some((item) => item.key === key) ? list.filter((item) => item.key !== key) : list));
}

export const isPending = (key: string) => get(pending).some((item) => item.key === key);

// The pages hold their own copy of what is on screen. When pending changes
// leave without being made (taken off the list, discarded) or after they were
// made, the pages that own those keys read the disk again.
type Listener = (keys: string[] | "all") => void;
const listeners = new Set<Listener>();

export function onReleased(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

const notify = (keys: string[] | "all") => listeners.forEach((listener) => listener(keys));

/** Takes one change off the list from the bar; its page goes back to the disk state. */
export function release(key: string): void {
  unqueue(key);
  notify([key]);
}

export async function discardAll(): Promise<void> {
  if (get(pending).length === 0) return;
  if (!(await askConfirm(t("pending.discardTitle"), t("pending.discardBody"), t("pending.discard"), true))) return;
  pending.set([]);
  notify("all");
}

export type ApplyPhase = "idle" | "running" | "done" | "failed";

export const applyState = writable<{ phase: ApplyPhase; output: string }>({ phase: "idle", output: "" });

/** Makes every pending change in one go, then runs update-grub once. */
export async function applyPending(): Promise<void> {
  const items = get(pending);
  if (items.length === 0 || get(applyState).phase !== "idle") return;
  if (!(await askBackup())) return;
  const ops: Op[] = [...items.flatMap((item) => item.ops), { op: "update-grub" }];
  applyState.set({ phase: "running", output: "" });
  const result = await runOps(ops, (output) => applyState.update((state) => ({ ...state, output })));
  applyState.set({ phase: result.ok ? "done" : "failed", output: result.output });
}

/** Closes the output panel. After a success the list is empty and the pages read the disk again. */
export function closeApply(): void {
  const { phase } = get(applyState);
  if (phase === "running") return;
  applyState.set({ phase: "idle", output: "" });
  if (phase === "done") {
    pending.set([]);
    notify("all");
  }
}
