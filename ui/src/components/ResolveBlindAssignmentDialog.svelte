<script lang="ts">
  import type { ActionHash, AgentPubKey } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, fromHex, shortHash, toHex } from "$lib/format";

  let { open = $bindable(false), purchase }: { open: boolean; purchase: ActionHash } = $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let busy = $state(false);
  let fulfilled = $state(true);

  // Resolved from the purchase's own record — `BlindPurchaseCommitment`'s
  // own author IS the authoritative buyer (see blind_collection.rs's
  // struct doc), so there's no reason to ask the minter to paste a key by
  // hand; that would only add a place for the demo's trust gap to widen
  // via typo.
  let batch = $state<ActionHash | null>(null);
  let buyer = $state<AgentPubKey | null>(null);
  let loadError = $state<string | null>(null);

  $effect(() => {
    if (!open) {
      dlg?.close();
      return;
    }
    dlg?.showModal();
    batch = null;
    buyer = null;
    loadError = null;
    void app.blindPurchaseInfo(purchase).then((info) => {
      if (!info) {
        loadError = "Could not find this purchase on-chain.";
        return;
      }
      batch = info.batch;
      buyer = info.buyer;
    });
  });

  async function submit() {
    if (!batch || !buyer) return;
    // See MintBlindBatchDialog's storage caveat — this lookup fails if
    // it's a different browser session than the one that minted the
    // batch, which is a real gap for anyone but a single demo operator
    // holding both roles in one browser.
    const seedHex = sessionStorage.getItem(`blind-seed:${toHex(batch)}`);
    if (!seedHex) {
      loadError = "No stored mint_seed for this batch in this browser session.";
      return;
    }
    busy = true;
    try {
      const mintSeed = fromHex(seedHex);
      const commitment = await app.resolveBlindAssignment(purchase, mintSeed, buyer, fulfilled);
      if (commitment) open = false;
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(440px, 88vw)">
    <h3>Resolve blind purchase <span class="pill warn">demo</span></h3>
    <p class="faint small">
      Demo mode — no real Unyt settlement is checked here, same as the
      ordinary-sale flow. Toggling <strong>fulfilled</strong> off is
      rejected by <code>resolve_blind_assignment_from_rave</code>, the same
      way a real unfulfilled RAVE would be.
    </p>

    {#if loadError}
      <p class="small" style="color: var(--danger, #c33)">{loadError}</p>
    {:else if !buyer}
      <p class="faint small">Loading purchase…</p>
    {:else}
      <dl class="defs">
        <dt>buyer</dt>
        <dd class="mono small truncate">{b64(buyer)} ({shortHash(buyer)})</dd>
        <dt>nft_id</dt>
        <dd class="mono small truncate">{b64(purchase)}</dd>
      </dl>
    {/if}

    <label class="row" style="gap: 0.5rem">
      <input type="checkbox" bind:checked={fulfilled} style="width: auto" />
      <span class="small">Settlement fulfilled</span>
    </label>

    <div class="row between">
      <button class="ghost" onclick={() => (open = false)} disabled={busy}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy || !buyer}>
        {busy ? "Resolving…" : "Resolve & send offer"}
      </button>
    </div>
  </div>
</dialog>
