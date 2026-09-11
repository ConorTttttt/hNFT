<script lang="ts">
  import { decodeNftManifest, type CollectionItemView } from "@p2p-art-marketplace/client";
  import { NFT_ROLE, useAppState, type CollectionSummary } from "$lib/appState.svelte";
  import { b64, nftTitle } from "$lib/format";

  let { collection, onremove }: { collection: CollectionSummary; onremove: () => void } =
    $props();

  const app = useAppState();

  let open = $state(false);
  let items = $state<CollectionItemView[]>([]);
  let titles = $state<Record<string, string>>({});
  let itemsLoaded = false;
  let itemsLoading = $state(false);

  let editing = $state(false);
  let editName = $state("");
  let editDescription = $state("");
  let saving = $state(false);

  function toggleEdit() {
    editing = !editing;
    if (editing) {
      editName = collection.name;
      editDescription = collection.description;
    }
  }

  async function loadItems() {
    itemsLoading = true;
    try {
      items = await app.social.collectionItems(collection.collection);
      const resolved = await Promise.all(
        items
          .filter((i) => i.target_role === NFT_ROLE)
          .map(async (i) => {
            const rec = await app.nft.getNft(i.target).catch(() => null);
            return [b64(i.target), rec ? nftTitle(decodeNftManifest(rec)) : "unknown"] as const;
          }),
      );
      titles = Object.fromEntries(resolved);
      itemsLoaded = true;
    } finally {
      itemsLoading = false;
    }
  }

  function toggle() {
    open = !open;
    if (open && !itemsLoaded) void loadItems();
  }

  async function save() {
    saving = true;
    try {
      await app.updateCollection(collection.collection, editName.trim(), editDescription.trim());
      editing = false;
    } catch {
      // toasted by updateCollection
    } finally {
      saving = false;
    }
  }

  async function removeItem(i: CollectionItemView) {
    await app.removeFromCollection(collection.collection, i.target);
    itemsLoaded = false;
    await loadItems();
  }
</script>

<div class="row-card">
  <div class="row between">
    <button class="link grow" onclick={toggle}>
      <strong>{collection.name}</strong>
      {#if items.length}<span class="faint small">· {items.length}</span>{/if}
    </button>
    <button class="ghost tiny" onclick={toggleEdit}>
      {editing ? "Cancel" : "Edit"}
    </button>
    <button class="ghost tiny" onclick={onremove}>Delete</button>
  </div>

  {#if collection.description && !editing}
    <p class="faint small">{collection.description}</p>
  {/if}

  {#if editing}
    <div class="stack tight" style="margin-top: 0.4rem">
      <input bind:value={editName} placeholder="Name" disabled={saving} />
      <input bind:value={editDescription} placeholder="Description" disabled={saving} />
      <div class="row end">
        <button class="primary tiny" onclick={save} disabled={saving || !editName.trim()}>
          {saving ? "…" : "Save"}
        </button>
      </div>
    </div>
  {/if}

  {#if open}
    <div class="stack tight" style="margin-top: 0.5rem">
      {#if itemsLoading}
        <p class="muted small">Loading…</p>
      {:else if !items.length}
        <p class="muted small">No items yet.</p>
      {:else}
        {#each items as i (b64(i.target))}
          <div class="row between item">
            <span class="truncate">{titles[b64(i.target)] ?? "(other collection)"}</span>
            <button class="ghost tiny" onclick={() => removeItem(i)}>Remove</button>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .row-card {
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
  .link {
    text-align: left;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 0;
  }
  .item {
    padding: 0.3rem 0.4rem;
  }
</style>
