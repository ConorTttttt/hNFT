<script lang="ts">
  import { mintPolicyOf } from "@p2p-art-marketplace/client";
  import { NFT_ROLE, useAppState } from "$lib/appState.svelte";
  import { collectionKind, COLLECTION_KIND_LABEL, MINT_POLICY_LABEL } from "$lib/collectionCards";
  import { supplyStats } from "$lib/collectionStats";
  import { b64 } from "$lib/format";

  /**
   * The active collection at a glance. Every figure comes from an existing
   * authoritative source for THIS bound collection only:
   *   - identity / max supply / mint policy: `collection_info`
   *   - minted: `getAllNfts()` (loaded once via `ensureMarketData`)
   *   - for sale: `allListings()` (loaded once, shared with the Items grid)
   *   - "You can mint" / allowance / progress: `mint_status` (Phase J.2) — an
   *     AGENT-specific picture, not a collection-wide minting phase (no such
   *     flag exists).
   * No floor price, volume, owner count or ranking is shown: the application
   * doesn't track any of them.
   */
  let { onitems }: { onitems: () => void } = $props();

  const app = useAppState();

  $effect(() => {
    void app.ensureMarketData();
  });

  // Phase J.3: cheap, idempotent — lets "Publish to network"/"Published"
  // resolve correctly even for a session that never visited the browser.
  $effect(() => {
    if (!app.discoveredLoaded) void app.loadDiscoveredCollections();
  });

  const info = $derived(app.collection);
  const isClone = $derived(app.nftRoleName !== NFT_ROLE);
  const myDnaHashB64 = $derived(app.localCloneDnaHashes.get(app.nftRoleName) ?? null);
  const alreadyPublished = $derived(
    myDnaHashB64 != null &&
      app.discoveredCollections.some((v) => b64(v.dna_hash) === myDnaHashB64),
  );
  let publishing = $state(false);
  async function publish() {
    publishing = true;
    try {
      await app.publishActiveCollection();
    } finally {
      publishing = false;
    }
  }
  const kind = $derived(collectionKind(info));
  const supply = $derived(
    supplyStats(info?.max_supply, app.discoverLoaded ? app.discover.length : null),
  );
  const forSale = $derived(app.listingsLoaded ? app.listings.length : null);
  const policy = $derived(mintPolicyOf(info));
  const status = $derived(app.mintStatus);
  const remaining = $derived(
    status?.my_allowance != null ? Math.max(0, status.my_allowance - status.minted_by_me) : null,
  );
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="panel stack">
    <div class="row between wrap" style="gap: 0.75rem">
      <div class="stack tight">
        <h2 style="margin: 0">{info?.name ?? "Collection"}</h2>
        <div class="row wrap" style="gap: 0.4rem">
          {#if info?.symbol}<span class="pill">{info.symbol}</span>{/if}
          <span class="pill" class:cyan={kind === "generative"} class:warn={kind === "blind"}>
            {COLLECTION_KIND_LABEL[kind]}
          </span>
          <span class="pill">{MINT_POLICY_LABEL[policy]}</span>
          {#if app.isMinter}<span class="pill cyan">You can mint</span>{/if}
        </div>
      </div>
    </div>
    {#if policy === "creator_only"}
      <p class="faint small" style="margin: 0">
        Only this collection's configured minter can mint here.
      </p>
    {:else if policy === "whitelist"}
      <p class="faint small" style="margin: 0">
        Only whitelisted agents can mint here, each up to their own allowance.
      </p>
    {:else}
      <p class="faint small" style="margin: 0">
        Anyone can mint here{supply.unlimited ? "" : " (development configuration — no supply cap is enforced without a single minter or whitelist)"}.
      </p>
    {/if}

    {#if policy === "whitelist"}
      <div class="panel stack tight" style="margin-top: 0.25rem">
        <span class="small"><strong>Your minting status</strong></span>
        {#if status?.my_allowance != null}
          <div class="row wrap" style="gap: 1.25rem">
            <span class="small">Your allowance: <strong>{status.my_allowance}</strong></span>
            <span class="small">Minted: <strong>{status.minted_by_me}</strong></span>
            <span class="small">Remaining: <strong>{remaining}</strong></span>
          </div>
          {#if !status.eligible}
            <p class="error small" style="margin: 0">Your mint allowance has been reached.</p>
          {/if}
        {:else}
          <p class="faint small" style="margin: 0">You are not on the whitelist.</p>
        {/if}
      </div>
    {:else if policy === "creator_only" && !app.isMinter}
      <p class="faint small" style="margin: 0.25rem 0 0">
        You are not this collection's configured minter.
      </p>
    {/if}
  </div>

  <div class="stats">
    <div class="panel stat">
      <span class="faint small">Max supply</span>
      <strong>
        {#if supply.unlimited}Unlimited{:else if supply.max !== null}{supply.max}{:else}—{/if}
      </strong>
    </div>
    <div class="panel stat">
      <span class="faint small">Minted</span>
      <strong>{supply.minted ?? (app.discoverLoading ? "…" : "—")}</strong>
    </div>
    <div class="panel stat">
      <span class="faint small">Remaining</span>
      <strong>
        {#if supply.unlimited}Unlimited{:else if supply.remaining !== null}{supply.remaining}{:else}{app.discoverLoading ? "…" : "—"}{/if}
      </strong>
    </div>
    <div class="panel stat">
      <span class="faint small">For sale</span>
      <strong>{forSale ?? (app.listingsLoading ? "…" : "—")}</strong>
    </div>
  </div>

  {#if isClone}
    <div class="panel stack tight">
      <h4 style="margin: 0">Network discovery</h4>
      {#if alreadyPublished}
        <p class="faint small" style="margin: 0">
          Published — other peers can discover this collection on the network.
        </p>
      {:else}
        <p class="faint small" style="margin: 0">
          Not published yet. Publishing announces this collection so other peers can find and
          open it — it does not change anything about the collection itself.
        </p>
        <div>
          <button class="secondary" onclick={publish} disabled={publishing}>
            {publishing ? "Publishing…" : "Publish to network"}
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="panel stack tight">
    <h4 style="margin: 0">How buying works here</h4>
    <p class="faint small" style="margin: 0">
      Listed items show an asking price. Expressing interest lets the seller know you
      want an item; the seller can then choose to send you a transfer offer, which you
      accept from Activity. There is no instant purchase or checkout.
    </p>
    <div>
      <button class="secondary" onclick={onitems}>
        Browse items{forSale ? ` · ${forSale} for sale` : ""}
      </button>
    </div>
  </div>
</div>

<style>
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 0.9rem;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .stat strong {
    font-size: 1.4rem;
  }
</style>
