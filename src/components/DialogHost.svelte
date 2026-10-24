<script lang="ts">
  import { _ } from "svelte-i18n";
  import Modal from "./Modal.svelte";
  import { dialog } from "../lib/dialogs";

  // Draws the dialog that lib/dialogs.ts is asking for.
  let saving = false;
  let backupError = "";

  async function saveBackup() {
    if (!$dialog?.onSave || saving) return;
    saving = true;
    backupError = "";
    const result = await $dialog.onSave();
    saving = false;
    if (result.ok) $dialog?.resolve(true);
    else backupError = result.error ?? "";
  }

  $: current = $dialog;
  $: if (!current) backupError = "";
</script>

{#if current}
  {#if current.kind === "backup"}
    <Modal variant="confirm" title={current.title} showFooter={false} maxWidth={480} onClose={() => current?.resolve(false)}>
      <p>{current.body}</p>
      {#if backupError}
        <p class="mt-2 text-danger text-[0.78rem] break-all">{backupError}</p>
      {/if}
      <div class="mt-4 flex flex-wrap justify-end gap-2">
        <button class="px-4 py-2 rounded border border-border text-text hover:bg-surface-2 text-[0.82rem]" on:click={() => current?.resolve(false)}>
          {$_("common.cancel")}
        </button>
        <button class="px-4 py-2 rounded border border-border text-text hover:bg-surface-2 text-[0.82rem]" on:click={() => current?.resolve(true)}>
          {$_("backup.continueWithout")}
        </button>
        <button class="px-4 py-2 rounded bg-primary hover:bg-primary-hover text-text-bright text-[0.82rem] font-semibold disabled:opacity-60" disabled={saving} on:click={saveBackup}>
          {saving ? $_("backup.saving") : $_("backup.save")}
        </button>
      </div>
    </Modal>
  {:else}
    <Modal
      variant={current.kind === "danger" ? "danger" : current.kind === "confirm" ? "confirm" : "info"}
      title={current.title}
      confirmLabel={current.confirmLabel}
      maxWidth={current.details ? 560 : 440}
      onClose={() => current?.resolve(false)}
      onConfirm={() => current?.resolve(true)}
    >
      <p>{current.body}</p>
      {#if current.details}
        <pre class="mt-3 max-h-[200px] overflow-auto content-scroll rounded border border-border bg-panel p-2 text-[0.72rem] text-text whitespace-pre-wrap break-words">{current.details}</pre>
      {/if}
    </Modal>
  {/if}
{/if}
