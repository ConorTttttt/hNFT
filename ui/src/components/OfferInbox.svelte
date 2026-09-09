<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";
  import Countdown from "./Countdown.svelte";

  const app = useAppState();

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  let busyId = $state<string | null>(null);

  async function accept(commitment: Uint8Array) {
    busyId = b64(commitment);
    try {
      await app.acceptOffer(commitment);
    } finally {
      busyId = null;
    }
  }
</script>

<div class="stack">
  <h3>
    Pending offers {#if app.inbox.length}<span class="faint">· {app.inbox.length}</span>{/if}
  </h3>

  {#if !app.inbox.length}
    <div class="empty small">Nothing waiting for you.</div>
  {:else}
    <div class="stack tight">
      {#each app.inbox as offer (b64(offer.commitment))}
        {@const expired = now > offer.deadlineMs}
        <div class="offer">
          {#if offer.imageUri}
            <img class="thumb-xs" src={offer.imageUri} alt="" />
          {:else}
            <div class="thumb-xs"></div>
          {/if}

          <div class="stack tight grow" style="min-width: 0">
            <strong class="truncate">{offer.title}</strong>
            <div class="row" style="gap: 0.4rem">
              <AgentBadge agent={offer.from} size={16} />
              <span class="faint small">· hop #{offer.sequence}</span>
            </div>
          </div>

          {#if expired}
            <span class="pill danger">expired</span>
            <button class="ghost tiny" onclick={() => app.dismissOffer(offer.commitment)}>
              Hide
            </button>
          {:else}
            <Countdown deadlineMs={offer.deadlineMs} />
            <button
              class="primary tiny"
              disabled={busyId === b64(offer.commitment)}
              onclick={() => accept(offer.commitment)}
            >
              Accept
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .offer {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
</style>
