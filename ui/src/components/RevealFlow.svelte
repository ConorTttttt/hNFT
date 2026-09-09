<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";

  let { nft, hasSalt }: { nft: ActionHash; hasSalt: boolean } = $props();

  const app = useAppState();
  let busy = $state(false);

  async function reveal() {
    busy = true;
    try {
      // First (and only) reveal branches off the mint, whose action hash IS `nft`.
      await app.reveal(nft, nft);
    } finally {
      busy = false;
    }
  }
</script>

{#if hasSalt}
  <div class="row wrap" style="gap: 0.6rem">
    <button class="primary" onclick={reveal} disabled={busy}>
      {busy ? "Revealing…" : "Reveal metadata"}
    </button>
    <span class="faint small">Uses the salt saved on this device at mint time.</span>
  </div>
{:else}
  <p class="error small">
    The reveal salt for this NFT isn't on this device — the metadata can never be
    revealed here. Reveal from the device you minted on.
  </p>
{/if}
