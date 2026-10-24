<script lang="ts">
  import { Info } from "lucide-svelte";
  import { showNotes } from "../lib/ui";

  // A short explanation under a setting, only shown while the notes are on.
  export let text: string;
</script>

{#if $showNotes}
  <p class="flex items-start gap-1.5 text-[0.72rem] leading-snug text-text-muted mt-1">
    <Info size={12} class="mt-[1px] shrink-0 text-primary/80" />
    <span>{text}</span>
  </p>
{/if}
