<script lang="ts">
  import { onMount } from "svelte";
  import { _ } from "svelte-i18n";
  import { Lock, Plus, Trash2 } from "lucide-svelte";
  import Modal from "../components/Modal.svelte";
  import Hint from "../components/Hint.svelte";
  import { entriesState, ensureEntries, reloadEntries, loadEntriesAsAdmin } from "../lib/entries";
  import { applyChange, t, type MenuEntry } from "../lib/grub";
  import { askConfirm, showInfo } from "../lib/dialogs";

  // The boot entries of grub.cfg. Only what this program can really change is
  // changeable: the memtest entries and the ones written to 40_custom. Kernel
  // and other-system entries are regenerated from what is installed, and the
  // recovery and other-system ones already have their own switch in the
  // general settings, so they are not listed twice.

  type Category = "custom" | "memtest" | "recovery" | "os-prober" | "kernel" | "other";

  function memtestBinary(title: string): string | null {
    const lower = title.toLowerCase();
    if (lower.includes("mt86+x64") || lower.includes("x86_64")) return "mt86+x64";
    if (lower.includes("mt86+ia32") || lower.includes("ia32")) return "mt86+ia32";
    return null;
  }

  // Checked first: a title written by the user could contain "x86_64" and
  // would otherwise be taken for a memtest entry.
  function category(entry: MenuEntry): Category {
    if (entry.sourceScript === "40_custom") return "custom";
    if (memtestBinary(entry.displayName)) return "memtest";
    if (entry.displayName.toLowerCase().includes("recovery")) return "recovery";
    if (entry.sourceScript === "30_os-prober") return "os-prober";
    if (entry.sourceScript === "10_linux") return "kernel";
    return "other";
  }

  $: entries = $entriesState.entries.filter((e) => {
    const kind = category(e);
    return kind !== "recovery" && kind !== "os-prober";
  });

  let selected = -1;
  let readingAsAdmin = false;

  onMount(() => {
    void ensureEntries();
  });

  // The submenus an entry sits in, from its GRUB_DEFAULT-style name.
  const parentsOf = (entry: MenuEntry) => entry.name.split(">").slice(0, -1);

  const KIND_STYLES: Record<string, string> = {
    system: "bg-primary/15 text-primary",
    memtest: "bg-warning/15 text-warning",
    firmware: "bg-surface-2 text-text",
    recovery: "bg-danger/10 text-danger",
  };
  const kindClass = (kind: string) => KIND_STYLES[kind] ?? KIND_STYLES.system;

  async function readAsAdmin() {
    readingAsAdmin = true;
    await loadEntriesAsAdmin();
    readingAsAdmin = false;
  }

  const notSupported = () => showInfo(t("entries.notSupportedTitle"), t("entries.notSupported"));

  async function toggle(entry: MenuEntry) {
    const kind = category(entry);
    if ((kind !== "memtest" && kind !== "custom") || !entry.sourceScript) {
      await notSupported();
      return;
    }
    const done = await applyChange([{ op: "set-entry-enabled", script: entry.sourceScript, title: entry.displayName, enabled: !entry.enabled }], t("entries.updated"));
    if (done) await reloadEntries();
  }

  async function remove() {
    const entry = entries[selected];
    if (!entry) return;
    const kind = category(entry);
    if (kind === "memtest") {
      const binary = memtestBinary(entry.displayName) as string;
      if (!(await askConfirm(t("entries.removeTitle"), t("entries.removeMemtest", { name: binary }), t("common.remove"), true))) return;
      if (await applyChange([{ op: "delete-memtest", binary }], t("entries.removed"))) {
        selected = -1;
        await reloadEntries();
      }
    } else if (kind === "custom") {
      if (!(await askConfirm(t("entries.removeTitle"), t("entries.removeCustom", { name: entry.displayName }), t("common.remove"), true))) return;
      if (await applyChange([{ op: "delete-entry", script: "40_custom", title: entry.displayName }], t("entries.removed"))) {
        selected = -1;
        await reloadEntries();
      }
    } else {
      await notSupported();
    }
  }

  let adding = false;
  let newEntry = "menuentry 'My custom entry' {\n\n}\n";

  async function add() {
    const text = newEntry;
    adding = false;
    if (!text.trim()) return;
    if (await applyChange([{ op: "append-custom-entry", text }], t("entries.added"))) await reloadEntries();
  }
</script>

<div class="flex flex-col gap-3">
  <Hint text={$_("hints.entriesIntro")} />
  {#if $entriesState.unreadable}
    <div class="flex flex-wrap items-center gap-3 rounded border border-border bg-surface px-3 py-2">
      <Lock size={15} class="shrink-0 text-text-muted" />
      <p class="min-w-0 flex-1 text-[0.78rem] text-text">{$_("entries.unreadable")}</p>
      <button class="px-3 py-1.5 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.78rem] font-semibold disabled:opacity-60" disabled={readingAsAdmin} on:click={readAsAdmin}>
        {readingAsAdmin ? $_("entries.reading") : $_("entries.read")}
      </button>
    </div>
  {/if}

  <div class="grid gap-2 2xl:grid-cols-2 items-start">
    {#if $entriesState.loading && !$entriesState.loaded}
      <p class="rounded-xl border border-border bg-surface p-6 text-center text-[0.82rem] text-text-muted">{$_("entries.loading")}</p>
    {:else if entries.length === 0}
      <p class="rounded-xl border border-border bg-surface p-6 text-center text-[0.82rem] text-text-muted">{$_("entries.empty")}</p>
    {:else}
      {#each entries as entry, index (entry.name + index)}
        {@const parents = parentsOf(entry)}
        <div
          role="button"
          tabindex="0"
          class="flex items-center gap-3 rounded-xl border px-3.5 py-3 cursor-pointer transition-colors {selected === index ? 'border-primary bg-primary/10 ring-1 ring-primary/40' : 'border-border bg-surface hover:bg-surface-2'} {entry.enabled ? '' : 'opacity-60'}"
          on:click={() => (selected = index)}
          on:keydown={(e) => (e.key === "Enter" || e.key === " ") && (selected = index)}
        >
          <span class="w-7 h-7 shrink-0 rounded-lg bg-surface-2 text-text-muted grid place-items-center text-[0.72rem] font-semibold">{index}</span>
          <div class="min-w-0 flex-1">
            {#if parents.length > 0}
              <p class="truncate text-[0.7rem] text-text-muted">{parents.join(" › ")}</p>
            {/if}
            <p class="text-[0.86rem] font-semibold text-text-bright break-words">{entry.displayName}</p>
            {#if entry.path}
              <p class="mt-0.5 truncate font-mono text-[0.7rem] text-text-muted" title={entry.path}>{entry.path}</p>
            {/if}
          </div>
          <span class="shrink-0 rounded-full px-2.5 py-0.5 text-[0.68rem] font-semibold {kindClass(entry.kind)}">{$_(`entries.kind.${entry.kind}`)}</span>
          <button
            role="switch"
            aria-checked={entry.enabled}
            aria-label={entry.displayName}
            title={$_("entries.colEnabled")}
            class="relative shrink-0 w-10 h-[22px] rounded-full border transition-colors {entry.enabled ? 'bg-primary border-primary' : 'bg-surface-2 border-border'}"
            on:click|stopPropagation={() => toggle(entry)}
          >
            <span class="absolute top-[3px] left-[3px] w-4 h-4 rounded-full bg-text-bright shadow transition-transform {entry.enabled ? 'translate-x-[18px]' : ''}"></span>
          </button>
        </div>
      {/each}
    {/if}
  </div>

  <div class="flex gap-2">
    <button class="flex items-center gap-1.5 px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold" on:click={() => (adding = true)}>
      <Plus size={14} />
      {$_("entries.add")}
    </button>
    <button class="flex items-center gap-1.5 px-4 py-2 rounded border border-danger text-danger hover:bg-danger/10 text-[0.82rem] font-semibold disabled:opacity-40" disabled={selected < 0} on:click={remove}>
      <Trash2 size={14} />
      {$_("entries.remove")}
    </button>
  </div>
  <Hint text={$_("hints.entriesToggle")} />
</div>

{#if adding}
  <Modal variant="confirm" title={$_("entries.addTitle")} confirmLabel={$_("common.save")} maxWidth={600} confirmDisabled={!newEntry.trim()} onClose={() => (adding = false)} onConfirm={add}>
    <p class="mb-2">{$_("entries.addHint")}</p>
    <textarea
      class="w-full h-[220px] bg-panel border border-border rounded p-2 font-mono text-[0.78rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60 resize-none"
      spellcheck="false"
      bind:value={newEntry}
    ></textarea>
  </Modal>
{/if}
