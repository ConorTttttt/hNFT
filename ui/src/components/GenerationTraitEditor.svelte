<script lang="ts">
  import {
    emptyCategory,
    emptyTrait,
    suggestId,
    totalWeight,
    validateGenerationConfig,
    weightPercent,
    zeroWeightTraits,
    type GenerationConfig,
    type TraitCategory,
  } from "@p2p-art-marketplace/client";

  /**
   * Categories/traits/weights editor for a generative collection (G5b). IDs are
   * NEVER generated or changed automatically — `suggestId` only fills an EMPTY
   * id field on request, and never overwrites one the user has already typed.
   * The frozen integrity limits (32 categories, 256 traits/category, 64-byte
   * display names/ids) are mirrored here for immediate feedback only; the
   * authoritative check is `GenerationConfig::validate()` in Rust, re-run at
   * collection creation.
   */
  let { config = $bindable() }: { config: GenerationConfig } = $props();

  const issues = $derived(validateGenerationConfig(config));
  function issuesFor(path: string) {
    return issues.filter((i) => i.path === path || i.path.startsWith(`${path}.`));
  }

  function addCategory() {
    config = { ...config, categories: [...config.categories, emptyCategory()] };
  }
  function removeCategory(i: number) {
    config = { ...config, categories: config.categories.filter((_, j) => j !== i) };
  }
  function moveCategory(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= config.categories.length) return;
    const cats = [...config.categories];
    [cats[i], cats[j]] = [cats[j]!, cats[i]!];
    config = { ...config, categories: cats };
  }
  function setCategory(i: number, patch: Partial<TraitCategory>) {
    const cats = config.categories.map((c, j) => (j === i ? { ...c, ...patch } : c));
    config = { ...config, categories: cats };
  }
  function addTrait(ci: number) {
    setCategory(ci, { traits: [...config.categories[ci]!.traits, emptyTrait()] });
  }
  function removeTrait(ci: number, ti: number) {
    setCategory(ci, { traits: config.categories[ci]!.traits.filter((_, j) => j !== ti) });
  }
  function moveTrait(ci: number, ti: number, dir: -1 | 1) {
    const traits = [...config.categories[ci]!.traits];
    const tj = ti + dir;
    if (tj < 0 || tj >= traits.length) return;
    [traits[ti], traits[tj]] = [traits[tj]!, traits[ti]!];
    setCategory(ci, { traits });
  }
  function setTrait(ci: number, ti: number, patch: Partial<(typeof config.categories)[number]["traits"][number]>) {
    const traits = config.categories[ci]!.traits.map((t, j) => (j === ti ? { ...t, ...patch } : t));
    setCategory(ci, { traits });
  }

  /** Fills an id from its display name — ONLY when the id is currently empty. */
  function suggestCategoryId(i: number) {
    const c = config.categories[i]!;
    if (c.id === "") setCategory(i, { id: suggestId(c.display_name) });
  }
  function suggestTraitId(ci: number, ti: number) {
    const t = config.categories[ci]!.traits[ti]!;
    if (t.id === "") setTrait(ci, ti, { id: suggestId(t.display_name) });
  }
</script>

<div class="stack" style="gap: 1rem">
  {#each config.categories as cat, ci (ci)}
    {@const zeros = zeroWeightTraits(cat)}
    <div class="panel stack tight">
      <div class="row between wrap" style="gap: 0.5rem">
        <span class="faint small">Category {ci + 1}</span>
        <div class="row" style="gap: 0.3rem">
          <button class="ghost tiny" onclick={() => moveCategory(ci, -1)} disabled={ci === 0}>↑</button>
          <button
            class="ghost tiny"
            onclick={() => moveCategory(ci, 1)}
            disabled={ci === config.categories.length - 1}
          >
            ↓
          </button>
          <button class="ghost tiny" onclick={() => removeCategory(ci)}>Remove</button>
        </div>
      </div>
      <div class="row wrap" style="gap: 0.5rem">
        <label class="field grow">
          <span>Display name</span>
          <input
            value={cat.display_name}
            oninput={(e) => setCategory(ci, { display_name: e.currentTarget.value })}
          />
        </label>
        <label class="field grow">
          <span>ID (exact, on-chain)</span>
          <input value={cat.id} oninput={(e) => setCategory(ci, { id: e.currentTarget.value })} />
        </label>
        {#if cat.id === "" && cat.display_name}
          <button class="ghost tiny" onclick={() => suggestCategoryId(ci)}>Suggest from name</button>
        {/if}
      </div>
      {#each issuesFor(`categories[${ci}]`) as issue}
        <span class="error small">{issue.message}</span>
      {/each}

      <div class="stack tight" style="padding-left: 1rem">
        {#each cat.traits as t, ti (ti)}
          <div class="row wrap" style="gap: 0.5rem; align-items: flex-end">
            <label class="field grow">
              <span>Trait name</span>
              <input
                value={t.display_name}
                oninput={(e) => setTrait(ci, ti, { display_name: e.currentTarget.value })}
              />
            </label>
            <label class="field grow">
              <span>ID</span>
              <input value={t.id} oninput={(e) => setTrait(ci, ti, { id: e.currentTarget.value })} />
            </label>
            <label class="field" style="max-width: 8rem">
              <span>Weight</span>
              <input
                type="number"
                min="0"
                value={t.weight}
                oninput={(e) => setTrait(ci, ti, { weight: Math.max(0, Math.floor(Number(e.currentTarget.value) || 0)) })}
              />
            </label>
            <span class="faint small" style="min-width: 3.5rem">{weightPercent(cat, t).toFixed(1)}%</span>
            {#if t.id === "" && t.display_name}
              <button class="ghost tiny" onclick={() => suggestTraitId(ci, ti)}>Suggest id</button>
            {/if}
            <button class="ghost tiny" onclick={() => moveTrait(ci, ti, -1)} disabled={ti === 0}>↑</button>
            <button
              class="ghost tiny"
              onclick={() => moveTrait(ci, ti, 1)}
              disabled={ti === cat.traits.length - 1}
            >
              ↓
            </button>
            <button class="ghost tiny" onclick={() => removeTrait(ci, ti)}>Remove</button>
          </div>
          {#each issuesFor(`categories[${ci}].traits[${ti}]`) as issue}
            <span class="error small">{issue.message}</span>
          {/each}
        {/each}
        <button class="ghost tiny" onclick={() => addTrait(ci)}>+ Trait</button>
        <span class="faint small">
          Total weight: {totalWeight(cat)}
          {#if zeros.length}
            · <span class="warn">never selected: {zeros.map((z) => z.display_name || z.id).join(", ")}</span>
          {/if}
        </span>
      </div>
    </div>
  {/each}
  <button class="secondary" onclick={addCategory}>+ Category</button>
</div>

<style>
  .warn {
    color: var(--warn, #a86a00);
  }
</style>
