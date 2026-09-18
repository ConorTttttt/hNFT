<script lang="ts">
  import { prepareCommitment, type NftAttribute } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { toHex } from "$lib/format";

  let { open = $bindable(false) }: { open: boolean } = $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let busy = $state(false);

  $effect(() => {
    if (open) dlg?.showModal();
    else dlg?.close();
  });

  /**
   * `mint_blind_batch` needs a real, distinct reveal commitment per slot —
   * same as an ordinary mint, just N of them (see `MintForm.svelte`). A
   * `batchSize`-only form can't actually call this extern. This demo form
   * generates each slot's attributes from one shared template plus a slot
   * number rather than asking for N full attribute rows — the minter's
   * secret pre-shuffle (see DESIGN.md step 1) is exactly this: which real
   * attributes end up at which slot is never written down anywhere
   * `validate()` can see, so a template + index is a legitimate way to
   * produce distinct-but-related items, not a shortcut around the design.
   */
  let batchSize = $state(10);
  let namePrefix = $state("Mystery Item");
  let imageUriTemplate = $state(""); // "{i}" is replaced with the 0-based slot index

  function randomSalt(): Uint8Array {
    const salt = new Uint8Array(32);
    crypto.getRandomValues(salt);
    return salt;
  }

  function randomSeed(): Uint8Array {
    const seed = new Uint8Array(32);
    crypto.getRandomValues(seed);
    return seed;
  }

  async function submit() {
    const n = Math.max(1, Math.min(10_000, Math.floor(batchSize)));
    busy = true;
    try {
      const commitments = Array.from({ length: n }, (_, i) => {
        const attributes: NftAttribute[] = [
          { trait_type: "name", value: `${namePrefix.trim() || "Mystery Item"} #${i + 1}` },
        ];
        const imageUri = imageUriTemplate.trim().replace("{i}", String(i));
        return prepareCommitment(attributes, imageUri, randomSalt());
      });
      const mintSeed = randomSeed();

      const result = await app.mintBlindBatch(commitments, mintSeed);
      if (result) {
        // No on-chain way to recover mint_seed — the minter's own client
        // must hold onto it, keyed by batch hash, until every purchase
        // against this batch is resolved. sessionStorage is a demo-only
        // stand-in; see docs/blind-box-collections.md's UI-storage note —
        // a real minter needs durable, backed-up storage, same category as
        // saltStore's reveal commitments.
        sessionStorage.setItem(`blind-seed:${toHex(result.batch)}`, toHex(mintSeed));
        open = false;
      }
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(440px, 88vw)">
    <h3>Mint a blind-box batch</h3>
    <p class="faint small">
      Mints <code>{batchSize}</code> sealed NFTs. Each stays hidden until a
      buyer's purchase is resolved and you reveal it individually (same
      Reveal flow as an ordinary mint).
    </p>

    <label class="field">
      <span>Batch size</span>
      <input type="number" min="1" max="10000" bind:value={batchSize} disabled={busy} />
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
