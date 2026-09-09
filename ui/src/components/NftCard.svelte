<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { NftView } from "$lib/appState.svelte";
  import { nftTitle } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";

  let { view, onopen }: { view: NftView; onopen: (nft: ActionHash) => void } =
    $props();

  const payload = $derived(view.manifest.payload);
  const title = $derived(nftTitle(view.manifest));
  let imgOk = $state(true);
</script>

<button class="card" onclick={() => onopen(view.nft)}>
  {#if payload.status === "revealed" && imgOk}
    <img
      class="thumb"
      src={payload.image_uri}
      alt=""
      onerror={() => (imgOk = false)}
    />
  {:else}
    <div class="thumb placeholder">
      {payload.status === "revealed" ? "no image" : "sealed"}
    </div>
  {/if}

  <div class="body">
    <div class="row between">
      <strong class="truncate">{title}</strong>
      <span class="pill {payload.status}">{payload.status}</span>
    </div>
    <div class="row between">
      <AgentBadge agent={view.minter} size={18} />
      {#if payload.status === "sealed" && !view.hasSalt}
        <span class="pill danger" title="reveal salt not on this device">
          no salt
        </span>
      {/if}
    </div>
  </div>
</button>
