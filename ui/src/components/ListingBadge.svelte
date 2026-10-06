<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { ListingView } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { formatListingPrice } from "$lib/collectionStats";

  /**
   * `listing`: pass the already-loaded listing (or `null` = known unlisted) to
   * avoid a `listingForNft` query per card. Omit it (`undefined`) and the badge
   * looks the listing up itself, as before.
   */
  let { nft, listing: known }: { nft: ActionHash; listing?: ListingView | null } = $props();

  const app = useAppState();
  let fetched = $state<ListingView | null>(null);

  $effect(() => {
    if (known !== undefined) return;
    let cancelled = false;
    app.nft
      .listingForNft(nft)
      .then((v) => {
        if (!cancelled) fetched = v;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const listing = $derived(known !== undefined ? known : fetched);
</script>

{#if listing}
  <span class="pill listed" title="Asking price — express interest to contact the seller">
    for sale · {formatListingPrice(listing)}
  </span>
{/if}

<style>
  .listed {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in oklab, var(--accent) 35%, transparent);
  }
</style>
