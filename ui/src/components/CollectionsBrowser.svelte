<script lang="ts">
  import { MINT_POLICY_LABEL, sortCollectionCardsByRecent, type CollectionCardData } from "$lib/collectionCards";
  import { filterCollectionCards } from "$lib/collectionFiltering";
  import {
    searchNetworkCards,
    sortNetworkCardsByRecent,
    type NetworkCollectionCardData,
  } from "$lib/discoveredCollectionCards";
  import type { NetworkNftCardData } from "$lib/networkNftCards";
  import CollectionCard from "./CollectionCard.svelte";

  /**
   * The primary collection-discovery surface. Two independent sources, never
   * conflated (Phase J.3 report §8):
   *   - `cards`: this device's LOCAL registry (`collectionRegistry.ts`) — a
   *     cache of collections this device can already open.
   *   - `networkCards`: the AUTHORITATIVE network-wide discovery list
   *     (`social`'s `discover_nft_collections`) — collections ANY peer has
   *     published, whether or not this device has opened them yet.
   * A collection can appear in both; `networkCards[].availableLocally`
   * distinguishes "Discovered" from "Available locally" (§10) — opening a
   * not-yet-local one reconstructs its clone cell (`onopendiscovered`).
   *
   * The search box filters only whichever already-loaded list is showing —
   * it is not NFT search, owner search, or a live network query per keystroke.
   */
  let {
    cards,
    networkCards,
    networkLoading,
    networkNftCards,
    networkNftsLoading,
    networkNftsLoaded,
    networkNftsUnresolvedCount,
    onselect,
    oncreate,
    onopendiscovered,
    onloadnetworkitems,
  }: {
    cards: CollectionCardData[];
    networkCards: NetworkCollectionCardData[];
    networkLoading: boolean;
    /** Phase J.4: a bounded preview, not a full cross-collection feed — see
     * `discoverNetworkNfts`'s doc for the exact bounds. */
    networkNftCards: NetworkNftCardData[];
    networkNftsLoading: boolean;
    networkNftsLoaded: boolean;
    networkNftsUnresolvedCount: number;
    onselect: (cloneId: string | undefined) => void;
    oncreate: () => void;
    onopendiscovered: (card: NetworkCollectionCardData) => Promise<void>;
    onloadnetworkitems: () => Promise<void>;
  } = $props();

  let query = $state("");
  let opening = $state<string | null>(null);

  // "Recently added" = this device's own registration order, not on-chain
  // creation time — see `sortCollectionCardsByRecent`'s own doc comment.
  const ordered = $derived(sortCollectionCardsByRecent(cards));
  const visible = $derived(filterCollectionCards(ordered, query));

  // Only collections NOT already available locally get their own "Network"
  // card + Open action — a collection already open shows once, in the local
  // grid above, not duplicated here.
  const networkOnly = $derived(networkCards.filter((c) => !c.availableLocally));
  const networkOrdered = $derived(sortNetworkCardsByRecent(networkOnly));
  const networkVisible = $derived(searchNetworkCards(networkOrdered, query));

  async function open(card: NetworkCollectionCardData) {
    opening = card.dnaHashB64;
    try {
      await onopendiscovered(card);
    } finally {
      opening = null;
    }
  }
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="row between wrap">
    <h3>Collections {#if cards.length}<span class="faint">· {cards.length}</span>{/if}</h3>
  </div>

  {#if cards.length}
    <input
      type="search"
      placeholder="Search collections by name or symbol…"
      bind:value={query}
      aria-label="Search collections"
      style="max-width: 320px"
    />

    {#if visible.length}
      <div class="grid">
        {#each visible as card (card.cloneId ?? "__default__")}
          <CollectionCard {card} {onselect} />
        {/each}
      </div>
    {:else}
      <div class="empty small">
        No collections match "{query.trim()}".
        <button class="ghost tiny" onclick={() => (query = "")}>Clear search</button>
      </div>
    {/if}
  {:else}
    <div class="empty-hero">
      <strong>No collections saved on this device yet</strong>
      <span class="muted small">
        This device hasn't registered any collections. Create one to get started.
      </span>
      <div class="actions">
        <button class="primary" onclick={oncreate}>Create collection</button>
      </div>
    </div>
  {/if}

  <p class="faint small">
    Collections above are ones this device can already open.
  </p>

  <div class="divider"></div>

  <div class="row between wrap">
    <h3>
      Discovered on the network
      {#if networkVisible.length}<span class="faint">· {networkVisible.length}</span>{/if}
    </h3>
  </div>

  {#if networkLoading && !networkCards.length}
    <p class="faint small">Searching the network…</p>
  {:else if networkVisible.length}
    <div class="grid">
      {#each networkVisible as card (card.dnaHashB64)}
        <div class="panel stack tight network-card">
          <div class="row between">
            <strong class="truncate">{card.name}</strong>
            {#if card.symbol}<span class="pill">{card.symbol}</span>{/if}
          </div>
          <div class="row wrap" style="gap: 0.35rem">
            <span class="pill">{MINT_POLICY_LABEL[card.mintPolicy]}</span>
            <span class="pill accent">Discovered</span>
          </div>
          <span class="faint small">
            {card.maxSupply > 0 ? `Max supply ${card.maxSupply}` : "Unlimited supply"}
          </span>
          <button
            class="secondary tiny"
            onclick={() => open(card)}
            disabled={opening === card.dnaHashB64}
          >
            {opening === card.dnaHashB64 ? "Opening…" : "Open"}
          </button>
        </div>
      {/each}
    </div>
  {:else if query.trim()}
    <div class="empty small">No network collections match "{query.trim()}".</div>
  {:else}
    <p class="faint small">
      No other collections discovered on the network yet. Publish one of yours from its
      Overview page so other peers can find it.
    </p>
  {/if}

  <p class="faint small">
    "Discovered" collections are announced by their publisher — see the collection's own
    page for what that does and doesn't verify about who really created it. Opening one
    creates a local copy of its Holochain cell on this device.
  </p>

  <div class="divider"></div>

  <div class="row between wrap">
    <h3>Network items preview</h3>
    <button class="ghost tiny" onclick={onloadnetworkitems} disabled={networkNftsLoading}>
      {networkNftsLoading ? "Loading…" : networkNftsLoaded ? "Refresh" : "Load"}
    </button>
  </div>
  <p class="faint small" style="margin: 0">
    A bounded preview only — a few items from a handful of collections, not a full
    cross-network feed. Opens any needed collection cells on this device.
  </p>

  {#if networkNftsLoading && !networkNftCards.length}
    <p class="faint small">Resolving items across discovered collections…</p>
  {:else if networkNftCards.length}
    <div class="grid">
      {#each networkNftCards as card (card.key)}
        <div class="panel stack tight">
          <strong class="truncate small">{card.title}</strong>
          <span class="faint small">{card.collectionName}</span>
          <span class="pill" class:warn={card.sealed}>{card.sealed ? "Sealed" : "Revealed"}</span>
          <button class="ghost tiny" onclick={() => onselect(card.collectionCloneId)}>
            Open collection
          </button>
        </div>
      {/each}
    </div>
  {:else if networkNftsLoaded}
    <div class="empty small">No items resolved from the network yet.</div>
  {/if}

  {#if networkNftsUnresolvedCount > 0}
    <p class="faint small">
      {networkNftsUnresolvedCount} discovered collection{networkNftsUnresolvedCount === 1 ? "" : "s"}
      could not be resolved right now.
    </p>
  {/if}
</div>
