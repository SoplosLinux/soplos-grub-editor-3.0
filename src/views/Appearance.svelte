<script lang="ts">
  import { onMount } from "svelte";
  import { _ } from "svelte-i18n";
  import { open } from "@tauri-apps/plugin-dialog";
  import { Check, FolderOpen, ImageOff, Palette, Type, Paintbrush, PackagePlus } from "lucide-svelte";
  import { api } from "../lib/api";
  import { applyChange, readConfig, runOps, tagged, t, visibleOutput, type Config, type Op } from "../lib/grub";
  import { askConfirm, showError, showInfo } from "../lib/dialogs";
  import Card from "../components/Card.svelte";
  import Hint from "../components/Hint.svelte";

  // Themes, background and colors, and fonts. A theme and a custom background
  // exclude each other, so applying one clears the other, and the scripts that
  // inject a theme of their own (05_debian_theme, 05_soplos_theme) are turned
  // off with it so they do not override the choice.

  type Panel = "theme" | "background" | "fonts";

  // The sixteen colors GRUB knows, with the VGA color each is drawn as.
  const GRUB_COLORS: { name: string; hex: string }[] = [
    { name: "black", hex: "#000000" },
    { name: "blue", hex: "#0000aa" },
    { name: "green", hex: "#00aa00" },
    { name: "cyan", hex: "#00aaaa" },
    { name: "red", hex: "#aa0000" },
    { name: "magenta", hex: "#aa00aa" },
    { name: "brown", hex: "#aa5500" },
    { name: "light-gray", hex: "#aaaaaa" },
    { name: "dark-gray", hex: "#555555" },
    { name: "light-blue", hex: "#5555ff" },
    { name: "light-green", hex: "#55ff55" },
    { name: "light-cyan", hex: "#55ffff" },
    { name: "light-red", hex: "#ff5555" },
    { name: "light-magenta", hex: "#ff55ff" },
    { name: "yellow", hex: "#ffff55" },
    { name: "white", hex: "#ffffff" },
  ];
  const colorNames = GRUB_COLORS.map((c) => c.name);
  const hexOf = (name: string) => GRUB_COLORS.find((c) => c.name === name)?.hex ?? "#000000";

  let panel: Panel = "theme";
  let config: Config = {};
  let themes: string[] = [];
  let themePreviews: Record<string, string | null> = {};
  let fonts: string[] = [];
  let selectedTheme = "";
  let selectedFont = "";
  let background = "";
  let backgroundPreview: string | null = null;
  let textColor = "white";
  let backgroundColor = "black";
  let highlightText = "black";
  let highlightBackground = "white";
  let fontFile = "";
  let fontSize = 16;
  let busy = false;

  async function preview(command: "read_theme_preview" | "read_image_preview", args: Record<string, string>): Promise<string | null> {
    try {
      return await api.invoke<string | null>(command, args);
    } catch {
      return null;
    }
  }

  let backgroundRequest = 0;
  async function showBackgroundPreview() {
    const mine = ++backgroundRequest;
    const url = background.trim() ? await preview("read_image_preview", { path: background.trim() }) : null;
    if (mine === backgroundRequest) backgroundPreview = url;
  }

  function splitPair(value: string | undefined, fallback: [string, string]): [string, string] {
    const [text, back] = (value ?? "").split("/");
    return colorNames.includes(text) && colorNames.includes(back) ? [text, back] : fallback;
  }

  async function load() {
    try {
      config = await readConfig();
    } catch {
      config = {};
    }
    themes = await api.invoke<string[]>("list_themes").catch(() => []);
    fonts = await api.invoke<string[]>("list_fonts").catch(() => []);
    const custom = await api.invoke<{ normal: string; highlight: string }>("read_custom_colors").catch(() => ({ normal: "", highlight: "" }));

    // The theme in use is the folder name in GRUB_THEME.
    const themePath = config.GRUB_THEME ?? "";
    const currentTheme = themePath ? (themePath.split("/").slice(-2, -1)[0] ?? "") : "";
    selectedTheme = themes.includes(currentTheme) ? currentTheme : (themes[0] ?? "");
    const fontPath = config.GRUB_FONT ?? "";
    const currentFont = fontPath.split("/").pop() ?? "";
    selectedFont = fonts.includes(currentFont) ? currentFont : (fonts[0] ?? "");
    background = config.GRUB_BACKGROUND ?? "";

    const [text, back] = splitPair(config.GRUB_COLOR_NORMAL || custom.normal, ["white", "black"]);
    const [highlight, highlightBack] = splitPair(config.GRUB_COLOR_HIGHLIGHT || custom.highlight, ["black", "white"]);
    textColor = text;
    backgroundColor = back;
    highlightText = highlight;
    highlightBackground = highlightBack;

    // Without a theme in use, the page opens on the custom look.
    if (!(config.GRUB_THEME ?? "") && (background || custom.normal || config.GRUB_COLOR_NORMAL)) panel = "background";
    await showBackgroundPreview();

    // One thumbnail per installed theme.
    const found: Record<string, string | null> = {};
    for (const name of themes) found[name] = await preview("read_theme_preview", { name });
    themePreviews = found;
  }

  onMount(load);

  const themeFolder = (name: string) => `/boot/grub/themes/${name}/`;
  const activeTheme = (name: string) => (config.GRUB_THEME ?? "").includes(themeFolder(name));
  $: themeInUse = themes.find((name) => activeTheme(name)) ?? "";

  // The settings that leave GRUB with the given custom background and colors
  // and no theme. Empty colors clear custom.cfg.
  function customLook(bg: string, normal: string, highlight: string): Op[] {
    return [
      {
        op: "save-config",
        settings: [
          { key: "GRUB_THEME", value: "" },
          { key: "GRUB_BACKGROUND", value: bg },
          { key: "GRUB_COLOR_NORMAL", value: "" },
          { key: "GRUB_COLOR_HIGHLIGHT", value: "" },
        ],
      },
      { op: "write-colors", normal, highlight },
      { op: "set-theme-scripts", active: false },
    ];
  }

  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try {
      await action();
    } finally {
      busy = false;
    }
  }

  async function chooseFile(title: string, name: string, extensions: string[]): Promise<string | null> {
    const picked = await open({ title, multiple: false, directory: false, filters: [{ name, extensions }] });
    return typeof picked === "string" ? picked : null;
  }

  // ---- Themes

  const installTheme = () =>
    run(async () => {
      const archive = await chooseFile(t("appearance.selectArchive"), t("appearance.archives"), ["zip", "tar.gz", "tgz", "tar.xz", "tar.bz2", "gz", "xz", "bz2"]);
      if (!archive) return;
      const result = await runOps([{ op: "install-theme", archive }]);
      if (!result.ok) {
        await showError(t("common.error"), t("appearance.installFailed"), visibleOutput(result.output));
        return;
      }
      const name = tagged(result.output, "theme")[0] ?? "";
      await load();
      if (name && themes.includes(name)) selectedTheme = name;
      await showInfo(t("common.done"), t("appearance.installed", { name }));
    });

  const applyTheme = () =>
    run(async () => {
      if (!selectedTheme) return;
      if (!(await askConfirm(t("appearance.applyThemeTitle"), t("appearance.applyThemeBody", { name: selectedTheme }), t("common.apply")))) return;
      const ops: Op[] = [
        {
          op: "save-config",
          settings: [
            { key: "GRUB_THEME", value: `/boot/grub/themes/${selectedTheme}/theme.txt` },
            { key: "GRUB_BACKGROUND", value: "" },
            { key: "GRUB_COLOR_NORMAL", value: "" },
            { key: "GRUB_COLOR_HIGHLIGHT", value: "" },
            { key: "GRUB_FONT", value: "" },
          ],
        },
        { op: "write-colors", normal: "", highlight: "" },
        { op: "set-theme-scripts", active: false },
      ];
      if (await applyChange(ops, t("appearance.themeApplied"))) {
        background = "";
        await load();
      }
    });

  const disableTheme = () =>
    run(async () => {
      if (!(await askConfirm(t("appearance.disableThemeTitle"), t("appearance.disableThemeBody"), t("appearance.disable")))) return;
      if (await applyChange(customLook("", "white/black", "black/white"), t("appearance.themeDisabled"))) await load();
    });

  const removeTheme = () =>
    run(async () => {
      const name = selectedTheme;
      if (!name) return;
      if (!(await askConfirm(t("appearance.removeThemeTitle"), t("appearance.removeThemeBody", { name }), t("common.remove"), true))) return;
      const ops: Op[] = [{ op: "remove-theme", name }];
      if (activeTheme(name)) {
        // The theme is in use: GRUB goes back to plain colors with it.
        await applyChange([...ops, ...customLook("", "white/black", "black/white")], t("appearance.themeRemoved"));
      } else {
        const result = await runOps(ops);
        if (!result.ok) {
          await showError(t("common.error"), t("appearance.removeFailed"), visibleOutput(result.output));
          return;
        }
        await showInfo(t("common.done"), t("appearance.themeRemoved"));
      }
      await load();
    });

  // ---- Background and colors

  const browseBackground = async () => {
    const picked = await chooseFile(t("appearance.selectImage"), t("appearance.images"), ["png", "jpg", "jpeg"]);
    if (picked) {
      background = picked;
      await showBackgroundPreview();
    }
  };

  async function applyLook(bg: string, askFirst: boolean) {
    if (askFirst && !(await askConfirm(t("appearance.applyLookTitle"), t("appearance.applyLookBody"), t("common.apply")))) return;
    const done = await applyChange(customLook(bg, `${textColor}/${backgroundColor}`, `${highlightText}/${highlightBackground}`), t("appearance.lookApplied"));
    if (done) await load();
  }

  const applyBackground = () => run(() => applyLook(background.trim(), true));

  const removeBackground = () =>
    run(async () => {
      if (!(await askConfirm(t("appearance.removeBackgroundTitle"), t("appearance.removeBackgroundBody"), t("common.remove"), true))) return;
      background = "";
      backgroundPreview = null;
      // The colors stay; only the image goes.
      await applyLook("", false);
    });

  // ---- Fonts

  const browseFont = async () => {
    const picked = await chooseFile(t("appearance.selectFont"), t("appearance.fontFiles"), ["ttf", "otf", "ttc", "pcf"]);
    if (picked) fontFile = picked;
  };

  const convertFont = () =>
    run(async () => {
      if (!fontFile.trim()) {
        await showError(t("common.error"), t("appearance.noFont"));
        return;
      }
      const size = Math.min(72, Math.max(8, Math.trunc(Number(fontSize)) || 16));
      const result = await runOps([{ op: "convert-font", source: fontFile.trim(), size }]);
      if (!result.ok) {
        await showError(t("common.error"), t("appearance.convertFailed"), visibleOutput(result.output));
        return;
      }
      const name = tagged(result.output, "font")[0] ?? "";
      await load();
      if (name && fonts.includes(name)) selectedFont = name;
      await showInfo(t("common.done"), t("appearance.converted", { name }));
    });

  const applyFont = () =>
    run(async () => {
      if (!selectedFont) return;
      const done = await applyChange([{ op: "save-config", settings: [{ key: "GRUB_FONT", value: `/boot/grub/fonts/${selectedFont}` }] }], t("appearance.fontApplied"));
      if (done) await load();
    });

  const removeFont = () =>
    run(async () => {
      const name = selectedFont;
      if (!name) return;
      if (!(await askConfirm(t("appearance.removeFontTitle"), t("appearance.removeFontBody", { name }), t("common.remove"), true))) return;
      const ops: Op[] = [{ op: "remove-font", name }];
      if ((config.GRUB_FONT ?? "") === `/boot/grub/fonts/${name}`) {
        // The font in use goes, so the setting that names it goes too.
        await applyChange([...ops, { op: "save-config", settings: [{ key: "GRUB_FONT", value: "" }] }], t("appearance.fontRemoved"));
      } else {
        const result = await runOps(ops);
        if (!result.ok) {
          await showError(t("common.error"), t("appearance.removeFailed"), visibleOutput(result.output));
          return;
        }
        await showInfo(t("common.done"), t("appearance.fontRemoved"));
      }
      await load();
    });

  const field = "bg-surface-2 border border-border rounded-lg px-3 py-2 text-[0.85rem] text-text-bright outline-none focus-visible:ring-2 focus-visible:ring-primary/60";
  const secondary = "px-4 py-2.5 rounded-lg border border-border text-text hover:bg-surface-2 text-[0.82rem] disabled:opacity-50";
  const primaryButton = "px-5 py-2.5 rounded-lg bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold shadow disabled:opacity-50";
  const dangerButton = "px-4 py-2.5 rounded-lg border border-danger text-danger hover:bg-danger/10 text-[0.82rem] font-semibold disabled:opacity-50";

  $: panels = [
    { id: "theme" as Panel, label: $_("appearance.tabTheme"), icon: Palette },
    { id: "background" as Panel, label: $_("appearance.tabBackground"), icon: Paintbrush },
    { id: "fonts" as Panel, label: $_("appearance.tabFonts"), icon: Type },
  ];

  $: colorRows = [
    { label: $_("appearance.textColor"), value: textColor, set: (v: string) => (textColor = v) },
    { label: $_("appearance.backgroundColor"), value: backgroundColor, set: (v: string) => (backgroundColor = v) },
    { label: $_("appearance.highlightText"), value: highlightText, set: (v: string) => (highlightText = v) },
    { label: $_("appearance.highlightBackground"), value: highlightBackground, set: (v: string) => (highlightBackground = v) },
  ];
</script>

<div class="flex flex-col gap-4">
  <div class="flex gap-1.5 p-1 rounded-xl border border-border bg-surface shadow-sm self-start" role="tablist">
    {#each panels as item (item.id)}
      <button
        role="tab"
        aria-selected={panel === item.id}
        class="flex items-center gap-2 px-4 py-2 rounded-lg text-[0.84rem] transition-colors {panel === item.id ? 'bg-primary text-text-bright font-semibold shadow' : 'text-text-muted hover:text-text-bright hover:bg-surface-2'}"
        on:click={() => (panel = item.id)}
      >
        <svelte:component this={item.icon} size={15} />
        {item.label}
      </button>
    {/each}
  </div>

  {#if panel === "theme"}
    <Card title={$_("appearance.tabTheme")} subtitle={$_("appearance.themeInfo")} icon={Palette}>
      {#if themes.length === 0}
        <div class="flex flex-col items-center text-center gap-3 py-8">
          <span class="w-14 h-14 rounded-2xl bg-primary/15 text-primary grid place-items-center"><PackagePlus size={26} /></span>
          <h3 class="text-text-bright font-semibold text-[0.95rem]">{$_("appearance.emptyThemesTitle")}</h3>
          <p class="max-w-[420px] text-[0.8rem] text-text-muted">{$_("appearance.emptyThemesBody")}</p>
          <button class={primaryButton} disabled={busy} on:click={installTheme}>{$_("appearance.install")}</button>
        </div>
      {:else}
        <div class="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(190px,1fr))]">
          {#each themes as name (name)}
            <button
              class="group text-left rounded-xl border overflow-hidden transition-all {selectedTheme === name ? 'border-primary ring-2 ring-primary/50 shadow-lg' : 'border-border hover:border-primary/60'} bg-panel"
              on:click={() => (selectedTheme = name)}
            >
              <div class="aspect-[16/10] bg-black/40 grid place-items-center overflow-hidden">
                {#if themePreviews[name]}
                  <img src={themePreviews[name]} alt={name} class="w-full h-full object-cover" />
                {:else}
                  <ImageOff size={26} class="text-text-muted" />
                {/if}
              </div>
              <div class="flex items-center justify-between gap-2 px-3 py-2">
                <span class="truncate text-[0.82rem] font-semibold {selectedTheme === name ? 'text-primary' : 'text-text-bright'}">{name}</span>
                {#if themeInUse === name}
                  <span class="flex items-center gap-1 shrink-0 rounded-full bg-success/15 px-2 py-0.5 text-[0.62rem] font-semibold text-success"><Check size={10} />{$_("appearance.inUse")}</span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
        <Hint text={$_("hints.themeChoice")} />
        <div class="flex flex-wrap gap-2 mt-4">
          <button class={primaryButton} disabled={busy || !selectedTheme} on:click={applyTheme}>{$_("common.apply")}</button>
          <button class={secondary} disabled={busy} on:click={installTheme}>{$_("appearance.install")}</button>
          <button class={secondary} disabled={busy || !themeInUse} on:click={disableTheme}>{$_("appearance.disable")}</button>
          <button class="{dangerButton} ml-auto" disabled={busy || !selectedTheme} on:click={removeTheme}>{$_("common.remove")}</button>
        </div>
      {/if}
    </Card>
  {:else if panel === "background"}
    <div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-start">
      <Card title={$_("appearance.sampleTitle")} subtitle={$_("appearance.colorsInfo")} icon={Paintbrush}>
        <div class="aspect-[16/10] rounded-xl border border-border overflow-hidden relative grid place-items-center" style="background-color: {hexOf(backgroundColor)};">
          {#if backgroundPreview}
            <img src={backgroundPreview} alt="" class="absolute inset-0 w-full h-full object-cover" />
          {/if}
          <div class="relative w-[78%] rounded-md border border-white/30 p-1.5 font-mono text-[0.72rem]" style="background: {hexOf(backgroundColor)}cc; color: {hexOf(textColor)};">
            <div class="px-2 py-1 rounded-sm" style="background: {hexOf(highlightBackground)}; color: {hexOf(highlightText)};">* Soplos Linux</div>
            <div class="px-2 py-1">{$_("appearance.sampleAdvanced")}</div>
            <div class="px-2 py-1">{$_("appearance.sampleMemtest")}</div>
          </div>
        </div>
        {#if !backgroundPreview}
          <p class="mt-2 flex items-center gap-1.5 text-[0.72rem] text-text-muted"><ImageOff size={12} />{$_("appearance.noImage")}</p>
        {/if}
      </Card>

      <Card title={$_("appearance.tabBackground")} icon={Paintbrush}>
        <label for="background-path" class="block text-[0.82rem] font-medium text-text-bright mb-1.5">{$_("appearance.backgroundImage")}</label>
        <div class="flex gap-2">
          <input id="background-path" type="text" class="{field} flex-1 min-w-0" placeholder={$_("appearance.noImage")} bind:value={background} on:change={showBackgroundPreview} />
          <button class={secondary} disabled={busy} on:click={browseBackground} title={$_("appearance.browse")}><FolderOpen size={15} /></button>
          <button class={dangerButton} disabled={busy} on:click={removeBackground}>{$_("common.remove")}</button>
        </div>
        <Hint text={$_("hints.backgroundImage")} />

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-3 gap-y-3 mt-4">
          {#each colorRows as row}
            <div>
              <span class="block text-[0.78rem] text-text-muted mb-1">{row.label}</span>
              <div class="flex items-center gap-2">
                <span class="w-6 h-6 rounded-md border border-border shrink-0" style="background: {hexOf(row.value)}"></span>
                <select class="{field} w-full" value={row.value} on:change={(e) => row.set(e.currentTarget.value)}>
                  {#each GRUB_COLORS as color}
                    <option value={color.name}>{$_(`colors.${color.name}`)}</option>
                  {/each}
                </select>
              </div>
            </div>
          {/each}
        </div>
        <Hint text={$_("hints.colors")} />
        <div class="flex justify-end mt-5">
          <button class={primaryButton} disabled={busy} on:click={applyBackground}>{$_("appearance.applyLook")}</button>
        </div>
      </Card>
    </div>
  {:else}
    <div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-start">
      <Card title={$_("appearance.convertTitle")} subtitle={$_("appearance.convertInfo")} icon={Type}>
        <div class="flex gap-2">
          <input type="text" class="{field} flex-1 min-w-0" placeholder={$_("appearance.fontPlaceholder")} bind:value={fontFile} />
          <button class={secondary} disabled={busy} on:click={browseFont} title={$_("appearance.browse")}><FolderOpen size={15} /></button>
        </div>
        <Hint text={$_("hints.fontConvert")} />
        <div class="flex items-end gap-3 mt-4">
          <div>
            <label for="font-size" class="block text-[0.78rem] text-text-muted mb-1">{$_("appearance.fontSize")}</label>
            <input id="font-size" type="number" min="8" max="72" class="{field} w-[100px]" bind:value={fontSize} />
          </div>
          <button class="{primaryButton} ml-auto" disabled={busy} on:click={convertFont}>{$_("appearance.convert")}</button>
        </div>
      </Card>

      <Card title={$_("appearance.installedFonts")} icon={Type}>
        {#if fonts.length === 0}
          <p class="text-[0.8rem] text-text-muted py-4 text-center">{$_("appearance.noFonts")}</p>
        {:else}
          <div class="flex flex-col gap-1.5 max-h-[260px] overflow-y-auto overscroll-contain content-scroll pr-1">
            {#each fonts as name (name)}
              <button
                class="flex items-center justify-between gap-2 text-left rounded-lg border px-3 py-2 text-[0.82rem] transition-colors {selectedFont === name ? 'border-primary bg-primary/10 text-primary font-semibold' : 'border-border text-text hover:bg-surface-2'}"
                on:click={() => (selectedFont = name)}
              >
                <span class="truncate">{name}</span>
                {#if (config.GRUB_FONT ?? "").endsWith(`/${name}`)}
                  <span class="flex items-center gap-1 shrink-0 rounded-full bg-success/15 px-2 py-0.5 text-[0.62rem] font-semibold text-success"><Check size={10} />{$_("appearance.inUse")}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
        <Hint text={$_("hints.fontApply")} />
        <div class="flex gap-2 mt-4">
          <button class={primaryButton} disabled={busy || !selectedFont} on:click={applyFont}>{$_("appearance.applyFont")}</button>
          <button class="{dangerButton} ml-auto" disabled={busy || !selectedFont} on:click={removeFont}>{$_("appearance.removeFont")}</button>
        </div>
      </Card>
    </div>
  {/if}
</div>
