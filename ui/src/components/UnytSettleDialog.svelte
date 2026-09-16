<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, unb64 } from "$lib/format";

  let { nft, onclose }: { nft: ActionHash; onclose: () => void } = $props();

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let buyer = $state("");
  let fulfilled = $state(true);
  let busy = $state(false);

  $effect(() => {
    dlg?.showModal();
  });

  async function submit() {
    busy = true;
    try {
      await app.settleFromRave(nft, unb64(buyer.trim()), fulfilled);
      dlg?.close();
    } catch {
      // toast already shown by app.settleFromRave
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={onclose}>
  <div class="stack" style="min-width: min(440px, 88vw)">
    <h3>Settle via Unyt <span class="pill warn">demo</span></h3>
    <p class="faint small">
      This app doesn't talk to a live Unyt network — it's a stand-in for "the
      buyer already paid through an <code>nft_sale_escrow</code> Smart
      Agreement, and the UI read the resulting RAVE." Toggling
      <strong>fulfilled</strong> off shows the transfer being rejected the
      same way a real unfulfilled settlement would be.
    </p>
    <label class="field">
      <span>Buyer's agent key (base64) — copy it from the other window's header</span>
      <input class="mono" bind:value={buyer} placeholder="uhCAk…" />
    </label>
    <label class="row" style="gap: 0.5rem">
      <input type="checkbox" bind:checked={fulfilled} style="width: auto" />
      <span class="small">Settlement fulfilled</span>
    </label>
    <dl class="defs">
      <dt>nft_id</dt>
      <dd class="mono small truncate">{b64(nft)}</dd>
      <dt>previous_owner</dt>
      <dd class="mono small truncate">{b64(app.me)}</dd>
      <dt>new_owner</dt>
      <dd class="mono small truncate">{buyer.trim() ? buyer.trim() : "—"}</dd>
    </dl>
    <div class="row between">
      <button class="ghost" onclick={() => dlg?.close()}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy || !buyer.trim()}>
        {busy ? "Settling…" : "Settle & send offer"}
      </button>
    </div>
  </div>
</dialog>
