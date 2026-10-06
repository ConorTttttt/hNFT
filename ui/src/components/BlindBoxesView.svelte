<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, relativeTime, shortHash } from "$lib/format";
  import MintBlindBatchDialog from "./MintBlindBatchDialog.svelte";
  import ReserveBlindPurchaseDialog from "./ReserveBlindPurchaseDialog.svelte";
  import BatchAssignmentsDialog from "./BatchAssignmentsDialog.svelte";
  import type { ActionHash } from "@holochain/client";
  import type { BlindBatchView } from "$lib/appState.svelte";

  const app = useAppState();

  let showMintBatch = $state(false);
  let showReserve = $state(false);
  let reserveBatch = $state<ActionHash | null>(null);
  let showAssign = $state(false);
  let assignBatch = $state<BlindBatchView | null>(null);
  let busyBatch = $state<string | null>(null);

  function openReserve(batch: ActionHash) {
    reserveBatch = batch;
    showReserve = true;
  }
  function openAssign(batch: BlindBatchView) {
    assignBatch = batch;
    showAssign = true;
  }
  async function act(batch: BlindBatchView, fn: () => Promise<unknown>) {
    busyBatch = b64(batch.batch);
    try {
      await fn();
    } finally {
      busyBatch = null;
    }
  }
  const stateLabel: Record<string, string> = {
    DRAFT: "draft",
    SALES_OPEN: "sales open",
    CLOSED: "closed",
    RANDOMNESS_FIXED: "randomness fixed",
  };
  const openPurchases = (b: BlindBatchView) =>
    app.myBatchPurchases.filter((p) => b64(p.batch) === b64(b.batch)).length;

  const myBatches = $derived(app.blindBatches.filter((b) => eqHash(b.minter, app.me)));
  const otherBatches = $derived(app.blindBatches.filter((b) => !eqHash(b.minter, app.me)));
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="row between wrap">
    <h3>
      Blind boxes {#if app.blindBatches.length}<span class="faint"
          >· {app.blindBatches.length}</span
        >{/if}
    </h3>
    {#if app.isMinter}
      <button class="primary" onclick={() => (showMintBatch = true)}>+ Mint blind batch</button>
    {/if}
  </div>
  <p class="faint small">
    Each batch commits a fixed inventory of N sealed NFTs. When the minter
    closes it with K purchases (1 ≤ K ≤ N), randomness selects K of the N NFTs
    and assigns them to the K purchases. The minter chooses which purchases are
    included, and unselected NFTs stay locked — see the docs for what this does
    and doesn't guarantee.
  </p>

  {#if myBatches.length}
    <div class="stack tight">
      <h4>Your batches</h4>
      <div class="grid">
        {#each myBatches as batch (b64(batch.batch))}
          <div class="panel stack tight">
            <div class="row" style="gap: 0.4rem">
              <span class="pill cyan">mine</span>
              <span class="pill">{stateLabel[batch.state] ?? batch.state}</span>
            </div>
            <span class="mono small">{shortHash(batch.batch)}</span>
            <span class="faint small"
              >{batch.batchSize} items · minted {relativeTime(batch.mintedAtMs)}</span
            >
            {#if batch.state === "SALES_OPEN"}
              <span class="faint small">{openPurchases(batch)} purchase(s) so far</span>
              <button
                class="primary tiny"
                disabled={busyBatch !== null || openPurchases(batch) === 0}
                onclick={() => act(batch, () => app.closeBlindBatch(batch.batch))}
              >
                Close batch
              </button>
            {:else if batch.state === "CLOSED" && batch.close}
              <button
                class="tiny"
                disabled={busyBatch !== null}
                onclick={() => act(batch, () => app.recordDevRandomness(batch.close!))}
              >
                Record dev randomness (insecure)
              </button>
            {:else if batch.state === "RANDOMNESS_FIXED"}
              <button class="primary tiny" onclick={() => openAssign(batch)}>Assignments</button>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div class="stack tight">
    <h4>Buy blind</h4>
    {#if otherBatches.length}
      <div class="grid">
        {#each otherBatches as batch (b64(batch.batch))}
          <div class="panel stack tight">
            <span class="mono small">{shortHash(batch.batch)}</span>
            <span class="faint small"
              >{batch.batchSize} items · by {shortHash(batch.minter)} · {relativeTime(
                batch.mintedAtMs,
              )}</span
            >
            <div class="row" style="gap: 0.4rem">
              <span class="pill">{stateLabel[batch.state] ?? batch.state}</span>
            </div>
            {#if batch.state === "SALES_OPEN"}
              <button class="primary tiny" onclick={() => openReserve(batch.batch)}>
                Buy blind
              </button>
            {:else if batch.state === "RANDOMNESS_FIXED"}
              <button class="tiny" onclick={() => openAssign(batch)}>Assignments</button>
            {/if}
          </div>
        {/each}
      </div>
    {:else if app.blindBatchesLoading}
      <div class="grid">
        {#each Array(3) as _, i (i)}
          <div class="skeleton" style="aspect-ratio: 1.4"></div>
        {/each}
      </div>
    {:else}
      <div class="empty small">No other blind-box batches yet.</div>
    {/if}
  </div>
</div>

{#if showMintBatch}
  <MintBlindBatchDialog bind:open={showMintBatch} />
{/if}
{#if reserveBatch}
  <ReserveBlindPurchaseDialog bind:open={showReserve} batch={reserveBatch} />
{/if}
{#if assignBatch && assignBatch.close && assignBatch.result}
  <BatchAssignmentsDialog
    bind:open={showAssign}
    close={assignBatch.close}
    result={assignBatch.result}
    minter={assignBatch.minter}
  />
{/if}
