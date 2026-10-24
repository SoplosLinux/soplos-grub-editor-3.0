<script lang="ts">
  import { _ } from "svelte-i18n";
  import { BookOpen, ShieldCheck, FileCode2 } from "lucide-svelte";

  // The panel beside a page while the notes are on: what the page does, what
  // to watch for, and which files it touches.
  export let page: "general" | "entries" | "appearance";

  const files: Record<string, string[]> = {
    general: ["/etc/default/grub", "/boot/grub/grub.cfg"],
    entries: ["/etc/grub.d/40_custom", "/etc/grub.d/20_memtest86+", "/boot/grub/grub.cfg"],
    appearance: ["/boot/grub/themes/", "/boot/grub/fonts/", "/boot/grub/custom.cfg", "/etc/default/grub"],
  };
</script>

<aside class="flex flex-col gap-2.5 text-[0.78rem] text-text">
  <div class="rounded-xl border border-border bg-surface px-3.5 py-3">
    <h3 class="flex items-center gap-1.5 mb-2 text-[0.66rem] font-semibold tracking-[0.07em] text-text-muted uppercase">
      <BookOpen size={12} class="text-primary" />
      {$_("notes.whatTitle")}
    </h3>
    <p class="leading-relaxed">{$_(`notes.${page}.what`)}</p>
  </div>
  <div class="rounded-xl border border-border bg-surface px-3.5 py-3">
    <h3 class="flex items-center gap-1.5 mb-2 text-[0.66rem] font-semibold tracking-[0.07em] text-text-muted uppercase">
      <ShieldCheck size={12} class="text-primary" />
      {$_("notes.careTitle")}
    </h3>
    <p class="leading-relaxed">{$_(`notes.${page}.care`)}</p>
  </div>
  <div class="rounded-xl border border-border bg-surface px-3.5 py-3">
    <h3 class="flex items-center gap-1.5 mb-2 text-[0.66rem] font-semibold tracking-[0.07em] text-text-muted uppercase">
      <FileCode2 size={12} class="text-primary" />
      {$_("notes.filesTitle")}
    </h3>
    {#each files[page] as file}
      <div class="my-1 rounded bg-surface-2 px-2 py-1.5 font-mono text-[0.7rem] break-all text-text-bright">{file}</div>
    {/each}
  </div>
</aside>
