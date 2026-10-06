<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { ListingView } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";

  let { nft, iOwn }: { nft: ActionHash; iOwn: boolean } = $props();

  const app = useAppState();

  let listing = $state<ListingView | null>(null);
  let loaded = $state(false);

  async function load() {
    listing = await app.nft.listingForNft(nft).catch(() => null);
    loaded = true;
  }

  $effect(() => {
    void nft;
    loaded = false;
    void load();
  });

  let price = $state("");
  let currency = $state("HOT");
  let busy = $state(false);
  let editing = $state(false);

  async function list() {
    const p = Number(price);
    if (!p || p <= 0) return;
    busy = true;
    try {
      await app.listItem(nft, p, currency.trim() || "HOT");
      await load();
      price = "";
    } catch {
      // toasted by listItem
    } finally {
      busy = false;
    }
  }

  function startEdit() {
    if (!listing) return;
    price = String(listing.price);
    currency = listing.currency;
    editing = true;
  }

  async function saveEdit() {
    if (!listing) return;
    const p = Number(price);
    if (!p || p <= 0) return;
    busy = true;
    try {
      await app.updateListingPrice(listing.listing, p, currency.trim() || "HOT", nft);
      await load();
      editing = false;
    } catch {
      // toasted by updateListingPrice
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    if (!listing) return;
    busy = true;
    try {
      await app.cancelListing(listing.listing, nft);
      await load();
    } finally {
      busy = false;
    }
  }

  let interestBusy = $state(false);
  async function interested() {
    if (!listing) return;
    interestBusy = true;
    try {
      await app.expressInterest(listing.listing);
    } finally {
      interestBusy = false;
    }
  }
</script>

{#if loaded}
  <div class="stack tight">
    <h4>Sale</h4>

    {#if listing && iOwn}
      {#if editing}
        <div class="row wrap" style="gap: 0.5rem; align-items: center">
          <input class="mono" style="max-width: 8rem" bind:value={price} placeholder="price" disabled={busy} />
          <input class="mono" style="max-width: 6rem" bind:value={currency} placeholder="currency" disabled={busy} />
          <button class="primary tiny" onclick={saveEdit} disabled={busy}>Save</button>
          <button class="ghost tiny" onclick={() => (editing = false)}>Cancel</button>
        </div>
      {:else}
        <div class="row wrap" style="gap: 0.5rem; align-items: center">
          <span class="pill listed">for sale · {listing.price} {listing.currency}</span>
          <button class="ghost tiny" onclick={startEdit}>Edit price</button>
          <button class="ghost tiny" onclick={cancel} disabled={busy}>Cancel listing</button>
        </div>
      {/if}
    {:else if listing}
      <div class="row wrap" style="gap: 0.5rem; align-items: center">
        <span class="pill listed">for sale · {listing.price} {listing.currency}</span>
        <button class="primary tiny" onclick={interested} disabled={interestBusy}>
          {interestBusy ? "…" : "Express interest"}
        </button>
      </div>
      <p class="faint small">
        Express interest to let the seller know you want this NFT. The seller can
        then choose to send you a transfer offer, which you accept from Activity.
        This is not an instant purchase.
      </p>
    {:else if iOwn}
      <div class="row wrap" style="gap: 0.5rem; align-items: center">
        <input class="mono" style="max-width: 8rem" bind:value={price} placeholder="price" disabled={busy} />
        <input class="mono" style="max-width: 6rem" bind:value={currency} placeholder="currency" disabled={busy} />
        <button class="primary tiny" onclick={list} disabled={busy || !price}>
          {busy ? "…" : "List for sale"}
        </button>
      </div>
    {:else}
      <p class="muted small">Not listed for sale.</p>
    {/if}
  </div>
{/if}

<style>
  .listed {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in oklab, var(--accent) 35%, transparent);
  }
</style>
