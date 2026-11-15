<script lang="ts">
  import { onMount, tick } from "svelte";
  import { _ } from "svelte-i18n";
  import { Lock, Rocket, Cpu, SlidersHorizontal, Pin, PackagePlus, History } from "lucide-svelte";
  import Card from "../components/Card.svelte";
  import Hint from "../components/Hint.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { readConfig, t, type Config, type Setting } from "../lib/grub";
  import { queue, unqueue, isPending, onReleased } from "../lib/pending";
  import { entriesState, ensureEntries, loadEntriesAsAdmin } from "../lib/entries";

  // The general settings of /etc/default/grub. The boot entry is only written
  // when the user chose one: opening this page and applying must never
  // pin the default entry to the current kernel, which would keep any later
  // kernel from being booted.

  type BootMode = "fixed" | "last_installed" | "last_booted";

  const RESOLUTIONS = [
    "auto",
    // 16:9
    "3840x2160", "2560x1440", "1920x1080", "1600x900", "1366x768", "1280x720",
    // 16:10
    "2560x1600", "1920x1200", "1680x1050", "1440x900", "1280x800",
    // 4:3 and older
    "1600x1200", "1280x1024", "1024x768", "800x600", "640x480",
  ];

  let config: Config = {};
  let configLoaded = false;
  let bootMode: BootMode = "last_installed";
  let selectedEntry = "";
  // GRUB_DEFAULT as found, when it is not the name of an entry (for example
  // "saved"): the list shows it verbatim so nothing is lost.
  let extraEntry: string | null = null;
  let originalDefault: string | null = null;
  let initialMode: BootMode | null = null;
  let initialEntry = "";
  let entryTouched = false;

  let timeout = 5;
  let resolution = "auto";
  // GRUB_GFXMODE as found when it is not one of the listed modes (a list such
  // as "1920x1080,auto", for example): it stays selectable so applying never
  // replaces it with "auto" unasked.
  let extraResolution: string | null = null;
  let cmdline = "";
  let showMenu = true;
  let detectOs = true;
  let recovery = true;
  let useUuid = true;
  let disableSubmenu = false;

  const truthy = (value: string | undefined) => ["true", "y", "yes", "1"].includes((value ?? "").toLowerCase());

  function pickInitial(names: string[], value: string): { selected: string; extra: string | null } {
    if (/^\d+$/.test(value)) {
      const index = Number(value);
      return { selected: index < names.length ? names[index] : (names[0] ?? ""), extra: null };
    }
    if (names.includes(value)) return { selected: value, extra: null };
    return { selected: value, extra: value };
  }

  function loadFields() {
    const value = config.GRUB_DEFAULT ?? "0";
    originalDefault = value;
    const names = $entriesState.entries.map((e) => e.name);
    const initial = pickInitial(names, value);
    selectedEntry = initial.selected;
    extraEntry = initial.extra;
    initialEntry = selectedEntry;

    // The mode comes from the two keys that define it. "saved" without
    // GRUB_SAVEDEFAULT is not "last booted": it boots whatever grub-set-default
    // recorded, so it stays a fixed entry and is shown verbatim.
    if (value === "saved" && truthy(config.GRUB_SAVEDEFAULT)) bootMode = "last_booted";
    else if (value === "0") bootMode = "last_installed";
    else bootMode = "fixed";
    initialMode = bootMode;

    const parsedTimeout = Number.parseInt(config.GRUB_TIMEOUT ?? "5", 10);
    timeout = Number.isNaN(parsedTimeout) ? 5 : parsedTimeout;
    const mode = config.GRUB_GFXMODE ?? "auto";
    resolution = mode;
    extraResolution = RESOLUTIONS.includes(mode) ? null : mode;
    cmdline = config.GRUB_CMDLINE_LINUX_DEFAULT ?? "";
    showMenu = (config.GRUB_TIMEOUT_STYLE ?? "menu") !== "hidden";
    recovery = (config.GRUB_DISABLE_RECOVERY ?? "false").toLowerCase() !== "true";
    // GRUB 2.06 and later leave os-prober off when the key is missing.
    detectOs = (config.GRUB_DISABLE_OS_PROBER ?? "true").toLowerCase() !== "true";
    useUuid = (config.GRUB_DISABLE_LINUX_UUID ?? "false").toLowerCase() !== "true";
    disableSubmenu = ["true", "y"].includes((config.GRUB_DISABLE_SUBMENU ?? "false").toLowerCase());
    entryTouched = false;
  }

  async function reload() {
    try {
      config = await readConfig();
    } catch (error) {
      console.error("Reading /etc/default/grub failed", error);
      config = {};
    }
    configLoaded = true;
    await ensureEntries();
    loadFields();
    await rebaseline();
  }

  onMount(() => {
    void reload();
    return onReleased((keys) => {
      if (keys === "all" || keys.includes("general")) void reload();
    });
  });

  // The entries can arrive after the page opens (read as administrator); until
  // the user touches the selector, it follows them.
  let seenEntries = $entriesState.entries;
  $: if (configLoaded && $entriesState.entries !== seenEntries) {
    seenEntries = $entriesState.entries;
    if (!entryTouched) {
      const initial = pickInitial(
        $entriesState.entries.map((e) => e.name),
        originalDefault ?? "0",
      );
      selectedEntry = initial.selected;
      extraEntry = initial.extra;
      initialEntry = selectedEntry;
      if (!isPending("general")) void rebaseline();
    }
  }

  $: if (timeout === 0) showMenu = false;
  $: isFixed = bootMode === "fixed";

  const label = (index: number, name: string, display: string) => (name.includes(">") ? `${index}: ${name.replaceAll(">", " » ")}` : `${index}: ${display}`);

  function buildSettings(): Setting[] {
    const settings: Setting[] = [];
    const add = (key: string, value: string) => settings.push({ key, value });

    if (bootMode === "last_installed") {
      // Entry 0 is the newest kernel, so this needs no extra key.
      add("GRUB_DEFAULT", "0");
      add("GRUB_SAVEDEFAULT", "");
    } else if (bootMode === "last_booted") {
      add("GRUB_DEFAULT", "saved");
      add("GRUB_SAVEDEFAULT", "true");
    } else {
      // The stored value is kept as it was unless something was chosen:
      // the same mode as on load and the same entry. Switching the mode to
      // "fixed" is itself a choice, so then the entry shown is written.
      const untouched = bootMode === initialMode && selectedEntry === initialEntry && originalDefault !== null;
      add("GRUB_DEFAULT", untouched ? (originalDefault as string) : selectedEntry || "0");
      add("GRUB_SAVEDEFAULT", "");
    }

    add("GRUB_TIMEOUT", String(Number.isFinite(timeout) ? Math.max(0, Math.trunc(timeout)) : 5));
    add("GRUB_GFXMODE", resolution || "auto");
    add("GRUB_CMDLINE_LINUX_DEFAULT", cmdline);

    // Only written when they differ from the default or the file already has them.
    const optional = (key: string, value: string, standard: string) => {
      if (key in config || value !== standard) add(key, value);
    };
    optional("GRUB_TIMEOUT_STYLE", showMenu ? "menu" : "hidden", "menu");
    optional("GRUB_DISABLE_RECOVERY", recovery ? "false" : "true", "false");
    optional("GRUB_DISABLE_OS_PROBER", detectOs ? "false" : "true", "true");
    optional("GRUB_DISABLE_LINUX_UUID", useUuid ? "false" : "true", "false");
    optional("GRUB_DISABLE_SUBMENU", disableSubmenu ? "y" : "false", "false");
    return settings;
  }

  // Nothing is written from this page. Any difference from what was read
  // from the disk waits as one pending change (replaced as the fields change,
  // dropped when they are back as they were), and the bar applies it.
  const snapshot = () => JSON.stringify([bootMode, selectedEntry, timeout, resolution, cmdline, showMenu, detectOs, recovery, useUuid, disableSubmenu]);
  let baseline = "";

  async function rebaseline() {
    await tick();
    baseline = snapshot();
  }

  $: current = JSON.stringify([bootMode, selectedEntry, timeout, resolution, cmdline, showMenu, detectOs, recovery, useUuid, disableSubmenu]);
  $: if (configLoaded && baseline) syncPending(current);

  function syncPending(now: string) {
    if (now === baseline) unqueue("general");
    else queue({ key: "general", label: t("pending.label.general"), ops: [{ op: "save-config", settings: buildSettings() }] });
  }

  let readingAsAdmin = false;
  async function readEntriesAsAdmin() {
    readingAsAdmin = true;
    await loadEntriesAsAdmin();
    readingAsAdmin = false;
  }

  $: modes = [
    { id: "fixed" as BootMode, icon: Pin, title: $_("general.modeFixed"), text: $_("general.modeFixedText") },
    { id: "last_installed" as BootMode, icon: PackagePlus, title: $_("general.modeLastInstalled"), text: $_("general.modeLastInstalledText") },
    { id: "last_booted" as BootMode, icon: History, title: $_("general.modeLastBooted"), text: $_("general.modeLastBootedText") },
  ];

  const field = "bg-surface-2 border border-border rounded px-2 py-1.5 text-[0.85rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60";
</script>

<div class="grid gap-4 xl:grid-cols-2 items-start">
  <Card title={$_("general.bootConfig")} subtitle={$_("general.bootConfigSub")} icon={Rocket}>
    <div class="flex flex-col gap-4">
      <div>
        <p class="text-[0.82rem] font-medium text-text-bright mb-2">{$_("general.bootMode")}</p>
        <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5" role="radiogroup" aria-label={$_("general.bootMode")}>
          {#each modes as mode (mode.id)}
            <button
              type="button"
              role="radio"
              aria-checked={bootMode === mode.id}
              class="text-left rounded-xl border p-3.5 transition-colors {bootMode === mode.id ? 'border-primary bg-primary/10 ring-1 ring-primary/50' : 'border-border bg-surface-2/40 hover:bg-surface-2'}"
              on:click={() => (bootMode = mode.id)}
            >
              <span class="flex items-center gap-2 text-[0.86rem] font-semibold {bootMode === mode.id ? 'text-primary' : 'text-text-bright'}">
                <svelte:component this={mode.icon} size={16} />
                {mode.title}
              </span>
              <span class="block mt-1.5 text-[0.74rem] leading-snug text-text-muted">{mode.text}</span>
            </button>
          {/each}
        </div>
        <Hint text={$_("hints.bootMode")} />
      </div>

      {#if isFixed}
        <div>
          <label for="boot-entry" class="block text-[0.82rem] font-medium text-text-bright mb-1.5">{$_("general.bootEntry")}</label>
          <select id="boot-entry" class={field} bind:value={selectedEntry} on:change={() => (entryTouched = true)}>
            {#if extraEntry !== null}
              <option value={extraEntry}>[{extraEntry}]</option>
            {/if}
            {#each $entriesState.entries as entry, index (entry.name + index)}
              <option value={entry.name}>{label(index, entry.name, entry.displayName)}</option>
            {/each}
          </select>
          <Hint text={$_("hints.bootEntry")} />
        </div>
      {/if}

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div>
          <label for="boot-timeout" class="block text-[0.82rem] font-medium text-text-bright mb-1.5">{$_("general.timeout")}</label>
          <input id="boot-timeout" type="number" min="0" max="60" class="{field} w-full" bind:value={timeout} />
          <Hint text={$_("hints.timeout")} />
        </div>
        <div>
          <label for="boot-resolution" class="block text-[0.82rem] font-medium text-text-bright mb-1.5">{$_("general.resolution")}</label>
          <select id="boot-resolution" class={field} bind:value={resolution}>
            {#if extraResolution !== null}
              <option value={extraResolution}>{extraResolution}</option>
            {/if}
            {#each RESOLUTIONS as value}
              <option {value}>{value === "auto" ? $_("general.resolutionAuto") : value}</option>
            {/each}
          </select>
          <Hint text={$_("hints.resolution")} />
        </div>
      </div>

      {#if $entriesState.unreadable}
        <div class="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface-2/60 px-3 py-2.5">
          <Lock size={15} class="shrink-0 text-text-muted" />
          <p class="min-w-0 flex-1 text-[0.78rem] text-text">{$_("entries.unreadable")}</p>
          <button class="px-3 py-1.5 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.78rem] font-semibold disabled:opacity-60" disabled={readingAsAdmin} on:click={readEntriesAsAdmin}>
            {readingAsAdmin ? $_("entries.reading") : $_("entries.read")}
          </button>
        </div>
      {/if}
    </div>
  </Card>

  <div class="flex flex-col gap-4">
    <Card title={$_("general.advanced")} subtitle={$_("general.advancedSub")} icon={SlidersHorizontal}>
      <div class="flex flex-col gap-3.5">
        <div>
          <Toggle label={$_("general.detectOs")} bind:checked={detectOs} />
          <Hint text={$_("hints.detectOs")} />
        </div>
        <div>
          <Toggle label={$_("general.showMenu")} bind:checked={showMenu} disabled={timeout === 0} title={timeout === 0 ? $_("general.menuNeedsTimeout") : ""} />
          <Hint text={$_("hints.showMenu")} />
        </div>
        <div>
          <Toggle label={$_("general.recovery")} bind:checked={recovery} />
          <Hint text={$_("hints.recovery")} />
        </div>
        <div>
          <Toggle label={$_("general.uuid")} bind:checked={useUuid} />
          <Hint text={$_("hints.uuid")} />
        </div>
        <div>
          <Toggle label={$_("general.disableSubmenu")} bind:checked={disableSubmenu} />
          <Hint text={$_("hints.submenu")} />
        </div>
      </div>
    </Card>

    <Card title={$_("general.kernelParams")} subtitle={$_("general.kernelParamsSub")} icon={Cpu}>
      <input type="text" class="{field} w-full font-mono" placeholder={$_("general.kernelPlaceholder")} bind:value={cmdline} maxlength="1000" />
      <Hint text={$_("hints.kernelParams")} />
    </Card>
  </div>
</div>
