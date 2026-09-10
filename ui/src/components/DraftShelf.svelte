<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, relativeFuture, tsToMs } from "$lib/format";

  const app = useAppState();

  let busy = $state<string | null>(null);

  // tick so the deadline copy stays fresh
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

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
        {@const ag = d.agreement}
        {@const revealByMs = ag ? tsToMs(ag.reveal_by_micros) : 0}
        {@const defaultAfterMs = ag ? tsToMs(ag.default_after_micros) : 0}
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
            {#if ag}
              <div class="row wrap" style="gap: 0.4rem; align-items: center">
                {#if ag.defaulted}
                  <span class="pill danger">reveal defaulted</span>
                {:else}
                  <span class="pill" class:warn={now >= revealByMs}>
                    reveal {relativeFuture(revealByMs)}
                  </span>
                {/if}
                {#if d.autoReveal}
                  <span class="faint small">· auto-reveal on</span>
                {/if}
                {#if !ag.defaulted && now >= defaultAfterMs}
                  <span class="faint small">· past deadline — anyone may mark it defaulted</span>
                {/if}
              </div>
            {/if}
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
