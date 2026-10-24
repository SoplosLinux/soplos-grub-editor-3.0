<script lang="ts">
  import { _ } from "svelte-i18n";
  import { ChevronDown, ChevronUp, X } from "lucide-svelte";
  import { pending, applyPending, discardAll, release } from "../lib/pending";

  // The changes asked for so far, waiting to be made together.
  let open = false;
  $: if ($pending.length === 0) open = false;
</script>

{#if $pending.length > 0}
  <div class="shrink-0 border-t border-border bg-surface px-5 py-3 shadow-[0_-4px_14px_rgba(0,0,0,0.18)]">
    {#if open}
      <ul class="mb-3 flex max-h-[150px] flex-col gap-1 overflow-y-auto content-scroll overscroll-contain">
        {#each $pending as item (item.key)}
          <li class="flex items-center gap-2 rounded-lg bg-surface-2/60 px-3 py-1.5 text-[0.78rem] text-text">
            <span class="min-w-0 flex-1 break-words">{item.label}</span>
            <button class="shrink-0 rounded p-1 text-text-muted hover:bg-surface-2 hover:text-text-bright" title={$_("pending.remove")} aria-label={$_("pending.remove")} on:click={() => release(item.key)}>
              <X size={13} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="flex flex-wrap items-center gap-3">
      <p class="text-[0.85rem] font-semibold text-text-bright">{$_("pending.count", { values: { count: $pending.length } })}</p>
      <button class="flex items-center gap-1 text-[0.78rem] text-text-muted hover:text-text-bright" aria-expanded={open} on:click={() => (open = !open)}>
        {$_("pending.details")}
        {#if open}<ChevronDown size={13} />{:else}<ChevronUp size={13} />{/if}
      </button>
      <div class="ml-auto flex gap-2">
        <button class="px-4 py-2 rounded-lg border border-border text-text hover:bg-surface-2 text-[0.82rem]" on:click={discardAll}>{$_("pending.discard")}</button>
        <button class="px-5 py-2 rounded-lg bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold shadow" on:click={applyPending}>{$_("pending.apply")}</button>
      </div>
    </div>
  </div>
{/if}
