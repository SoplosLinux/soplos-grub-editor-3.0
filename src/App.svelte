<script lang="ts">
  import { setContext, onMount, type ComponentType } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getVersion } from "@tauri-apps/api/app";
  import { _ } from "svelte-i18n";
  import { SlidersHorizontal, ListOrdered, Palette, TriangleAlert, Eye } from "lucide-svelte";
  import { api } from "./lib/api";
  import type { Presence } from "./lib/grub";
  import Sidebar from "./components/Sidebar.svelte";
  import AboutDialog from "./components/AboutDialog.svelte";
  import DialogHost from "./components/DialogHost.svelte";
  import NotesPanel from "./components/NotesPanel.svelte";
  import { showNotes } from "./lib/ui";
  import General from "./views/General.svelte";
  import Entries from "./views/Entries.svelte";
  import Appearance from "./views/Appearance.svelte";

  $: tabs = [
    {
      id: "general",
      icon: SlidersHorizontal,
      label: $_("app.tabs.general.label"),
      title: $_("app.tabs.general.title"),
      description: $_("app.tabs.general.description"),
    },
    {
      id: "entries",
      icon: ListOrdered,
      label: $_("app.tabs.entries.label"),
      title: $_("app.tabs.entries.title"),
      description: $_("app.tabs.entries.description"),
    },
    {
      id: "appearance",
      icon: Palette,
      label: $_("app.tabs.appearance.label"),
      title: $_("app.tabs.appearance.title"),
      description: $_("app.tabs.appearance.description"),
    },
  ];

  const views: Record<string, ComponentType> = {
    general: General,
    entries: Entries,
    appearance: Appearance,
  };

  let active = "general";
  let appVersion = "";
  // null until it is known whether this system boots with GRUB at all.
  let presence: Presence | null = null;

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch {
      appVersion = "";
    }
    try {
      presence = await api.invoke<Presence>("grub_present");
    } catch {
      presence = { hasConfig: true, hasUpdateGrub: true };
    }
  });

  function select(id: string) {
    active = id;
    document.getElementById("main-scroll")?.scrollTo({ top: 0 });
  }

  setContext("navigate", select);
  setContext("openAbout", () => (aboutOpen = true));

  $: activeTab = tabs.find((t) => t.id === active)!;
  $: ActiveView = views[active];
  $: hasGrub = presence ? presence.hasConfig && presence.hasUpdateGrub : true;

  let aboutOpen = false;

  // F1 -> About, Ctrl+Q -> quit, Ctrl+Tab / Ctrl+Shift+Tab -> cycle sections
  // (the same shortcuts as the 2.x version).
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "F1") {
      e.preventDefault();
      aboutOpen = true;
      return;
    }
    if (!e.ctrlKey) return;
    if (e.key === "q" || e.key === "Q") {
      e.preventDefault();
      getCurrentWindow().close();
    } else if (e.key === "i" || e.key === "I") {
      e.preventDefault();
      showNotes.update((v) => !v);
    } else if (e.code === "Tab") {
      e.preventDefault();
      const dir = e.shiftKey ? -1 : 1;
      const idx = tabs.findIndex((t) => t.id === active);
      select(tabs[(idx + dir + tabs.length) % tabs.length].id);
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="flex h-screen overflow-hidden">
  <Sidebar {tabs} {active} onSelect={select} {appVersion} />

  <div class="flex-1 flex flex-col overflow-hidden min-w-0">
    <div class="p-4 pb-0 shrink-0">
      <div
        class="relative flex min-h-[100px] items-end gap-3 overflow-hidden rounded border border-border p-4"
        style="background: linear-gradient(rgba(20, 20, 22, 0.25), rgba(20, 20, 22, 0.82)), url('/assets/hero/grub-editor-background.jpg'); background-size: cover; background-position: center;"
      >
        <div class="min-w-0">
          <p class="text-white font-semibold text-[0.95rem]">{activeTab.title}</p>
          <p class="text-[0.75rem] text-white/85 mt-0.5">{activeTab.description}</p>
        </div>
        <div class="ml-auto flex shrink-0 flex-wrap gap-1.5">
          <button
            class="flex items-center gap-1.5 rounded-md border bg-black/45 px-[9px] py-1 text-[0.78rem] hover:bg-black/60 {$showNotes ? 'border-primary text-primary' : 'border-white/25 text-white'}"
            aria-pressed={$showNotes}
            title="{$_('shell.notes')} (Ctrl+I)"
            on:click={() => showNotes.update((v) => !v)}
          >
            <Eye size={13} />
            {$_("shell.notes")}
          </button>
        </div>
      </div>
    </div>
    <div id="main-scroll" class="content-scroll flex-1 overflow-y-auto p-7">
      {#if presence && !hasGrub}
        <div class="max-w-[520px] mx-auto mt-10 flex flex-col items-center text-center gap-3">
          <TriangleAlert size={36} class="text-warning" />
          <h2 class="text-text-bright font-semibold text-[1.05rem]">{$_("noGrub.title")}</h2>
          <p class="text-[0.85rem] text-text">{$_("noGrub.body")}</p>
        </div>
      {:else}
        <div class="grid gap-4 {$showNotes ? 'items-start lg:grid-cols-[minmax(0,1fr)_290px]' : ''}">
          <div class="min-w-0">
            <svelte:component this={ActiveView} />
          </div>
          {#if $showNotes}
            <NotesPanel page={active === "entries" ? "entries" : active === "appearance" ? "appearance" : "general"} />
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>

{#if aboutOpen}
  <AboutDialog onClose={() => (aboutOpen = false)} />
{/if}

<DialogHost />
