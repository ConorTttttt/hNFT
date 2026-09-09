<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { NftView } from "$lib/appState.svelte";
  import { shortHash } from "$lib/format";

  let { view, onopen }: { view: NftView; onopen: (nft: ActionHash) => void } =
    $props();

  const payload = $derived(view.manifest.payload);
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
      <span class="pill {payload.status}">{payload.status}</span>
      {#if payload.status === "sealed" && !view.hasSalt}
        <span class="pill danger" title="reveal salt not on this device">
          no salt
        </span>
      {/if}
    </div>
    <span class="mono faint small">{shortHash(view.nft)}</span>
  </div>
</button>
