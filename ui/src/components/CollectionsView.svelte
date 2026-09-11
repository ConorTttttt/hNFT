<script lang="ts">
  import type { ActionHash, AgentPubKey } from "@holochain/client";
  import { decodeEntry, recordActionHash, type CollectionEntry } from "@p2p-art-marketplace/client";
  import { useAppState, type CollectionSummary } from "$lib/appState.svelte";
  import { b64 } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";
  import CollectionRow from "./CollectionRow.svelte";

  const app = useAppState();

  let name = $state("");
  let description = $state("");
  let creating = $state(false);

  async function create() {
    const n = name.trim();
    if (!n) return;
    creating = true;
    try {
      await app.createCollection(n, description.trim());
      name = "";
      description = "";
    } catch {
      // toasted by createCollection
    } finally {
      creating = false;
    }
  }

  let directory = $state<{ collection: ActionHash; curator: AgentPubKey; name: string }[]>([]);
  let directoryLoading = $state(false);
  let directoryLoaded = false;

  async function loadDirectory() {
    directoryLoading = true;
    try {
      const records = await app.social.allCollections();
      directory = records.map((r) => ({
        collection: recordActionHash(r),
        curator: r.signed_action.hashed.content.header.author,
        name: decodeEntry<CollectionEntry>(r).name,
      }));
      directoryLoaded = true;
    } catch {
      directory = [];
    } finally {
      directoryLoading = false;
    }
  }

  let showDirectory = $state(false);
  function toggleDirectory() {
    showDirectory = !showDirectory;
    if (showDirectory && !directoryLoaded) void loadDirectory();
  }

  async function remove(c: CollectionSummary) {
    if (!confirm(`Delete "${c.name}"?`)) return;
    await app.deleteCollection(c.collection);
  }
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="panel stack">
    <h3>New collection</h3>
    <div class="row wrap" style="gap: 0.5rem">
      <input class="grow" bind:value={name} placeholder="Name" disabled={creating} />
      <input class="grow" bind:value={description} placeholder="Description (optional)" disabled={creating} />
      <button class="primary" onclick={create} disabled={creating || !name.trim()}>
        {creating ? "…" : "Create"}
      </button>
    </div>
  </div>

  <div class="stack">
    <h3>
      My collections {#if app.myCollections.length}<span class="faint">· {app.myCollections.length}</span>{/if}
    </h3>
    {#if !app.myCollections.length}
      <div class="empty small">You haven't created any collections yet.</div>
    {:else}
      <div class="stack tight">
        {#each app.myCollections as c (b64(c.collection))}
          <CollectionRow collection={c} onremove={() => remove(c)} />
        {/each}
      </div>
    {/if}
  </div>

  <div class="stack">
    <button class="ghost" onclick={toggleDirectory}>
      {showDirectory ? "Hide" : "Browse"} all collections
    </button>
    {#if showDirectory}
      {#if directoryLoading}
        <p class="muted small">Loading…</p>
      {:else if !directory.length}
        <p class="muted small">No collections yet.</p>
      {:else}
        <div class="stack tight">
          {#each directory as d (b64(d.collection))}
            <div class="row between directory-row">
              <span>{d.name}</span>
              <AgentBadge agent={d.curator} size={16} />
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .directory-row {
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
</style>
