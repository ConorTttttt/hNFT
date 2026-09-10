<script lang="ts">
  import type { ActionHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, relativeFuture, tsToMs } from "$lib/format";
  import { toast } from "$lib/toast.svelte";

  const app = useAppState();

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  // per-NFT pasted secret blob + busy flag
  let blobs = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);

  interface SecretBlob {
    nft?: string;
    salt: number[] | string;
    attributes: { trait_type: string; value: string }[];
    image_uri: string;
  }

  function parseSecret(raw: string): {
    salt: Uint8Array;
    attributes: { trait_type: string; value: string }[];
    imageUri: string;
  } {
    const j = JSON.parse(raw) as SecretBlob;
    const salt =
      typeof j.salt === "string"
        ? Uint8Array.from(atob(j.salt), (c) => c.charCodeAt(0))
        : Uint8Array.from(j.salt);
    if (salt.length !== 32) throw new Error("salt must be 32 bytes");
    if (!Array.isArray(j.attributes)) throw new Error("attributes missing");
    if (typeof j.image_uri !== "string") throw new Error("image_uri missing");
    return { salt, attributes: j.attributes, imageUri: j.image_uri };
  }

  async function reveal(nft: ActionHash) {
    const key = b64(nft);
    const raw = (blobs[key] ?? "").trim();
    if (!raw) {
      toast.error("Paste the reveal secret the minter gave you first");
      return;
    }
    let secret;
    try {
      secret = parseSecret(raw);
    } catch (e) {
      toast.error(`That doesn't look like a reveal secret: ${String(e)}`);
      return;
    }
    busy = key;
    try {
      await app.revealAsDelegate(nft, secret);
      blobs = { ...blobs, [key]: "" };
    } finally {
      busy = null;
    }
  }
</script>

{#if app.delegatedReveals.length}
  <div class="panel stack">
    <div class="row between">
      <h3>Reveals delegated to you <span class="faint">· {app.delegatedReveals.length}</span></h3>
      <span class="faint small">you may reveal these once their deadline passes</span>
    </div>
    <div class="stack">
      {#each app.delegatedReveals as d (b64(d.nft))}
        {@const revealByMs = tsToMs(d.reveal_by_micros)}
        {@const ready = now >= revealByMs}
        {@const key = b64(d.nft)}
        <div class="row-item stack tight">
          <div class="row between wrap" style="gap: 0.5rem">
            <span class="mono small truncate">{key}</span>
            {#if d.defaulted}
              <span class="pill danger">reveal defaulted</span>
            {:else}
              <span class="pill" class:warn={ready}>
                {ready ? "revealable now" : `revealable ${relativeFuture(revealByMs)}`}
              </span>
            {/if}
          </div>
          <textarea
            class="mono"
            rows="3"
            placeholder="Paste the reveal secret the minter handed you (JSON)"
            bind:value={blobs[key]}
          ></textarea>
          <div class="row end">
            <button
              class="primary tiny"
              disabled={!ready || busy === key}
              onclick={() => reveal(d.nft)}
            >
              {busy === key ? "Revealing…" : "Reveal on the minter's behalf"}
            </button>
          </div>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .row-item {
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
  textarea {
    width: 100%;
    resize: vertical;
    font-size: 0.78rem;
  }
</style>
