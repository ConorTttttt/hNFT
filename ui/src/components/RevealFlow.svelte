<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { getAppStateContext } from "$lib/context";

  let { nft, hasSalt }: { nft: ActionHash; hasSalt: boolean } = $props();

  const app = getAppStateContext();

  let busy = $state(false);
  let err = $state("");

  async function reveal() {
    busy = true;
    err = "";
    try {
      // First (and only) reveal branches off the mint, whose action hash IS `nft`.
      await app.reveal(nft, nft);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if hasSalt}
  <div class="row">
    <button class="primary" onclick={reveal} disabled={busy}>
      {busy ? "Revealing…" : "Reveal metadata"}
    </button>
    <span class="muted">Uses the salt saved on this device at mint time.</span>
  </div>
{:else}
  <p class="error">
    The reveal salt for this NFT is not stored on this device. Without it the
    metadata can never be revealed. If you minted elsewhere, reveal from that
    device.
  </p>
{/if}
{#if err}<p class="error">{err}</p>{/if}
