<script lang="ts">
  import { prepareCommitment, type NftAttribute } from "@p2p-art-marketplace/client";
  import { AppState, useAppState } from "$lib/appState.svelte";
  import { b64, unb64 } from "$lib/format";

  let { open = $bindable(false) }: { open: boolean } = $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let busy = $state(false);

  $effect(() => {
    if (open) dlg?.showModal();
    else dlg?.close();
  });

  /**
   * A blind batch commits N distinct sealed NFTs up front (one reveal
   * commitment per slot, same as an ordinary mint). This demo form generates
   * each slot's attributes from one shared template plus a slot number rather
   * than asking for N full attribute rows.
   */
  let batchSize = $state(10);
  let namePrefix = $state("Mystery Item");
  let imageUriTemplate = $state(""); // "{i}" is replaced with the 0-based slot index

  // Group the batch into a social Collection (the "Collections" tab — a
  // separate, curated grouping from the DNA cell every NFT already
  // belongs to) so buyers browse the whole batch together there.
  let groupIntoCollection = $state(true);
  let collectionMode = $state<"new" | "existing">("new");
  let newCollectionName = $state("");
  let newCollectionDescription = $state("");
  let existingCollectionHash = $state("");

  function randomSalt(): Uint8Array {
    const salt = new Uint8Array(32);
    crypto.getRandomValues(salt);
    return salt;
  }

  async function submit() {
    const n = Math.max(1, Math.min(AppState.MAX_UI_BATCH_SIZE, Math.floor(batchSize)));
    busy = true;
    try {
      const commitments = Array.from({ length: n }, (_, i) => {
        const attributes: NftAttribute[] = [
          { trait_type: "name", value: `${namePrefix.trim() || "Mystery Item"} #${i + 1}` },
        ];
        const imageUri = imageUriTemplate.trim().replace("{i}", String(i));
        return prepareCommitment(attributes, imageUri, randomSalt());
      });

      const group = !groupIntoCollection
        ? undefined
        : collectionMode === "existing" && existingCollectionHash
          ? { collection: unb64(existingCollectionHash) }
          : {
              name: newCollectionName.trim() || namePrefix.trim() || "Blind batch",
              description: newCollectionDescription.trim(),
            };

      const result = await app.mintBlindBatch(commitments, group);
      if (result) open = false;
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(440px, 88vw)">
    <h3>Mint a blind-box batch</h3>
    <p class="faint small">
      Commits <code>{batchSize}</code> sealed NFTs as a fixed inventory. Once
      buyers have purchased and you close the batch, randomness picks which of
      the committed NFTs are sold and which buyer gets which. Each item stays
      hidden until it is transferred and you reveal it (same Reveal flow as an
      ordinary mint).
    </p>

    <label class="field">
      <span>Batch size (max {AppState.MAX_UI_BATCH_SIZE})</span>
      <input
        type="number"
        min="1"
        max={AppState.MAX_UI_BATCH_SIZE}
        bind:value={batchSize}
        disabled={busy}
      />
    </label>
    <label class="field">
      <span>Name prefix — each item becomes "prefix #N"</span>
      <input bind:value={namePrefix} disabled={busy} />
    </label>
    <label class="field">
      <span>Image URI template — <code>{"{i}"}</code> becomes the slot number</span>
      <input
        bind:value={imageUriTemplate}
        placeholder="ipfs://…/{'{i}'}.png"
        disabled={busy}
      />
    </label>

    <label class="row" style="gap: 0.5rem">
      <input
        type="checkbox"
        bind:checked={groupIntoCollection}
        style="width: auto"
        disabled={busy}
      />
      <span class="small">Group this batch into a Collection</span>
    </label>

    {#if groupIntoCollection}
      <div class="stack tight" style="padding-left: 1.5rem">
        {#if app.myCollections.length}
          <label class="row" style="gap: 0.4rem">
            <input
              type="radio"
              name="collection-mode"
              value="new"
              bind:group={collectionMode}
              style="width: auto"
              disabled={busy}
            />
            <span class="small">New collection</span>
          </label>
        {/if}

        {#if collectionMode === "new" || !app.myCollections.length}
          <label class="field">
            <span>Collection name</span>
            <input
              bind:value={newCollectionName}
              placeholder={namePrefix.trim() || "Blind batch"}
              disabled={busy}
            />
          </label>
          <label class="field">
            <span>Description (optional)</span>
            <input bind:value={newCollectionDescription} disabled={busy} />
          </label>
        {/if}

        {#if app.myCollections.length}
          <label class="row" style="gap: 0.4rem">
            <input
              type="radio"
              name="collection-mode"
              value="existing"
              bind:group={collectionMode}
              style="width: auto"
              disabled={busy}
            />
            <span class="small">Add to an existing collection</span>
          </label>
          {#if collectionMode === "existing"}
            <select bind:value={existingCollectionHash} disabled={busy}>
              <option value="">Choose one…</option>
              {#each app.myCollections as c (b64(c.collection))}
                <option value={b64(c.collection)}>{c.name}</option>
              {/each}
            </select>
          {/if}
        {/if}
      </div>
    {/if}

    <p class="small faint">
      Every slot's salt is generated now and saved to this browser, same as
      an ordinary mint — you need it to reveal each item later.
    </p>

    <div class="row between">
      <button class="ghost" onclick={() => (open = false)} disabled={busy}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy}>
        {busy ? "Minting…" : "Mint batch"}
      </button>
    </div>
  </div>
</dialog>
