<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import type { AssignmentView } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, shortHash } from "$lib/format";

  let {
    open = $bindable(false),
    close,
    result,
    minter,
  }: {
    open: boolean;
    close: ActionHash;
    result: ActionHash;
    minter: Uint8Array;
  } = $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let rows = $state<AssignmentView[]>([]);
  let loading = $state(false);
  let busyKey = $state<string | null>(null);

  const isMinter = $derived(eqHash(minter, app.me));

  $effect(() => {
    if (!open) {
      dlg?.close();
      return;
    }
    dlg?.showModal();
    loading = true;
    void app.loadBatchAssignments(close, result).then((r) => {
      rows = r;
      loading = false;
    });
  });

  async function release(a: AssignmentView) {
    busyKey = b64(a.purchase);
    try {
      await app.releaseAssignment(a, close, result);
    } finally {
      busyKey = null;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(520px, 90vw)">
    <h3>Assignment <span class="pill warn">dev randomness</span></h3>
    <p class="faint small">
      Derived deterministically from the closed purchase set and the recorded
      randomness — nobody, including the minter, chooses it. With K purchases
      in an N-item batch, K NFTs are selected; the rest stay locked. The
      randomness verifier here is a dev/test stand-in and is not secure.
    </p>
    {#if loading}
      <p class="faint small">Computing…</p>
    {:else}
      {#each rows as a (b64(a.purchase))}
        <div class="row between wrap" style="gap: 0.5rem; align-items: center">
          <div class="stack tight">
            <span class="small"
              >slot {a.slot} → buyer <span class="mono">{shortHash(a.buyer)}</span></span
            >
            <span class="faint small mono">nft {shortHash(a.nft)}</span>
          </div>
          {#if isMinter}
            <button class="primary tiny" onclick={() => release(a)} disabled={busyKey !== null}>
              {busyKey === b64(a.purchase) ? "Releasing…" : "Release"}
            </button>
          {/if}
        </div>
      {/each}
    {/if}
    <div class="row between">
      <button class="ghost" onclick={() => (open = false)}>Close</button>
    </div>
  </div>
</dialog>
