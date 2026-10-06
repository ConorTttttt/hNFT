<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { decodeNftManifest, type GenerationRecord } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import {
    canReveal,
    deriveGenerativeStatus,
    GENERATIVE_STATUS_LABEL,
    type GenerativeStatus,
  } from "$lib/generativeStatus";

  /**
   * The single generative-NFT lifecycle view (G5b) — used from both
   * `GenerativeDraftShelf` (wallet) and `NftDetail`, so the state machine
   * exists in exactly one place. See `docs/generative-collections.md`.
   *
   * The user never enters a seed, randomness, a proof, an event id, or a
   * trait — every one of those is either protocol-derived or (for the
   * dev-insecure provider) generated client-side only to exercise the flow,
   * never presented as real entropy.
   */
  let { nft, title }: { nft: ActionHash; title?: string } = $props();

  const app = useAppState();

  let loading = $state(true);
  let manifestStatus = $state<"sealed" | "revealed">("sealed");
  let resultHash = $state<ActionHash | null>(null);
  let providerId = $state<string | null>(null);
  let eventId = $state<number[] | null>(null);
  let record = $state<GenerationRecord | null>(null);
  let actionPending = $state(false);
  let imageUri = $state("");

  const status = $derived<GenerativeStatus>(
    deriveGenerativeStatus({
      manifestStatus,
      hasRandomness: resultHash !== null,
      hasRecord: record !== null,
      actionPending,
    }),
  );

  async function load() {
    loading = true;
    try {
      const [manifestRec, randomness, rec] = await Promise.all([
        app.nft.getNft(nft),
        app.nft.getGenerationRandomness(nft),
        app.nft.getGenerationRecord(nft),
      ]);
      manifestStatus = manifestRec ? decodeNftManifest(manifestRec).payload.status : "sealed";
      if (randomness) {
        resultHash = randomness[0];
        providerId = randomness[1].provider_id;
        eventId = randomness[1].event_id;
      } else {
        resultHash = null;
        providerId = null;
        eventId = null;
      }
      record = rec;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void nft; // re-run when the panel is reused for a different NFT
    void load();
  });

  async function requestRandomness() {
    actionPending = true;
    try {
      const result = await app.requestGenerationRandomness(nft);
      if (result) resultHash = result;
    } finally {
      actionPending = false;
      await load();
    }
  }

  async function generateTraits() {
    if (!resultHash) return;
    actionPending = true;
    try {
      await app.createGenerationRecordFor(nft, resultHash);
    } finally {
      actionPending = false;
      await load();
    }
  }

  async function reveal() {
    actionPending = true;
    try {
      const ok = await app.revealGenerativeNft(nft, imageUri.trim());
      if (ok) await load();
    } finally {
      actionPending = false;
    }
  }
</script>

<div class="stack tight generative-panel">
  {#if title}<strong>{title}</strong>{/if}
  <div class="row wrap" style="gap: 0.4rem; align-items: center">
    <span class="pill" class:cyan={status === "ready-to-reveal" || status === "revealed"}>
      {GENERATIVE_STATUS_LABEL[status]}
    </span>
    {#if loading}<span class="faint small">refreshing…</span>{/if}
  </div>

  {#if status === "revealed"}
    <span class="faint small">Revealed — see its attributes above.</span>
  {:else if status === "unrevealed"}
    <p class="faint small">
      This NFT's traits don't exist yet — they're derived from randomness, not
      chosen by anyone. Provider: <code>{app.collection?.randomness?.provider_id ?? "none"}</code>.
    </p>
    {#if app.collection?.randomness?.provider_id === "dev-insecure"}
      <p class="warn small">
        <strong>dev-insecure</strong> is a public, forgeable proof — development/testing
        only. It provides no real unpredictability and must never be used for a
        production collection.
      </p>
    {/if}
    <button class="primary" onclick={requestRandomness} disabled={actionPending}>
      {actionPending ? "Requesting…" : "Request randomness"}
    </button>
  {:else if status === "randomness-pending"}
    <p class="faint small">Requesting randomness…</p>
  {:else if status === "randomness-verified"}
    <p class="faint small">
      Randomness verified{providerId ? ` (provider ${providerId}${eventId ? `, event ${b64(Uint8Array.from(eventId))}` : ""})` : ""}.
      Traits haven't been derived yet.
    </p>
    <button class="primary" onclick={generateTraits} disabled={actionPending}>
      {actionPending ? "Generating…" : "Generate traits"}
    </button>
  {:else if status === "traits-generated" || status === "ready-to-reveal"}
    {#if record}
      <div class="stack tight">
        <span class="faint small">Generation v{record.generation_version} — generated traits:</span>
        <dl class="defs">
          {#each record.generated_traits.traits as t (t.category_id)}
            <dt>{t.category_id}</dt>
            <dd>{t.trait_id}</dd>
          {/each}
        </dl>
      </div>
    {/if}
    <label class="field">
      <span>Image URI (not bound to the traits)</span>
      <input bind:value={imageUri} placeholder="https://…  ·  ipfs://…  ·  data:…" />
    </label>
    <button class="primary" onclick={reveal} disabled={!canReveal(status) || actionPending}>
      {actionPending ? "Revealing…" : "Reveal"}
    </button>
  {/if}
</div>

<style>
  .generative-panel .warn {
    color: var(--warn, #a86a00);
  }
</style>
