<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, nftTitle } from "$lib/format";
  import { toast } from "$lib/toast.svelte";
  import AgentBadge from "./AgentBadge.svelte";
  import ProvenanceTimeline from "./ProvenanceTimeline.svelte";
  import RevealFlow from "./RevealFlow.svelte";
  import TransferDialog from "./TransferDialog.svelte";

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let showTransfer = $state(false);
  let finalizeBusy = $state(false);
  let imgOk = $state(true);

  $effect(() => {
    dlg?.showModal();
  });

  const nft = $derived(app.detailNft);
  const d = $derived(app.detail);
  const iOwn = $derived(!!d && app.isMe(d.owner));
  const iMinted = $derived(!!d && app.isMe(d.minter));
  const staleWalletEntry = $derived(
    !!nft && !iOwn && app.myNfts.some((v) => eqHash(v.nft, nft)),
  );

  async function finalize() {
    if (!nft) return;
    finalizeBusy = true;
    try {
      await app.finalizeSale(nft);
    } finally {
      finalizeBusy = false;
    }
  }

  async function copyId() {
    if (!nft) return;
    try {
      await navigator.clipboard.writeText(b64(nft));
      toast.info("NFT id copied");
    } catch {
      toast.error("Clipboard unavailable");
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => app.closeDetail()}>
  <div class="detail">
    {#if !d || !nft}
      {#if app.detailLoading}
        <p class="muted">Loading…</p>
      {:else}
        <p class="muted">Not found.</p>
      {/if}
      <button class="ghost" onclick={() => dlg?.close()}>Close</button>
    {:else}
      <div class="row between">
        <div class="stack tight" style="min-width: 0">
          <h3 class="truncate">{nftTitle(d.manifest)}</h3>
          <span class="pill {d.manifest.payload.status}">{d.manifest.payload.status}</span>
        </div>
        <button class="ghost tiny" onclick={() => dlg?.close()}>Close ✕</button>
      </div>

      <div class="hero">
        {#if d.manifest.payload.status === "revealed" && imgOk}
          <img
            src={d.manifest.payload.image_uri}
            alt=""
            onerror={() => (imgOk = false)}
          />
        {:else}
          <div class="placeholder">
            {d.manifest.payload.status === "revealed" ? "no image" : "sealed"}
          </div>
        {/if}
      </div>

      {#if d.manifest.payload.status === "revealed" && d.manifest.payload.attributes.length}
        <dl class="defs">
          {#each d.manifest.payload.attributes as a (a.trait_type)}
            <dt>{a.trait_type}</dt>
            <dd>{a.value}</dd>
          {/each}
        </dl>
      {/if}

      <dl class="defs">
        <dt>id</dt>
        <dd>
          <button class="chip" onclick={copyId}>
            <span class="mono truncate" style="max-width: 260px">{b64(nft)}</span>
            <span class="faint">⧉</span>
          </button>
        </dd>
        <dt>minter</dt>
        <dd><AgentBadge agent={d.minter} size={18} /></dd>
        <dt>owner</dt>
        <dd><AgentBadge agent={d.owner} size={18} /></dd>
      </dl>

      {#if (d.manifest.payload.status === "sealed" && iMinted) || iOwn || staleWalletEntry}
        <div class="divider"></div>
        <div class="stack tight">
          {#if d.manifest.payload.status === "sealed" && iMinted}
            <RevealFlow {nft} hasSalt={d.hasSalt} />
          {/if}
          {#if iOwn}
            <div class="row wrap" style="gap: 0.5rem">
              <button class="primary" onclick={() => (showTransfer = true)}>
                Transfer
              </button>
              {#if app.isNftHidden(nft)}
                <button class="ghost" onclick={() => app.unhideNft(nft)}>
                  Show in collection
                </button>
              {:else}
                <button class="ghost" onclick={() => app.hideNft(nft)}>
                  Hide from collection
                </button>
              {/if}
            </div>
          {/if}
          {#if staleWalletEntry}
            <div class="row wrap" style="gap: 0.5rem">
              <button class="ghost tiny" onclick={finalize} disabled={finalizeBusy}>
                {finalizeBusy ? "…" : "Remove stale wallet entry"}
              </button>
              <span class="faint small">You sold this — drop the leftover index link.</span>
            </div>
          {/if}
        </div>
      {/if}

      <div class="divider"></div>
      <h4>Provenance</h4>
      <ProvenanceTimeline hops={d.provenance} />
    {/if}
  </div>
</dialog>

{#if showTransfer && nft}
  <TransferDialog {nft} onclose={() => (showTransfer = false)} />
{/if}

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    width: min(560px, 92vw);
  }
  .hero {
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--panel-2);
  }
  .hero img {
    display: block;
    width: 100%;
    max-height: 340px;
    object-fit: contain;
  }
  .hero .placeholder {
    aspect-ratio: 16/10;
    display: grid;
    place-items: center;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.8rem;
  }
</style>
