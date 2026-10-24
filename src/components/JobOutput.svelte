<script lang="ts">
  import { afterUpdate } from "svelte";
  import { _ } from "svelte-i18n";
  import Modal from "./Modal.svelte";
  import { applyState, closeApply } from "../lib/pending";

  // The panel shown while the pending changes are being made: the live output
  // of the helper and a bar that moves so nobody thinks the program froze.
  // update-grub reports no percentage, so the bar has no end until it is over.

  let log: HTMLPreElement | undefined;

  // The helper's own bookkeeping lines (@@...) are not for people.
  $: text = $applyState.output
    .split("\n")
    .filter((line) => line.trim() && !line.startsWith("@@"))
    .join("\n");
  $: phase = $applyState.phase;

  afterUpdate(() => {
    if (log) log.scrollTop = log.scrollHeight;
  });

  const close = () => {
    if (phase !== "running") closeApply();
  };
</script>

{#if phase !== "idle"}
  <Modal
    variant={phase === "failed" ? "danger" : "info"}
    title={phase === "running" ? $_("job.runningTitle") : phase === "done" ? $_("job.doneTitle") : $_("job.failedTitle")}
    showFooter={false}
    maxWidth={620}
    onClose={close}
  >
    <p>{phase === "running" ? $_("job.running") : phase === "done" ? $_("job.done") : $_("job.failed")}</p>

    <div class="relative mt-3 h-2 overflow-hidden rounded-full bg-surface-2" role="progressbar" aria-busy={phase === "running"}>
      {#if phase === "running"}
        <div class="progress-indeterminate absolute inset-y-0 left-0 w-2/5 rounded-full bg-primary"></div>
      {:else}
        <div class="absolute inset-0 rounded-full {phase === 'done' ? 'bg-success' : 'bg-danger'}"></div>
      {/if}
    </div>

    <pre bind:this={log} class="mt-3 h-[220px] overflow-auto content-scroll rounded border border-border bg-panel p-2 font-mono text-[0.72rem] text-text whitespace-pre-wrap break-words">{text || $_("job.waiting")}</pre>

    <div class="mt-4 flex justify-end">
      <button class="px-5 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.85rem] font-semibold disabled:opacity-50" disabled={phase === "running"} on:click={close}>
        {$_("common.close")}
      </button>
    </div>
  </Modal>
{/if}
