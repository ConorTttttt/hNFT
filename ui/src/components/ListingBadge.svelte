<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { ListingView } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";

  let { nft }: { nft: ActionHash } = $props();

  const app = useAppState();
  let listing = $state<ListingView | null>(null);

  $effect(() => {
    let cancelled = false;
    app.nft
      .listingForNft(nft)
      .then((v) => {
        if (!cancelled) listing = v;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
</script>

{#if listing}
  <span class="pill listed">for sale · {listing.price} {listing.currency}</span>
{/if}

<style>
  .listed {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in oklab, var(--accent) 35%, transparent);
  }
</style>
