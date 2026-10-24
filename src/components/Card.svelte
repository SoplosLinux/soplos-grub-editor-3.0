<script lang="ts">
  import type { ComponentType } from "svelte";

  // A titled block of a page: an icon, a title and, optionally, a short line
  // under it. The body goes in the slot.
  export let title: string;
  export let icon: ComponentType | null = null;
  export let subtitle = "";
</script>

<section class="rounded-xl border border-border bg-surface shadow-sm overflow-hidden">
  <header class="flex items-center gap-3 px-4 py-3 border-b border-border bg-gradient-to-r from-primary/10 via-transparent to-transparent">
    {#if icon}
      <span class="w-8 h-8 rounded-lg bg-primary/15 text-primary grid place-items-center shrink-0">
        <svelte:component this={icon} size={17} />
      </span>
    {/if}
    <div class="min-w-0">
      <h2 class="text-[0.9rem] font-semibold text-text-bright leading-tight">{title}</h2>
      {#if subtitle}
        <p class="text-[0.72rem] text-text-muted mt-0.5">{subtitle}</p>
      {/if}
    </div>
  </header>
  <div class="p-4">
    <slot />
  </div>
</section>
