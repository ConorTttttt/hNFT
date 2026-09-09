<script lang="ts">
  import type { InboundOffer } from "$lib/appState.svelte";
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

  const isActive = (o: InboundOffer) =>
    !o.hidden && o.status === "pending" && now < o.deadlineMs;

  const active = $derived(app.inbox.filter(isActive));
  const resolved = $derived(app.inbox.filter((o) => !isActive(o)));

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
    Pending offers {#if active.length}<span class="faint">· {active.length}</span>{/if}
  </h3>

  {#if !active.length}
    <div class="empty small">Nothing waiting for you.</div>
  {:else}
    <div class="stack tight">
      {#each active as o (b64(o.commitment))}
        <div class="offer">
          {#if o.imageUri}
            <img class="thumb-xs" src={o.imageUri} alt="" />
          {:else}
            <div class="thumb-xs"></div>
          {/if}
          <div class="stack tight grow" style="min-width: 0">
            <strong class="truncate">{o.title}</strong>
            <div class="row" style="gap: 0.4rem">
              <AgentBadge agent={o.from} size={16} />
              <span class="faint small">· hop #{o.sequence}</span>
            </div>
          </div>
          <Countdown deadlineMs={o.deadlineMs} />
          <button
            class="primary tiny"
            disabled={busyId === b64(o.commitment)}
            onclick={() => accept(o.commitment)}
          >
            Accept
          </button>
          <button class="ghost tiny" onclick={() => app.dismissOffer(o.commitment)}>
            Hide
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if resolved.length}
    <details class="resolved">
      <summary>Resolved &amp; hidden · {resolved.length}</summary>
      <div class="stack tight" style="margin-top: 0.5rem">
        {#each resolved as o (b64(o.commitment))}
          {@const expired = now >= o.deadlineMs}
          <div class="offer dim">
            {#if o.imageUri}
              <img class="thumb-xs" src={o.imageUri} alt="" />
            {:else}
              <div class="thumb-xs"></div>
            {/if}
            <div class="stack tight grow" style="min-width: 0">
              <span class="truncate">{o.title}</span>
              <AgentBadge agent={o.from} size={14} />
            </div>
            {#if o.status === "accepted"}
              <span class="pill revealed">accepted</span>
            {:else if expired}
              <span class="pill danger">expired</span>
            {/if}
            {#if o.hidden}
              <span class="pill">hidden</span>
              <button class="ghost tiny" onclick={() => app.restoreOffer(o.commitment)}>
                Unhide
              </button>
            {:else}
              <button class="ghost tiny" onclick={() => app.dismissOffer(o.commitment)}>
                Hide
              </button>
            {/if}
          </div>
        {/each}
      </div>
    </details>
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
  .offer.dim {
    opacity: 0.7;
  }
  .resolved summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.85rem;
    list-style: revert;
  }
</style>
