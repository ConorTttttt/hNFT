<script lang="ts">
  import { NFT_ROLE, useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, nftTitle, relativeFuture, tsToMs, unb64 } from "$lib/format";
  import { toast } from "$lib/toast.svelte";
  import AgentBadge from "./AgentBadge.svelte";
  import CommentThread from "./CommentThread.svelte";
  import FollowButton from "./FollowButton.svelte";
  import LikeButton from "./LikeButton.svelte";
  import ListingSection from "./ListingSection.svelte";
  import ProvenanceTimeline from "./ProvenanceTimeline.svelte";
  import RevealFlow from "./RevealFlow.svelte";
  import TransferDialog from "./TransferDialog.svelte";
  import UnytSettleDialog from "./UnytSettleDialog.svelte";

  const app = useAppState();

  let dlg = $state<HTMLDialogElement>();
  let showTransfer = $state(false);
  let showUnytSettle = $state(false);
  let finalizeBusy = $state(false);
  let imgOk = $state(true);

  $effect(() => {
    dlg?.showModal();
  });

  const nft = $derived(app.detailNft);
  const d = $derived(app.detail);
  const iOwn = $derived(!!d && app.isMe(d.owner));
  const iMinted = $derived(!!d && app.isMe(d.minter));
  const ag = $derived(d?.agreement ?? null);
  const sealed = $derived(d?.manifest.payload.status === "sealed");
  const pastDefault = $derived(
    !!ag && Date.now() >= tsToMs(ag.default_after_micros),
  );
  const canMarkDefault = $derived(
    !!nft && !!ag && sealed && !iMinted && pastDefault && !ag.defaulted,
  );

  let markBusy = $state(false);
  async function markDefault() {
    if (!nft) return;
    markBusy = true;
    try {
      await app.markDefault(nft);
    } finally {
      markBusy = false;
    }
  }

  async function copySecret() {
    if (!nft) return;
    const blob = await app.revealSecretBlob(nft);
    if (!blob) {
      toast.error("No reveal secret is stored on this device for this NFT");
      return;
    }
    try {
      await navigator.clipboard.writeText(blob);
      toast.info("Reveal secret copied — hand it to your delegate");
    } catch {
      toast.error("Clipboard unavailable");
    }
  }
  const staleWalletEntry = $derived(
    !!nft && !iOwn && app.myNfts.some((v) => eqHash(v.nft, nft)),
  );

  let addToCollectionTarget = $state("");
  async function addToCollection() {
    if (!nft || !addToCollectionTarget) return;
    await app.addToCollection(unb64(addToCollectionTarget), nft, NFT_ROLE);
    addToCollectionTarget = "";
  }

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
          <div class="row" style="gap: 0.4rem">
            <span class="pill {d.manifest.payload.status}">{d.manifest.payload.status}</span>
            <LikeButton target={nft} targetRole={NFT_ROLE} />
          </div>
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
        <dd class="row" style="gap: 0.4rem">
          <AgentBadge agent={d.minter} size={18} />
          {#if !app.isMe(d.minter)}<FollowButton agent={d.minter} />{/if}
        </dd>
        <dt>owner</dt>
        <dd class="row" style="gap: 0.4rem">
          <AgentBadge agent={d.owner} size={18} />
          {#if !app.isMe(d.owner)}<FollowButton agent={d.owner} />{/if}
        </dd>
      </dl>

      {#if app.myCollections.length}
        <div class="row wrap" style="gap: 0.5rem; align-items: center">
          <select bind:value={addToCollectionTarget}>
            <option value="">Add to collection…</option>
            {#each app.myCollections as c (b64(c.collection))}
              <option value={b64(c.collection)}>{c.name}</option>
            {/each}
          </select>
          <button class="ghost tiny" onclick={addToCollection} disabled={!addToCollectionTarget}>
            Add
          </button>
        </div>
      {/if}

      <div class="divider"></div>
      <ListingSection {nft} {iOwn} />

      {#if ag}
        <div class="divider"></div>
        <div class="stack tight">
          <div class="row between">
            <h4>Reveal agreement</h4>
            {#if ag.defaulted}
              <span class="pill danger">defaulted</span>
            {:else if sealed && Date.now() >= tsToMs(ag.reveal_by_micros)}
              <span class="pill warn">reveal overdue</span>
            {/if}
          </div>
          <dl class="defs">
            <dt>reveal by</dt>
            <dd>
              {new Date(tsToMs(ag.reveal_by_micros)).toLocaleString()}
              <span class="faint small">({relativeFuture(tsToMs(ag.reveal_by_micros))})</span>
            </dd>
            <dt>default after</dt>
            <dd>
              {new Date(tsToMs(ag.default_after_micros)).toLocaleString()}
              <span class="faint small">({relativeFuture(tsToMs(ag.default_after_micros))})</span>
            </dd>
            {#if ag.delegate}
              <dt>delegate</dt>
              <dd><AgentBadge agent={ag.delegate} size={18} /></dd>
            {/if}
          </dl>
          {#if canMarkDefault}
            <div class="row wrap" style="gap: 0.5rem; align-items: center">
              <button class="ghost" onclick={markDefault} disabled={markBusy}>
                {markBusy ? "…" : "Mark reveal defaulted"}
              </button>
              <span class="faint small">
                The deadline has passed and it's still sealed.
              </span>
            </div>
          {/if}
          {#if iMinted && ag.delegate && sealed}
            <div class="row wrap" style="gap: 0.5rem; align-items: center">
              <button class="ghost tiny" onclick={copySecret}>
                Copy reveal secret for delegate
              </button>
              <span class="faint small">Hand it over out of band.</span>
            </div>
          {/if}
        </div>
      {/if}

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
              <button class="ghost" onclick={() => (showUnytSettle = true)}>
                Settle via Unyt (demo)
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

      <div class="divider"></div>
      <CommentThread target={nft} targetRole={NFT_ROLE} />
    {/if}
  </div>
</dialog>

{#if showTransfer && nft}
  <TransferDialog {nft} onclose={() => (showTransfer = false)} />
{/if}
{#if showUnytSettle && nft}
  <UnytSettleDialog {nft} onclose={() => (showUnytSettle = false)} />
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
