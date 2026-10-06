<script lang="ts">
  import { onMount } from "svelte";
  import type { AppWebsocket } from "@holochain/client";
  import { AppState, setCurrentAppState } from "$lib/appState.svelte";
  import { buildCollectionCards, collectionKind, COLLECTION_KIND_LABEL } from "$lib/collectionCards";
  import { listRegisteredCollections, type RegisteredCollection } from "$lib/collectionRegistry";
  import { buildNetworkCollectionCards, type NetworkCollectionCardData } from "$lib/discoveredCollectionCards";
  import { buildNetworkNftCards } from "$lib/networkNftCards";
  import { connectHolochain } from "$lib/holochain";
  import { describeMintUnavailability } from "$lib/mintStatusText";
  import { theme, toggleTheme } from "$lib/theme.svelte";
  import ActivityFeed from "./components/ActivityFeed.svelte";
  import AgentBadge from "./components/AgentBadge.svelte";
  import BlindBoxesView from "./components/BlindBoxesView.svelte";
  import CollectionsBrowser from "./components/CollectionsBrowser.svelte";
  import CollectionOverview from "./components/CollectionOverview.svelte";
  import CollectionsView from "./components/CollectionsView.svelte";
  import CreateCollectionDialog from "./components/CreateCollectionDialog.svelte";
  import DiscoverGrid from "./components/DiscoverGrid.svelte";
  import MintDialog from "./components/MintDialog.svelte";
  import NetworkMarketplace from "./components/NetworkMarketplace.svelte";
  import NftDetail from "./components/NftDetail.svelte";
  import Toaster from "./components/Toaster.svelte";
  import WalletGrid from "./components/WalletGrid.svelte";

  let phase = $state<"connecting" | "ready" | "error">("connecting");
  let errMsg = $state("");
  let client = $state<AppWebsocket | null>(null);
  let app = $state<AppState | null>(null);
  let showMint = $state(false);
  let showCreateCollection = $state(false);
  /** `undefined` = the provisioned "nft" role (this app's default collection). */
  let selectedCloneId = $state<string | undefined>(undefined);
  let registeredCollections = $state<RegisteredCollection[]>(listRegisteredCollections());

  /**
   * "browse" (Explore — the Collections browser) is the PRIMARY discovery
   * surface; "collection" is the active collection's screen (Overview/Items/
   * My NFTs/…), reached BY entering a collection. The header dropdown inside
   * "collection" mode remains a secondary context switcher.
   */
  let appMode = $state<"browse" | "collection" | "marketplace">("browse");

  const collectionCards = $derived(
    app
      ? buildCollectionCards(registeredCollections, {
          cloneId: selectedCloneId,
          collectionInfo: app.collection,
          itemCount: app.discoverLoaded ? app.discover.length : null,
          listingCount: app.listingsLoaded ? app.listings.length : null,
          canMint: app.isMinter,
        })
      : [],
  );

  const networkCollectionCards = $derived(
    app ? buildNetworkCollectionCards(app.discoveredCards, new Set(app.localCloneDnaHashes.values())) : [],
  );
  const networkNftCards = $derived(app ? buildNetworkNftCards(app.networkNftReferences) : []);

  // Load the active collection's item/listing sets once (no view change), so
  // its card can show a real item count.
  $effect(() => {
    if (appMode === "browse" && app) void app.ensureMarketData();
  });

  // Network-wide discovery (Phase J.3): loaded once entering the browser,
  // independent of which collection (if any) is currently active.
  $effect(() => {
    if (appMode === "browse" && app && !app.discoveredLoaded) void app.loadDiscoveredCollections();
  });

  /** Reconstruct a discovered collection's clone cell, then enter it. */
  async function openDiscovered(card: NetworkCollectionCardData) {
    if (!app) return;
    const opened = await app.openDiscovered(card.source);
    if (opened) {
      registeredCollections = listRegisteredCollections();
      enterCollection(opened.cloneId);
    }
  }

  function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
    return Promise.race([
      p,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms),
      ),
    ]);
  }

  /** Rebuild/rebind AppState for `roleName` — never a shared multi-collection instance. */
  function bindCollection(roleName: string | undefined) {
    if (!client) return;
    app?.dispose();
    setCurrentAppState(null);
    const s = new AppState(client, client.myPubKey, roleName);
    setCurrentAppState(s);
    app = s;
    s.init();
  }

  async function connect() {
    phase = "connecting";
    errMsg = "";
    app?.dispose();
    app = null;
    setCurrentAppState(null);
    try {
      client = await withTimeout(connectHolochain(), 15000, "conductor connection");
      phase = "ready";
      bindCollection(selectedCloneId);
    } catch (e) {
      errMsg = String(e);
      phase = "error";
    }
  }

  function switchCollection(cloneId: string | undefined) {
    selectedCloneId = cloneId;
    bindCollection(cloneId);
  }

  /** From the Collections browser: bind the chosen collection (if not already
   * active) and enter its detail screen. */
  function enterCollection(cloneId: string | undefined) {
    if (cloneId !== selectedCloneId) switchCollection(cloneId);
    appMode = "collection";
  }

  function onCollectionCreated(created: { cloneId: string }) {
    registeredCollections = listRegisteredCollections();
    enterCollection(created.cloneId);
  }

  onMount(() => {
    connect();
    return () => {
      app?.dispose();
      setCurrentAppState(null);
    };
  });
</script>

<Toaster />

{#if phase === "connecting"}
  <main class="center"><p class="muted">Connecting to the conductor…</p></main>
{:else if phase === "error"}
  <main class="center stack" style="max-width: 460px">
    <h3>Can't reach the conductor</h3>
    <p class="error small mono">{errMsg}</p>
    <p class="muted small">
      Launch via <span class="mono">hc-spin</span> (it injects the conductor URL +
      token), or set <span class="mono">VITE_HC_ADMIN_PORT</span> for a packaged
      conductor.
    </p>
    <div><button class="primary" onclick={connect}>Retry</button></div>
  </main>
{:else if app}
  {@const a = app}
  <header>
    <div class="row wrap header-left" style="gap: 0.5rem">
      <span class="dot"></span>
      {#if appMode === "collection" || appMode === "marketplace"}
        <button class="ghost tiny" onclick={() => (appMode = "browse")}>← Explore</button>
      {/if}
      <strong class="truncate header-title">
        {appMode === "browse"
          ? "Explore"
          : appMode === "marketplace"
            ? "Network Marketplace"
            : (a.collection?.name ?? "Collection")}
      </strong>
      {#if appMode === "browse"}
        <button class="ghost tiny" onclick={() => (appMode = "marketplace")}>
          Network Marketplace →
        </button>
      {/if}
      {#if appMode === "collection"}
        <select
          class="collection-switcher"
          title="Switch collection"
          value={selectedCloneId ?? ""}
          onchange={(e) => {
            const v = e.currentTarget.value;
            if (v === "__new__") {
              e.currentTarget.value = selectedCloneId ?? "";
              showCreateCollection = true;
            } else {
              switchCollection(v === "" ? undefined : v);
            }
          }}
        >
          <option value="">{a.collection?.name ?? "Default collection"}</option>
          {#each registeredCollections as c (c.cloneId)}
            <option value={c.cloneId}>{c.name}</option>
          {/each}
          <option value="__new__">+ New collection…</option>
        </select>
        {#if a.collection}<span class="pill">{a.collection.symbol}</span>{/if}
        {#if a.collection}
          {@const kind = collectionKind(a.collection)}
          <span class="pill" class:cyan={kind === "generative"} class:warn={kind === "blind"}>
            {COLLECTION_KIND_LABEL[kind]}
          </span>
        {/if}
      {/if}
    </div>

    {#if appMode === "collection"}
      <nav class="nav">
        <button class:active={a.view === "overview"} onclick={() => a.openOverview()}>
          Overview
        </button>
        <button class:active={a.view === "discover"} onclick={() => a.openDiscover()}>
          Items
        </button>
        <button class:active={a.view === "collection"} onclick={() => (a.view = "collection")}>
          My NFTs
        </button>
        {#if !a.collection?.generation}
          <button class:active={a.view === "blind"} onclick={() => a.openBlindBoxes()}>
            Blind boxes
          </button>
        {/if}
        <button class:active={a.view === "activity"} onclick={() => (a.view = "activity")}>
          Activity
          {#if a.pendingOfferCount}<span class="count">{a.pendingOfferCount}</span>{/if}
        </button>
        <button class:active={a.view === "collections"} onclick={() => (a.view = "collections")}>
          Social collections
        </button>
      </nav>
    {/if}

    <div class="row" style="gap: 0.6rem">
      <span class="peer" class:syncing={a.peers === null || a.peers === 0}>
        <span class="dot"></span>
        {#if a.peers === null}
          syncing…
        {:else}
          {a.peers} peer{a.peers === 1 ? "" : "s"}
        {/if}
      </span>
      <button
        class="ghost tiny"
        title="toggle light / dark"
        onclick={toggleTheme}
        aria-label="toggle theme"
      >
        {theme.value === "dark" ? "☀" : "☾"}
      </button>
      <AgentBadge agent={a.me} size={22} />
      {#if appMode === "collection"}
        {@const mintReason = describeMintUnavailability(a.mintStatus)}
        {#if a.collection?.generation}
          <button
            class="primary"
            onclick={() => a.mintGenerative()}
            disabled={!a.isMinter}
            title={mintReason ?? undefined}
          >
            + Mint
          </button>
        {:else}
          <button
            class="primary"
            onclick={() => (showMint = true)}
            disabled={!a.isMinter}
            title={mintReason ?? undefined}
          >
            + Mint
          </button>
        {/if}
      {:else if appMode === "browse"}
        <button class="primary" onclick={() => (showCreateCollection = true)}>
          + Create collection
        </button>
      {/if}
    </div>
  </header>

  <main class="workspace">
    {#if appMode === "browse"}
      <CollectionsBrowser
        cards={collectionCards}
        networkCards={networkCollectionCards}
        networkLoading={a.discoveredLoading}
        networkNftCards={networkNftCards}
        networkNftsLoading={a.networkNftsLoading}
        networkNftsLoaded={a.networkNftsLoaded}
        networkNftsUnresolvedCount={a.networkNftsUnresolved.length}
        onselect={enterCollection}
        oncreate={() => (showCreateCollection = true)}
        onopendiscovered={openDiscovered}
        onloadnetworkitems={() => a.loadNetworkNfts()}
      />
    {:else if appMode === "marketplace"}
      <NetworkMarketplace onopencollection={enterCollection} />
    {:else if a.view === "overview"}
      <CollectionOverview onitems={() => a.openDiscover()} />
    {:else if a.view === "collection"}
      <WalletGrid onmint={() => (showMint = true)} />
    {:else if a.view === "discover"}
      <DiscoverGrid />
    {:else if a.view === "blind" && !a.collection?.generation}
      <BlindBoxesView />
    {:else if a.view === "collections"}
      <CollectionsView />
    {:else}
      <ActivityFeed />
    {/if}
  </main>

  {#if a.detailNft}<NftDetail />{/if}
  {#if showMint}<MintDialog onclose={() => (showMint = false)} />{/if}
  {#if showCreateCollection}
    <CreateCollectionDialog bind:open={showCreateCollection} oncreated={onCollectionCreated} />
  {/if}
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.7rem 1.25rem;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-elev) 88%, transparent);
    position: sticky;
    top: 0;
    backdrop-filter: blur(10px);
    z-index: 10;
    flex-wrap: wrap;
  }
  /* The left cluster (back/name/switcher/pills) is allowed to wrap onto a
     second intentional row rather than force horizontal overflow; the name
     truncates first so wrapping is a last resort, not the first thing that
     happens on a slightly narrow window. */
  .header-left {
    min-width: 0;
    flex: 1 1 auto;
  }
  .header-title {
    max-width: min(46vw, 320px);
  }
  .collection-switcher {
    width: auto;
    flex: 0 1 200px;
    min-width: 0;
  }
  .workspace {
    max-width: 1180px;
    margin: 0 auto;
    padding: 1.5rem 1.25rem;
  }
  .center {
    min-height: 70vh;
    display: grid;
    place-content: center;
    padding: 2rem;
    text-align: center;
  }

  @media (max-width: 480px) {
    header {
      padding: 0.6rem 0.85rem;
    }
    .workspace {
      padding: 1.1rem 0.85rem;
    }
    .header-title {
      max-width: 55vw;
    }
  }
</style>
