<script lang="ts">
  import { COLLECTION_KIND_LABEL, MINT_POLICY_LABEL, type CollectionCardData } from "$lib/collectionCards";
  import Identicon from "./Identicon.svelte";

  let { card, onselect }: { card: CollectionCardData; onselect: (cloneId: string | undefined) => void } =
    $props();

  // A deterministic, protocol-free placeholder — no media/image infrastructure.
  const seed = $derived(new TextEncoder().encode(card.cloneId ?? "default-collection"));

  const supplyLine = $derived.by(() => {
    const s = card.supply;
    if (s.minted === null) return "Minted: —";
    if (s.unlimited) return `${s.minted} minted · Unlimited`;
    if (s.max === null) return `${s.minted} minted`;
    return `${s.minted} / ${s.max} minted`;
  });
</script>

<div
  class="card"
  class:active={card.isActive}
  role="button"
  tabindex="0"
  aria-label={`Open collection ${card.name}${card.isActive ? " (active)" : ""}`}
  onclick={() => onselect(card.cloneId)}
  onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), onselect(card.cloneId))}
>
  <div class="thumb" aria-hidden="true">
    <Identicon {seed} size={96} />
  </div>
  <div class="body">
    <div class="row between">
      <strong class="truncate">{card.name}</strong>
      {#if card.symbol}<span class="pill">{card.symbol}</span>{/if}
    </div>

    <div class="row wrap" style="gap: 0.35rem">
      <span class="pill" class:cyan={card.kind === "generative"} class:warn={card.kind === "blind"}>
        {COLLECTION_KIND_LABEL[card.kind]}
      </span>
      {#if card.isActive}<span class="pill accent">Active</span>{/if}
      {#if card.mintPolicy}
        <span class="pill">{MINT_POLICY_LABEL[card.mintPolicy]}</span>
      {/if}
      {#if card.canMint}<span class="pill cyan">You can mint</span>{/if}
    </div>

    <span class="faint small">{supplyLine}</span>
    {#if card.listingCount !== null}
      <span class="faint small">For sale · {card.listingCount}</span>
    {/if}
  </div>
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 1rem;
    border-radius: 14px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    cursor: pointer;
    transition:
      border-color 0.12s ease,
      transform 0.12s ease;
  }
  .card:hover,
  .card:focus-visible {
    border-color: var(--accent);
    transform: translateY(-2px);
  }
  .card.active {
    border-color: var(--accent);
  }
  .thumb {
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 1.6;
    border-radius: 10px;
    background: color-mix(in oklab, var(--bg) 92%, var(--accent) 8%);
  }
</style>
