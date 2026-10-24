<script lang="ts">
  // A switch with its label. The state is bound with bind:checked.
  export let checked = false;
  export let disabled = false;
  export let label: string;
  export let title = "";
</script>

<label class="flex items-center justify-between gap-3 cursor-pointer select-none {disabled ? 'opacity-50 cursor-not-allowed' : ''}" {title}>
  <span class="text-[0.84rem] text-text">{label}</span>
  <span class="relative shrink-0">
    <input type="checkbox" class="peer sr-only" bind:checked {disabled} />
    <span class="block w-10 h-[22px] rounded-full bg-surface-2 border border-border transition-colors peer-checked:bg-primary peer-checked:border-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary/60"></span>
    <span class="absolute top-[3px] left-[3px] w-4 h-4 rounded-full bg-text-bright shadow transition-transform peer-checked:translate-x-[18px]"></span>
  </span>
</label>
