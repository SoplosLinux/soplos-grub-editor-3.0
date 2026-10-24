<script lang="ts">
  import { _ } from "svelte-i18n";
  import { X } from "lucide-svelte";
  import { applyState, closeApply } from "../lib/pending";

  // Strip above the progress bar with the job that applies the pending
  // changes, its latest output line and the full log on demand. Same layout
  // as the Store's, so the programs behave alike.
  let expanded = false;

  $: phase = $applyState.phase;
  // The helper's own bookkeeping lines (@@...) are not for people.
  $: lines = $applyState.output.split("\n").filter((line) => line.trim() && !line.startsWith("@@"));
  $: log = lines.join("\n");
  $: lastLine = phase === "failed" ? $_("job.failed") : phase === "done" ? $_("job.done") : (lines[lines.length - 1] ?? $_("job.running"));

  // A failed run opens the log by itself, since that is what is needed then.
  $: if (phase === "failed") expanded = true;
  $: if (phase === "idle") expanded = false;

  // Keeps the log scrolled to the newest line, done as an action on the
  // element itself (see the Store's JobBar for why).
  function stickToBottom(node: HTMLElement, _lineCount: number) {
    const scroll = () => (node.scrollTop = node.scrollHeight);
    scroll();
    return { update: scroll };
  }

  const STATUS_CLASS = { running: "text-primary", done: "text-success", failed: "text-danger", idle: "" };
</script>

{#if phase !== "idle"}
  <div class="border-t border-border bg-panel shrink-0">
    <div class="flex items-center gap-3 px-4 py-2 text-[0.78rem]">
      <span class="font-semibold text-text-bright shrink-0">{$_("job.runningTitle")}</span>
      {#if phase !== "running"}
        <span class="shrink-0 {STATUS_CLASS[phase]}">{phase === "done" ? $_("job.doneTitle") : $_("job.failedTitle")}</span>
      {/if}
      <span class="flex-1 min-w-0 truncate text-text-muted font-mono text-[0.7rem]">{lastLine}</span>
      <button class="text-primary hover:underline shrink-0" on:click={() => (expanded = !expanded)}>
        {expanded ? $_("job.hideLog") : $_("job.showLog")}
      </button>
      {#if phase !== "running"}
        <button class="p-1 rounded text-text-muted hover:bg-surface hover:text-text-bright shrink-0" title={$_("job.close")} aria-label={$_("job.close")} on:click={closeApply}>
          <X size={14} />
        </button>
      {/if}
    </div>
    {#if expanded}
      <pre
        use:stickToBottom={log.length}
        class="content-scroll h-[200px] overflow-auto mx-4 mb-2 p-2.5 rounded bg-dark border border-border text-[0.7rem] text-text font-mono whitespace-pre-wrap">{log || $_("job.waiting")}</pre>
    {/if}
  </div>
{/if}
