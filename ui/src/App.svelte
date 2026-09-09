<script lang="ts">
  import { onMount } from "svelte";
  import { AppState, setCurrentAppState } from "$lib/appState.svelte";
  import { connectHolochain } from "$lib/holochain";
  import { toast } from "$lib/toast.svelte";
  import ActivityFeed from "./components/ActivityFeed.svelte";
  import AgentBadge from "./components/AgentBadge.svelte";
  import MintDialog from "./components/MintDialog.svelte";
  import NftDetail from "./components/NftDetail.svelte";
  import Toaster from "./components/Toaster.svelte";
  import WalletGrid from "./components/WalletGrid.svelte";

  let phase = $state<"connecting" | "ready" | "error">("connecting");
  let errMsg = $state("");
  let app = $state<AppState | null>(null);
  let view = $state<"collection" | "activity">("collection");
  let showMint = $state(false);

  function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
    return Promise.race([
      p,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms),
      ),
    ]);
  }

  async function connect() {
    phase = "connecting";
    errMsg = "";
    app?.dispose();
    app = null;
    setCurrentAppState(null);
    try {
      const client = await withTimeout(connectHolochain(), 15000, "conductor connection");
      const s = new AppState(client, client.myPubKey);
      setCurrentAppState(s);
      app = s;
      phase = "ready";
      s.init();
    } catch (e) {
      errMsg = String(e);
      phase = "error";
    }
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
    <div class="row" style="gap: 0.6rem">
      <span class="dot"></span>
      <strong>{a.collection?.name ?? "P2P Art Hub"}</strong>
      {#if a.collection}<span class="pill">{a.collection.symbol}</span>{/if}
      {#if a.isMinter}<span class="pill cyan">minter</span>{/if}
    </div>

    <nav class="nav">
      <button class:active={view === "collection"} onclick={() => (view = "collection")}>
        Collection
      </button>
      <button disabled title="needs a get_all_nfts coordinator fn">Discover</button>
      <button class:active={view === "activity"} onclick={() => (view = "activity")}>
        Activity
        {#if a.inbox.length}<span class="count">{a.inbox.length}</span>{/if}
      </button>
    </nav>

    <div class="row" style="gap: 0.75rem">
      <span class="peer" class:syncing={a.peers === null || a.peers === 0}>
        <span class="dot"></span>
        {#if a.peers === null}
          syncing…
        {:else}
          {a.peers} peer{a.peers === 1 ? "" : "s"}
        {/if}
      </span>
      <AgentBadge agent={a.me} size={22} />
      <button class="primary" onclick={() => (showMint = true)}>+ Mint</button>
    </div>
  </header>

  <main class="workspace">
    {#if view === "collection"}
      <WalletGrid onmint={() => (showMint = true)} />
    {:else}
      <ActivityFeed />
    {/if}
  </main>

  {#if a.detailNft}<NftDetail />{/if}
  {#if showMint}<MintDialog onclose={() => (showMint = false)} />{/if}
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
</style>
