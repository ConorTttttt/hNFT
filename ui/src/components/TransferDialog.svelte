<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { unb64 } from "$lib/format";

  let { nft, onclose }: { nft: ActionHash; onclose: () => void } = $props();

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let recipient = $state("");
  let busy = $state(false);
  let err = $state("");
  let result = $state("");

  $effect(() => {
    dlg?.showModal();
  });

  async function submit() {
    busy = true;
    err = "";
    result = "";
    try {
      const to = unb64(recipient.trim());
      await app.initiateTransfer(nft, to);
      result =
        "Offer sent. It stays open for the collection's transfer window; the buyer accepts from their inbox.";
      recipient = "";
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={onclose}>
  <div class="stack">
    <h3 style="margin: 0">Transfer</h3>
    <label class="stack" style="gap: 0.25rem">
      <span class="muted">Recipient AgentPubKey (base64)</span>
      <input bind:value={recipient} placeholder="uhCAk…" class="mono" />
    </label>
    <div class="row">
      <button class="primary" onclick={submit} disabled={busy || !recipient.trim()}>
        {busy ? "Sending…" : "Send offer"}
      </button>
      <button class="ghost" onclick={() => dlg?.close()}>Close</button>
    </div>
    {#if err}<p class="error">{err}</p>{/if}
    {#if result}<p class="ok">{result}</p>{/if}
  </div>
</dialog>
