<script lang="ts">
  import {
    DEV_INSECURE_WARNING,
    isValidGenerationConfig,
    validateGenerationConfig,
    validateWhitelistEntries,
    type CreatedNftCollection,
    type GenerationConfig,
    type MintPolicy,
    type WhitelistEntry,
  } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import { MINT_POLICY_LABEL } from "$lib/collectionCards";
  import GenerationTraitEditor from "./GenerationTraitEditor.svelte";

  /**
   * Runtime NFT-collection creation (G5b) via `createCloneCell` — NOT a
   * generated/exported `dna.yaml`. `network_seed` is generated and hidden
   * inside `createNftCollection`; nothing here surfaces or asks for it.
   */
  let { open = $bindable(false), oncreated }: { open: boolean; oncreated: (c: CreatedNftCollection) => void } =
    $props();

  const app = useAppState();
  let dlg = $state<HTMLDialogElement>();
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) dlg?.showModal();
    else dlg?.close();
  });

  let name = $state("");
  let symbol = $state("");
  let maxSupply = $state(0);
  let transferWindowSecs = $state(300);

  /** Phase J.2: PUBLIC / CREATOR_ONLY / WHITELIST. Immutable once created —
   * there is no post-creation whitelist editing (see the J.2 report). */
  let mintPolicy = $state<MintPolicy>("creator_only");
  let whitelist = $state<WhitelistEntry[]>([{ agent: "", allowance: 1 }]);

  const restrictMintingToMe = $derived(mintPolicy === "creator_only");
  const cleanedWhitelist = $derived(
    whitelist
      .map((e) => ({ agent: e.agent.trim(), allowance: Number(e.allowance) }))
      .filter((e) => e.agent !== ""),
  );
  const whitelistIssues = $derived(
    mintPolicy === "whitelist" ? validateWhitelistEntries(cleanedWhitelist, maxSupply) : [],
  );
  const allocatedAllowance = $derived(cleanedWhitelist.reduce((sum, e) => sum + (e.allowance || 0), 0));

  function addWhitelistRow() {
    whitelist = [...whitelist, { agent: "", allowance: 1 }];
  }
  function removeWhitelistRow(i: number) {
    whitelist = whitelist.filter((_, j) => j !== i);
  }

  /** Mutually exclusive: this UI does not offer a blind-generative collection. */
  type Mode = "normal" | "generative" | "blind";
  let mode = $state<Mode>("normal");

  let generation = $state<GenerationConfig>({
    generation_version: 1,
    categories: [],
  });

  const needsRandomness = $derived(mode === "generative" || mode === "blind");
  const genIssues = $derived(mode === "generative" ? validateGenerationConfig(generation) : []);
  const canSubmit = $derived(
    name.trim() !== "" &&
      symbol.trim() !== "" &&
      (mode !== "generative" || isValidGenerationConfig(generation)) &&
      (mintPolicy !== "whitelist" || (cleanedWhitelist.length > 0 && whitelistIssues.length === 0)),
  );

  function preview() {
    return {
      name: name.trim(),
      symbol: symbol.trim(),
      max_supply: maxSupply,
      transfer_window_secs: transferWindowSecs,
      minter_pubkey: restrictMintingToMe ? b64(app.me) : null,
      whitelist: mintPolicy === "whitelist" ? cleanedWhitelist : null,
      collection_id: needsRandomness ? "<generated 64-hex id>" : null,
      randomness: needsRandomness ? { provider_id: "dev-insecure", verifier_params: "" } : null,
      generation: mode === "generative" ? generation : null,
    };
  }

  async function submit() {
    if (!canSubmit) return;
    busy = true;
    error = null;
    try {
      const created = await app.createNftCollection({
        name: name.trim(),
        symbol: symbol.trim(),
        maxSupply,
        transferWindowSecs,
        minterPubkey: restrictMintingToMe ? b64(app.me) : undefined,
        whitelist: mintPolicy === "whitelist" ? cleanedWhitelist : undefined,
        generation: mode === "generative" ? generation : undefined,
        randomness: needsRandomness ? { providerId: "dev-insecure" } : undefined,
      });
      if (created) {
        open = false;
        oncreated(created);
      } else {
        error = "Collection creation failed — see the toast for details.";
      }
    } finally {
      busy = false;
    }
  }
</script>

<dialog bind:this={dlg} onclose={() => (open = false)}>
  <div class="stack" style="min-width: min(640px, 92vw); max-height: 85vh; overflow: auto">
    <h3>New NFT collection</h3>
    <p class="faint small">
      Creates a runtime collection cell (a Holochain clone cell) — not a new app
      build. Once created, its configuration is fixed for the life of this cell.
    </p>

    <label class="field">
      <span>Name</span>
      <input bind:value={name} disabled={busy} />
    </label>
    <label class="field">
      <span>Symbol</span>
      <input bind:value={symbol} disabled={busy} maxlength="16" />
    </label>
    <div class="row wrap" style="gap: 1rem">
      <label class="field">
        <span>Max supply (0 = unlimited)</span>
        <input type="number" min="0" bind:value={maxSupply} disabled={busy} />
      </label>
      <label class="field">
        <span>Transfer window (seconds)</span>
        <input type="number" min="1" bind:value={transferWindowSecs} disabled={busy} />
      </label>
    </div>
    <div class="field">
      <span>Minting</span>
      <div class="stack tight">
        <label class="row" style="gap: 0.5rem; align-items: flex-start">
          <input type="radio" name="mint-policy" value="public" bind:group={mintPolicy} style="width: auto; margin-top: 0.2rem" disabled={busy} />
          <span class="stack tight" style="gap: 0.1rem">
            <span class="small">{MINT_POLICY_LABEL.public}</span>
            <span class="faint small">
              Anyone can mint. Note: without a single fixed minter, a supply cap
              cannot be enforced (see the panel below).
            </span>
          </span>
        </label>
        <label class="row" style="gap: 0.5rem; align-items: flex-start">
          <input type="radio" name="mint-policy" value="creator_only" bind:group={mintPolicy} style="width: auto; margin-top: 0.2rem" disabled={busy} />
          <span class="stack tight" style="gap: 0.1rem">
            <span class="small">{MINT_POLICY_LABEL.creator_only}</span>
            <span class="faint small">Only you (the collection creator) can mint.</span>
          </span>
        </label>
        <label class="row" style="gap: 0.5rem; align-items: flex-start">
          <input type="radio" name="mint-policy" value="whitelist" bind:group={mintPolicy} style="width: auto; margin-top: 0.2rem" disabled={busy} />
          <span class="stack tight" style="gap: 0.1rem">
            <span class="small">{MINT_POLICY_LABEL.whitelist}</span>
            <span class="faint small">
              Only approved agents can mint, each subject to their own allowance.
            </span>
          </span>
        </label>
      </div>
    </div>

    {#if mintPolicy === "whitelist"}
      <div class="panel stack tight">
        <div class="row between wrap">
          <span class="small"><strong>Whitelist</strong></span>
          <span class="faint small">
            Allocated mint allowance: {allocatedAllowance}{maxSupply > 0 ? ` / ${maxSupply}` : ""}
          </span>
        </div>
        <p class="faint small" style="margin: 0">
          This is fixed once the collection is created — there is no editing the
          whitelist afterward yet.
        </p>
        {#each whitelist as entry, i (i)}
          <div class="row" style="gap: 0.5rem">
            <input
              class="grow mono"
              bind:value={entry.agent}
              placeholder="Agent public key (uhCAk…)"
              disabled={busy}
            />
            <input
              type="number"
              min="1"
              style="max-width: 6rem"
              bind:value={entry.allowance}
              disabled={busy}
            />
            <button
              class="ghost tiny"
              onclick={() => removeWhitelistRow(i)}
              disabled={busy || whitelist.length === 1}
              aria-label="remove entry">✕</button
            >
          </div>
        {/each}
        <div><button class="ghost tiny" onclick={addWhitelistRow} disabled={busy}>+ agent</button></div>
        {#if whitelistIssues.length}
          <ul class="error small" style="margin: 0.3rem 0 0; padding-left: 1.1rem">
            {#each whitelistIssues as issue}<li>{issue}</li>{/each}
          </ul>
        {/if}
      </div>
    {/if}

    <div class="divider"></div>
    <div class="row wrap" style="gap: 1rem">
      <label class="row" style="gap: 0.4rem">
        <input type="radio" name="mode" value="normal" bind:group={mode} style="width: auto" disabled={busy} />
        <span class="small">Normal — manually authored NFTs</span>
      </label>
      <label class="row" style="gap: 0.4rem">
        <input type="radio" name="mode" value="generative" bind:group={mode} style="width: auto" disabled={busy} />
        <span class="small">Generative — traits derived from randomness</span>
      </label>
      <label class="row" style="gap: 0.4rem">
        <input type="radio" name="mode" value="blind" bind:group={mode} style="width: auto" disabled={busy} />
        <span class="small">Fixed-inventory blind boxes</span>
      </label>
    </div>

    {#if mode === "blind"}
      <p class="faint small">
        Blind batches are created afterward from the "Blind boxes" tab; this only
        sets up the collection's <code>collection_id</code> and randomness spec.
      </p>
    {/if}

    {#if needsRandomness}
      <div class="panel stack tight">
        <span class="small"><strong>Randomness provider:</strong> <code>dev-insecure</code></span>
        <p class="error small" style="margin: 0">{DEV_INSECURE_WARNING}</p>
      </div>
    {/if}

    {#if mode === "generative"}
      <GenerationTraitEditor bind:config={generation} />
      {#if genIssues.length}
        <p class="error small">{genIssues.length} configuration issue(s) — fix them above to continue.</p>
      {/if}
    {/if}

    <div class="divider"></div>
    <details>
      <summary class="small">Configuration preview</summary>
      <pre class="mono small" style="white-space: pre-wrap">{JSON.stringify(preview(), null, 2)}</pre>
    </details>

    {#if error}<p class="error small">{error}</p>{/if}

    <div class="row between">
      <button class="ghost" onclick={() => (open = false)} disabled={busy}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy || !canSubmit}>
        {busy ? "Creating…" : "Create collection"}
      </button>
    </div>
  </div>
</dialog>
