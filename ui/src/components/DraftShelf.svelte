<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";

  const app = useAppState();

  let busy = $state<string | null>(null);

  async function reveal(nft: ActionHash) {
    busy = b64(nft);
    try {
      await app.reveal(nft, nft); // first reveal branches off the mint (hash === nft)
    } finally {
      busy = null;
    }
  }
</script>

{#if app.drafts.length}
  <div class="panel stack">
    <div class="row between">
      <h3>Awaiting your reveal <span class="faint">· {app.drafts.length}</span></h3>
      <span class="faint small">NFTs you minted, still sealed</span>
    </div>
    <div class="stack tight">
      {#each app.drafts as d (b64(d.nft))}
        <div class="draft">
          {#if d.imageUri}
            <img class="thumb-xs" src={d.imageUri} alt="" />
          {:else}
            <div class="thumb-xs"></div>
          {/if}
          <div class="stack tight grow" style="min-width: 0">
            <strong class="truncate">{d.title}</strong>
            <span class="faint small">
              {d.ownedByOther ? "held by another agent" : "held by you"} · sealed
            </span>
          </div>
          <button class="ghost tiny" onclick={() => app.openDetail(d.nft)}>
            Details
          </button>
          <button
            class="primary tiny"
            disabled={busy === b64(d.nft)}
            onclick={() => reveal(d.nft)}
          >
            {busy === b64(d.nft) ? "Revealing…" : "Reveal"}
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .draft {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
</style>
