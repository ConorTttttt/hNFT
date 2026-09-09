<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { NftView } from "$lib/appState.svelte";
  import { shortHash } from "$lib/format";

  let { view, onopen }: { view: NftView; onopen: (nft: ActionHash) => void } =
    $props();

  const status = $derived(view.manifest.payload.status);
</script>

<button
  class="panel stack"
  style="text-align: left; align-items: stretch"
  onclick={() => onopen(view.nft)}
>
  <div class="row" style="justify-content: space-between">
    <span class="tag {status}">{status}</span>
    {#if status === "sealed" && !view.hasSalt}
      <span class="tag error" title="reveal salt not on this device">
        salt missing
      </span>
    {/if}
  </div>

  {#if view.manifest.payload.status === "revealed"}
    <img
      src={view.manifest.payload.image_uri}
      alt=""
      style="width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; background: var(--panel-2)"
    />
  {:else}
    <div
      style="width: 100%; aspect-ratio: 1; border-radius: 8px; background: var(--panel-2); display: grid; place-items: center"
      class="muted"
    >
      sealed
    </div>
  {/if}

  <span class="mono muted">{shortHash(view.nft)}</span>
</button>
