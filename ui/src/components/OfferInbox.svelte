<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, shortHash } from "$lib/format";
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

<div class="panel stack">
  <div class="row between">
    <h3>Inbox {#if app.inbox.length}<span class="faint">· {app.inbox.length}</span>{/if}</h3>
    <button
      class="ghost tiny"
      onclick={() => app.loadInbox()}
      disabled={app.inboxLoading}
    >
      {app.inboxLoading ? "…" : "Refresh"}
    </button>
  </div>

  {#if !app.inbox.length}
    <div class="empty small">No inbound offers.</div>
  {:else}
    <div class="stack tight">
      {#each app.inbox as offer (b64(offer.commitment))}
        {@const expired = now > offer.deadlineMs}
        <div class="offer">
          <div class="stack tight grow" style="min-width: 0">
            <span class="mono truncate">NFT {shortHash(offer.nft)}</span>
            <span class="faint small mono truncate">
              from {shortHash(offer.from)} · hop #{offer.sequence}
            </span>
          </div>
          {#if expired}
            <span class="pill danger">expired</span>
          {:else}
            <Countdown deadlineMs={offer.deadlineMs} />
          {/if}
          <button
            class="primary tiny"
            disabled={expired || busyId === b64(offer.commitment)}
            onclick={() => accept(offer.commitment)}
          >
            Accept
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .offer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
</style>
