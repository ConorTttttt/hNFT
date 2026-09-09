<script lang="ts">
  import type { AgentPubKey } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, shortHash } from "$lib/format";
  import { toast } from "$lib/toast.svelte";
  import Identicon from "./Identicon.svelte";

  let {
    agent,
    size = 22,
    label,
  }: { agent: AgentPubKey; size?: number; label?: string } = $props();

  const app = useAppState();
  const text = $derived(
    label ?? (app.isMe(agent) ? "you" : shortHash(agent)),
  );

  async function copy(e: MouseEvent) {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(b64(agent));
      toast.info("Agent key copied");
    } catch {
      toast.error("Clipboard unavailable");
    }
  }
</script>

<button class="agent" onclick={copy} title={b64(agent)}>
  <Identicon seed={agent} {size} />
  <span class="mono">{text}</span>
</button>

<style>
  .agent {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.2rem 0.4rem 0.2rem 0.2rem;
    border: 1px solid transparent;
    border-radius: 999px;
    background: transparent;
    color: inherit;
    font-size: 0.82rem;
  }
  .agent:hover {
    border-color: var(--border-strong);
    background: var(--panel-2);
  }
</style>
