<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, shortHash } from "$lib/format";
  import ProvenanceTimeline from "./ProvenanceTimeline.svelte";
  import RevealFlow from "./RevealFlow.svelte";
  import TransferDialog from "./TransferDialog.svelte";

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let showTransfer = $state(false);
  let finalizeBusy = $state(false);
  let err = $state("");

  $effect(() => {
    dlg?.showModal();
  });

  function close() {
    app.closeDetail();
  }

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
    err = "";
    try {
      await app.finalizeSale(nft);
    } catch (e) {
      err = String(e);
    } finally {
      finalizeBusy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={close} style="min-width: min(560px, 94vw)">
  {#if app.detailLoading && !d}
    <p class="muted">Loading…</p>
  {:else if !d || !nft}
    <p class="muted">Not found.</p>
    <button class="ghost" onclick={() => dlg?.close()}>Close</button>
  {:else}
    <div class="stack">
      <div class="row" style="justify-content: space-between">
        <span class="tag {d.manifest.payload.status}">{d.manifest.payload.status}</span>
        <button class="ghost" onclick={() => dlg?.close()}>Close</button>
      </div>

      {#if d.manifest.payload.status === "revealed"}
        <img
          src={d.manifest.payload.image_uri}
          alt=""
          style="width: 100%; max-height: 320px; object-fit: contain; border-radius: 8px; background: var(--panel-2)"
        />
        <div class="stack" style="gap: 0.25rem">
          {#each d.manifest.payload.attributes as a (a.trait_type)}
            <div class="row" style="justify-content: space-between">
              <span class="muted">{a.trait_type}</span><span>{a.value}</span>
            </div>
          {/each}
        </div>
      {/if}

      <dl style="display: grid; grid-template-columns: auto 1fr; gap: 0.25rem 0.75rem; margin: 0">
        <dt class="muted">id</dt>
        <dd class="mono" style="margin: 0">{b64(nft)}</dd>
        <dt class="muted">minter</dt>
        <dd class="mono" style="margin: 0">{iMinted ? "you" : shortHash(d.minter)}</dd>
        <dt class="muted">owner</dt>
        <dd class="mono" style="margin: 0">{iOwn ? "you" : shortHash(d.owner)}</dd>
      </dl>

      {#if d.manifest.payload.status === "sealed" && iMinted}
        <RevealFlow {nft} hasSalt={d.hasSalt} />
      {/if}

      {#if iOwn}
        <div class="row">
          <button class="primary" onclick={() => (showTransfer = true)}>Transfer</button>
        </div>
      {/if}

      {#if staleWalletEntry}
        <div class="row">
          <button class="ghost" onclick={finalize} disabled={finalizeBusy}>
            {finalizeBusy ? "…" : "Remove stale wallet entry"}
          </button>
          <span class="muted">You sold this; drop your leftover index link.</span>
        </div>
      {/if}

      {#if err}<p class="error">{err}</p>{/if}

      <div class="stack" style="gap: 0.35rem">
        <h4 style="margin: 0.5rem 0 0">Provenance</h4>
        <ProvenanceTimeline hops={d.provenance} />
      </div>
    </div>
  {/if}
</dialog>

{#if showTransfer && nft}
  <TransferDialog {nft} onclose={() => (showTransfer = false)} />
{/if}
