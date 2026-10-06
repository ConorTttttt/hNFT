<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import {
    buildNetworkMarketplaceCards,
    filterMarketplaceCards,
    searchMarketplaceCards,
    sortMarketplaceCardsListedFirst,
    type MarketplaceRevealFilter,
  } from "$lib/networkMarketplaceCards";

  /**
   * Phase J.5 — a dedicated, bounded Network Marketplace view. Composed
   * entirely client-side from J.3 collection discovery + each collection's
   * own authoritative `get_all_nfts`/`all_listings` (see
   * `discoverNetworkMarketplace`'s doc) — there is no global NFT/listing
   * index anywhere underneath this. This is explicitly NOT "all NFTs on the
   * network": it is a bounded, progressively-loaded sample across up to
   * `maxCollections` discovered collections, and says so in its own copy.
   */
  let { onopencollection }: { onopencollection: (cloneId: string) => void } = $props();

  const app = useAppState();

  let reveal = $state<MarketplaceRevealFilter>("all");
  let collectionFilter = $state<string | null>(null);
  let forSaleOnly = $state(false);
  let listedFirst = $state(false);
  let query = $state("");

  $effect(() => {
    if (!app.networkMarketplaceLoaded) void app.loadNetworkMarketplace();
  });

  const cards = $derived(buildNetworkMarketplaceCards(app.networkMarketplaceItems));
  const collections = $derived(
    [...new Map(cards.map((c) => [c.collectionCloneId, c.collectionName])).entries()].sort((a, b) =>
      a[1].localeCompare(b[1]),
    ),
  );
  const filtered = $derived(
    filterMarketplaceCards(cards, { reveal, collectionCloneId: collectionFilter, forSaleOnly }),
  );
  const searched = $derived(searchMarketplaceCards(filtered, query));
  const visible = $derived(listedFirst ? sortMarketplaceCardsListedFirst(searched) : searched);

  const loadedCollectionCount = $derived(app.networkMarketplaceLoadedCollectionCount);
  const unresolvedCount = $derived(app.networkMarketplaceUnresolved.length);

  async function refresh() {
    await app.loadNetworkMarketplace();
  }
  async function loadMore() {
    await app.loadNetworkMarketplace({ more: true });
  }
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="panel stack tight">
    <div class="row between wrap">
      <h2 style="margin: 0">Network Marketplace</h2>
      <button class="secondary tiny" onclick={refresh} disabled={app.networkMarketplaceLoading}>
        {app.networkMarketplaceLoading ? "Loading…" : "Refresh"}
      </button>
    </div>
    <p class="faint small" style="margin: 0">
      Items discovered across known network collections — a bounded sample, not a complete
      index of every NFT on the network. Prices/listings come straight from each collection's
      own authoritative listing state, re-checked on every load.
    </p>
  </div>

  <div class="row wrap" style="gap: 0.6rem">
    <input
      type="search"
      placeholder="Search title or NFT ID…"
      bind:value={query}
      aria-label="Search network items"
      style="max-width: 280px"
    />
    <div class="nav">
      {#each ["all", "unrevealed", "revealed"] as const as r}
        <button class:active={reveal === r} onclick={() => (reveal = r)}>
          {r === "all" ? "All" : r === "unrevealed" ? "Unrevealed" : "Revealed"}
        </button>
      {/each}
    </div>
    <select bind:value={collectionFilter} aria-label="Filter by collection" style="width: auto">
      <option value={null}>All collections</option>
      {#each collections as [cloneId, name] (cloneId)}
        <option value={cloneId}>{name}</option>
      {/each}
    </select>
    <label class="row" style="gap: 0.4rem; align-items: center">
      <input type="checkbox" style="width: auto" bind:checked={forSaleOnly} />
      <span class="small">For sale only</span>
    </label>
    <label class="row" style="gap: 0.4rem; align-items: center">
      <input type="checkbox" style="width: auto" bind:checked={listedFirst} />
      <span class="small">Listed first</span>
    </label>
  </div>

  <p class="faint small" style="margin: 0">
    Showing {visible.length} item{visible.length === 1 ? "" : "s"} from {loadedCollectionCount}
    collection{loadedCollectionCount === 1 ? "" : "s"}.
    {#if unresolvedCount > 0}
      {unresolvedCount} collection{unresolvedCount === 1 ? "" : "s"} could not be resolved.
    {/if}
    {#if app.networkMarketplaceMoreAvailable}
      More discovered collections have not been loaded yet.
    {/if}
  </p>

  {#if app.networkMarketplaceMoreAvailable}
    <div>
      <button class="ghost tiny" onclick={loadMore} disabled={app.networkMarketplaceLoading}>
        {app.networkMarketplaceLoading ? "Loading…" : "Load more collections"}
      </button>
    </div>
  {/if}

  {#if app.networkMarketplaceLoading && !cards.length}
    <div class="grid">
      {#each Array(3) as _, i (i)}
        <div class="skeleton" style="aspect-ratio: 0.8"></div>
      {/each}
    </div>
  {:else if visible.length}
    <div class="grid">
      {#each visible as card (card.key)}
        <div class="panel stack tight">
          <strong class="truncate small">{card.title}</strong>
          <span class="faint small">{card.collectionName}</span>
          <div class="row wrap" style="gap: 0.35rem">
            <span class="pill" class:warn={card.sealed}>{card.sealed ? "Sealed" : "Revealed"}</span>
            {#if card.listed}
              <span class="pill cyan">{card.price} {card.currency}</span>
            {/if}
          </div>
          <button class="ghost tiny" onclick={() => onopencollection(card.collectionCloneId)}>
            Open collection
          </button>
        </div>
      {/each}
    </div>
  {:else if query.trim() || forSaleOnly || reveal !== "all" || collectionFilter}
    <div class="empty small">No network items match the current filters.</div>
  {:else}
    <div class="empty-hero">
      <strong>No network items resolved yet</strong>
      <span class="muted small">
        This needs at least one other collection published to the network (see a collection's
        Overview page to publish yours).
      </span>
    </div>
  {/if}
</div>
