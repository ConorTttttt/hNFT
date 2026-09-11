<script lang="ts">
  import type { AgentPubKey } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";

  let { agent }: { agent: AgentPubKey } = $props();

  const app = useAppState();
  const following = $derived(app.isFollowing(agent));
  let busy = $state(false);

  async function toggle(e: MouseEvent) {
    e.stopPropagation();
    busy = true;
    try {
      await app.toggleFollow(agent);
    } finally {
      busy = false;
    }
  }
</script>

<button class="ghost tiny" class:following onclick={toggle} disabled={busy}>
  {following ? "Following" : "+ Follow"}
</button>

<style>
  .following {
    color: var(--text-dim);
  }
</style>
