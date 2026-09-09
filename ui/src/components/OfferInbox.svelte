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
  let err = $state("");

  async function accept(commitment: Uint8Array) {
    busyId = b64(commitment);
    err = "";
    try {
      await app.acceptOffer(commitment);
    } catch (e) {
      err = String(e);
    } finally {
      busyId = null;
    }
  }
</script>

<div class="panel stack">
  <div class="row" style="justify-content: space-between">
    <h3 style="margin: 0">Inbox</h3>
    <button class="ghost" onclick={() => app.loadInbox()} disabled={app.inboxLoading}>
      {app.inboxLoading ? "…" : "Refresh"}
    </button>
  </div>

  {#if app.inbox.length === 0}
    <p class="muted">No inbound offers.</p>
  {:else}
    <div class="stack">
      {#each app.inbox as offer (b64(offer.commitment))}
        {@const expired = now > offer.deadlineMs}
        <div class="panel row" style="justify-content: space-between">
          <div class="stack" style="gap: 0.2rem">
            <span class="mono">NFT {shortHash(offer.nft)}</span>
            <span class="muted mono">
              from {shortHash(offer.from)} · hop #{offer.sequence}
            </span>
          </div>
          <div class="row">
            {#if expired}
              <span class="tag error">expired</span>
            {:else}
              <Countdown deadlineMs={offer.deadlineMs} />
            {/if}
            <button
              class="primary"
              disabled={expired || busyId === b64(offer.commitment)}
              onclick={() => accept(offer.commitment)}
            >
              Accept
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if err}<p class="error">{err}</p>{/if}
</div>
