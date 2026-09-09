<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { NftView } from "$lib/appState.svelte";
  import { nftTitle } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";

  let {
    view,
    onopen,
    onhide,
    onunhide,
  }: {
    view: NftView;
    onopen: (nft: ActionHash) => void;
    onhide?: (nft: ActionHash) => void;
    onunhide?: (nft: ActionHash) => void;
  } = $props();

  const payload = $derived(view.manifest.payload);
  const title = $derived(nftTitle(view.manifest));
  let imgOk = $state(true);

  function toggleHide(e: MouseEvent) {
    e.stopPropagation();
    (view.hidden ? onunhide : onhide)?.(view.nft);
  }
</script>

<div
  class="card"
  role="button"
  tabindex="0"
  onclick={() => onopen(view.nft)}
  onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onopen(view.nft)}
>
  <div class="thumb-wrap">
    {#if payload.status === "revealed" && imgOk}
      <img class="thumb" src={payload.image_uri} alt="" onerror={() => (imgOk = false)} />
    {:else}
      <div class="thumb placeholder">
        {payload.status === "revealed" ? "no image" : "sealed"}
      </div>
    {/if}
    {#if onhide || onunhide}
      <button
        class="hide-btn"
        title={view.hidden ? "show in collection" : "hide from collection"}
        onclick={toggleHide}
      >
        {view.hidden ? "☑" : "🞫"}
      </button>
    {/if}
  </div>

  <div class="body">
    <div class="row between">
      <strong class="truncate">{title}</strong>
      <span class="pill {payload.status}">{payload.status}</span>
    </div>
    <div class="row between">
      <AgentBadge agent={view.minter} size={18} />
      {#if payload.status === "sealed" && !view.hasSalt}
        <span class="pill danger" title="reveal salt not on this device">no salt</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .thumb-wrap {
    position: relative;
  }
  .hide-btn {
    position: absolute;
    top: 0.4rem;
    right: 0.4rem;
    width: 26px;
    height: 26px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: color-mix(in oklab, var(--bg-elev) 80%, transparent);
    color: var(--text-dim);
    opacity: 0;
    transition: opacity 0.12s ease;
    backdrop-filter: blur(4px);
  }
  .card:hover .hide-btn,
  .hide-btn:focus-visible {
    opacity: 1;
  }
  .hide-btn:hover {
    color: var(--text);
    border-color: var(--accent);
  }
</style>
