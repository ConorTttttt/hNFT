<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, relativeTime, shortHash } from "$lib/format";
  import MintBlindBatchDialog from "./MintBlindBatchDialog.svelte";
  import ReserveBlindPurchaseDialog from "./ReserveBlindPurchaseDialog.svelte";
  import ResolveBlindAssignmentDialog from "./ResolveBlindAssignmentDialog.svelte";
  import type { ActionHash } from "@holochain/client";

  const app = useAppState();

  let showMintBatch = $state(false);
  let showReserve = $state(false);
  let reserveBatch = $state<ActionHash | null>(null);
  let showResolve = $state(false);
  let resolvePurchase = $state<ActionHash | null>(null);

  function openReserve(batch: ActionHash) {
    reserveBatch = batch;
    showReserve = true;
  }
  function openResolve(purchase: ActionHash) {
    resolvePurchase = purchase;
    showResolve = true;
  }

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
    Each batch is a set of sealed NFTs assigned randomly to buyers once their
    purchase resolves — see the collection's docs for how assignment is kept
    fair before you buy.
  </p>

  {#if app.isMinter && app.myBatchPurchases.length}
    <div class="panel stack tight">
      <h4>Resolution queue · {app.myBatchPurchases.length}</h4>
      <p class="faint small">
        Purchases reserved against your own batches, waiting on a settlement
        claim and resolution.
      </p>
      {#each app.myBatchPurchases as row (b64(row.purchase))}
        <div class="row between wrap" style="gap: 0.5rem; align-items: center">
          <div class="stack tight">
            <span class="mono small">{shortHash(row.purchase)}</span>
            <span class="faint small"
              >buyer {shortHash(row.buyer)} · reserved {relativeTime(row.reservedAtMs)}</span
            >
          </div>
          <button class="primary tiny" onclick={() => openResolve(row.purchase)}>
            Resolve
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if myBatches.length}
    <div class="stack tight">
      <h4>Your batches</h4>
      <div class="grid">
        {#each myBatches as batch (b64(batch.batch))}
          <div class="panel stack tight">
            <span class="pill cyan">mine</span>
            <span class="mono small">{shortHash(batch.batch)}</span>
            <span class="faint small"
              >{batch.batchSize} items · minted {relativeTime(batch.mintedAtMs)}</span
            >
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
            <button class="primary tiny" onclick={() => openReserve(batch.batch)}>
              Buy blind
            </button>
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
{#if resolvePurchase}
  <ResolveBlindAssignmentDialog bind:open={showResolve} purchase={resolvePurchase} />
{/if}
