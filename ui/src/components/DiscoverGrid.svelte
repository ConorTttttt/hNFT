<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import {
    filterNfts,
    NFT_FILTER_LABEL,
    NFT_SORT_LABEL,
    searchNfts,
    sortNfts,
    type NftFilter,
    type NftSort,
  } from "$lib/nftFiltering";
  import NftCard from "./NftCard.svelte";

  const app = useAppState();

  let forSaleOnly = $state(false);
  let filter = $state<NftFilter>("all");
  let query = $state("");
  let sort = $state<NftSort>("default");

  // One collection-wide `getAllNfts()` + `allListings()` load, shared with the
  // Overview and every NftCard (no per-card listing query). Search itself
  // never triggers a Holochain call — it's pure in-memory filtering below.
  $effect(() => {
    void app.ensureMarketData();
  });

  const forSaleCount = $derived(app.listingsLoaded ? app.listings.length : null);
  const notHidden = $derived(app.discover.filter((v) => !v.hidden));
  // Filters, then the "for sale" checkbox, then the search query — an
  // intersection, not a fallback to a wider match. See nftFiltering.ts.
  const filtered = $derived(
    filterNfts(notHidden, filter, app.me).filter(
      (v) => !forSaleOnly || app.listingByNft.has(b64(v.nft)),
    ),
  );
  const searched = $derived(searchNfts(filtered, query));
  // Sorting only reorders `searched` — it never widens the result set.
  const listedIds = $derived(new Set(app.listingByNft.keys()));
  const visible = $derived(sortNfts(searched, sort, listedIds));
  const searching = $derived(query.trim() !== "");
  const hidden = $derived(app.discover.filter((v) => v.hidden));
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="row between wrap">
    <h3>
      Items {#if notHidden.length}<span class="faint">· {visible.length} {searching || forSaleOnly || filter !== "all" ? "results" : "items"}</span>{/if}
    </h3>
    <label class="row" style="gap: 0.4rem; align-items: center">
      <input type="checkbox" style="width: auto" bind:checked={forSaleOnly} />
      <span class="small">
        For sale only{#if forSaleCount !== null}<span class="faint"> · {forSaleCount}</span>{/if}
      </span>
    </label>
    <span class="faint small">Every NFT minted in this collection</span>
  </div>

  {#if forSaleOnly}
    <p class="faint small" style="margin: 0">
      Prices are asking prices. Open an item and express interest to let the seller
      know you want it — the seller can then choose to send you a transfer offer.
    </p>
  {/if}

  <div class="row wrap" style="gap: 0.6rem">
    <input
      type="search"
      placeholder="Search this collection's items by title or ID…"
      bind:value={query}
      aria-label="Search items"
      style="max-width: 320px"
    />
    <div class="nav">
      {#each ["all", "mine", "unrevealed", "revealed"] as const as f}
        <button class:active={filter === f} onclick={() => (filter = f)}>
          {NFT_FILTER_LABEL[f]}
        </button>
      {/each}
    </div>
    <label class="row" style="gap: 0.4rem; align-items: center">
      <span class="small">Sort</span>
      <select bind:value={sort} aria-label="Sort items" style="width: auto">
        {#each ["default", "listed-first"] as const as s}
          <option value={s}>{NFT_SORT_LABEL[s]}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if visible.length}
    <div class="grid">
      {#each visible as view (b64(view.nft))}
        <NftCard
          {view}
          onopen={(nft) => app.openDetail(nft)}
          onhide={(nft) => app.hideNft(nft)}
        />
      {/each}
    </div>
  {:else if app.discoverLoading}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.8"></div>
      {/each}
    </div>
  {:else if app.discover.length === 0}
    <div class="empty-hero">
      <strong>Nothing minted yet</strong>
      <span class="muted small">
        Once someone mints into this collection, it shows up here.
      </span>
    </div>
  {:else if notHidden.length === 0}
    <div class="empty small">Every NFT here is hidden.</div>
  {:else if searching}
    <div class="empty small">
      No items match "{query.trim()}".
      <button class="ghost tiny" onclick={() => (query = "")}>Clear search</button>
    </div>
  {:else}
    <div class="empty small">No items match the current filter.</div>
  {/if}

  {#if hidden.length}
    <details class="hidden-section">
      <summary>Hidden · {hidden.length}</summary>
      <div class="grid" style="margin-top: 0.75rem">
        {#each hidden as view (b64(view.nft))}
          <NftCard
            {view}
            onopen={(nft) => app.openDetail(nft)}
            onunhide={(nft) => app.unhideNft(nft)}
          />
        {/each}
      </div>
    </details>
  {/if}
</div>

<style>
  .hidden-section summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.85rem;
  }
</style>
