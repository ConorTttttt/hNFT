<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { unb64 } from "$lib/format";

  let { nft, onclose }: { nft: ActionHash; onclose: () => void } = $props();

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let recipient = $state("");
  let busy = $state(false);

  $effect(() => {
    dlg?.showModal();
  });

  async function submit() {
    busy = true;
    try {
      await app.initiateTransfer(nft, unb64(recipient.trim()));
      dlg?.close();
    } catch {
      // toast already shown by app.initiateTransfer
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={onclose}>
  <div class="stack" style="min-width: min(400px, 88vw)">
    <h3>Transfer</h3>
    <label class="field">
      <span>Recipient agent key (base64) — copy it from the other window's header</span>
      <input class="mono" bind:value={recipient} placeholder="uhCAk…" />
    </label>
    <div class="row between">
      <button class="ghost" onclick={() => dlg?.close()}>Cancel</button>
      <button
        class="primary"
        onclick={submit}
        disabled={busy || !recipient.trim()}
      >
        {busy ? "Sending…" : "Send offer"}
      </button>
    </div>
  </div>
</dialog>
