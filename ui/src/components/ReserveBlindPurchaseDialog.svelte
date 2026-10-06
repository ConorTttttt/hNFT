<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";

  let {
    open = $bindable(false),
    batch,
    onreserved,
  }: {
    open: boolean;
    batch: ActionHash;
    /** Fires once the purchase is on-chain, so the caller can chain into
     * the existing Unyt payment dialog with `nftId = purchase hash`. */
    onreserved?: (purchase: ActionHash) => void;
  } = $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let busy = $state(false);

  $effect(() => {
    if (open) dlg?.showModal();
    else dlg?.close();
  });

  async function submit() {
    busy = true;
    try {
      const out = await app.reserveBlindPurchase(batch);
      if (out) {
        open = false;
        onreserved?.(out);
      }
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(420px, 88vw)">
    <h3>Buy blind</h3>
    <p class="faint small">
      You won't know which item you receive until the batch is closed and its
      randomness is fixed. The minter chooses which purchases are included, so
      an omitted purchase is not assigned anything; payment and refunds are not
      handled by this step.
    </p>
    <div class="row between">
      <button class="ghost" onclick={() => (open = false)} disabled={busy}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy}>
        {busy ? "Reserving…" : "Reserve purchase"}
      </button>
    </div>
  </div>
</dialog>
