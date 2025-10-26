<script lang="ts">
  import { onMount } from 'svelte';
  import { gameSession } from '../stores/gameSession';
  import { router } from '../router';
  import { clampRuleValue, defaultRules, formatCategory, normalizeCategories } from '../rules';
  import type { GameRules } from '../api';

  const codeMask = (value: string) =>
    value.replace(/[^a-zA-Z0-9]/g, '').slice(0, 4).toUpperCase();

  let createName = '';
  let joinName = '';
  let joinCode = '';
  let draftRules: GameRules = { ...defaultRules };
  let primedCategories = false;
  let loading = false;

  $: sessionState = $gameSession;
  $: availableCategories = sessionState.categories;

  $: joinCode = codeMask(joinCode);

  $: if (!primedCategories && availableCategories.length) {
    primedCategories = true;
    draftRules = {
      ...draftRules,
      question_categories: availableCategories.slice(),
    };
  }

  const ensureCategories = () => {
    const normalized = normalizeCategories(draftRules.question_categories);
    if (normalized.length) {
      return normalized;
    }
    return availableCategories.slice();
  };

  const handleRuleChange = (key: keyof GameRules, value: number | boolean) => {
    let next: number | boolean = value;
    if (typeof value === 'number') {
      next = clampRuleValue(key, value);
    }
    draftRules = { ...draftRules, [key]: next } as GameRules;
  };

  const toggleCategory = (category: string) => {
    const normalized = normalizeCategories(draftRules.question_categories);
    const key = category.toLowerCase();
    const updated = normalized.includes(key)
      ? normalized.filter((item) => item !== key)
      : [...normalized, key];
    draftRules = { ...draftRules, question_categories: updated };
  };

  const isCategorySelected = (category: string) =>
    normalizeCategories(draftRules.question_categories).includes(category.toLowerCase());

  const handleCreate = async () => {
    if (!createName.trim()) {
      gameSession.pushToast('error', 'Add a host name first');
      return;
    }

    loading = true;
    try {
      const categories = ensureCategories();
      const payload: GameRules = {
        ...draftRules,
        question_categories: categories,
      };
      const created = await gameSession.createLobby(createName.trim(), payload);
      router.replace('lobby', { code: created.code });
    } catch {
      // errors surfaced via toast store
    } finally {
      loading = false;
    }
  };

  const handleJoin = async () => {
    const code = codeMask(joinCode);
    if (!joinName.trim() || code.length !== 4) {
      gameSession.pushToast('error', 'Add your name and a 4-letter code');
      return;
    }

    loading = true;
    try {
      await gameSession.joinLobby(code, joinName.trim());
      router.replace('lobby', { code });
    } catch {
      // handled by toast store
    } finally {
      loading = false;
    }
  };
</script>

<section class="layout">
  <article class="hero">
    <h1>The Imposter</h1>
    <p class="tagline">
      Start a lobby, gather your crew, and find the imposter. Designed for quick in-person play.
    </p>
    {#if sessionState.session && sessionState.lobby}
      <button
        class="primary hero-button"
        type="button"
        on:click={() => router.goTo('lobby', { code: sessionState.lobby?.code ?? '' })}
      >
        Re-open lobby {sessionState.lobby.code}
      </button>
    {/if}
  </article>

  <div class="grid">
    <article class="card">
      <h2>Create a lobby</h2>
      <p class="card-note">Host sets the scene and invites everyone with a 4-letter room code.</p>
      <form
        class="form"
        on:submit|preventDefault={handleCreate}
      >
        <label>
          Host name
          <input
            type="text"
            bind:value={createName}
            placeholder="Your name"
            autocomplete="name"
            required
          />
        </label>

        <fieldset>
          <legend>Quick rules</legend>
          <div class="field-row">
            <label>
              Max players
              <input
                type="number"
                min="3"
                max="8"
                bind:value={draftRules.max_players}
                on:change={(event) =>
                  handleRuleChange('max_players', Number(event.currentTarget.value))}
              />
            </label>
            <label>
              Round timer (sec)
              <input
                type="number"
                min="30"
                max="600"
                step="30"
                bind:value={draftRules.round_time_seconds}
                on:change={(event) =>
                  handleRuleChange('round_time_seconds', Number(event.currentTarget.value))}
              />
            </label>
          </div>
          <div class="field-row">
            <label>
              Location pool
              <input
                type="number"
                min="1"
                max="15"
                bind:value={draftRules.location_pool_size}
                on:change={(event) =>
                  handleRuleChange('location_pool_size', Number(event.currentTarget.value))}
              />
              <span class="helper-text">Locations drawn at the start of the game.</span>
            </label>
            <label class="checkbox switch">
              <input
                type="checkbox"
                checked={draftRules.allow_repeated_questions}
                on:change={(event) =>
                  handleRuleChange('allow_repeated_questions', event.currentTarget.checked)}
              />
              Allow repeated questions
            </label>
          </div>

          {#if availableCategories.length}
            <p class="category-header">Question categories</p>
            <div class="category-list">
              {#each availableCategories as category}
                <label class="checkbox category-item">
                  <input
                    type="checkbox"
                    checked={isCategorySelected(category)}
                    on:change={() => toggleCategory(category)}
                  />
                  {formatCategory(category)}
                </label>
              {/each}
            </div>
            <p class="category-note">Deselect all to let the game choose from every category.</p>
          {/if}
        </fieldset>

        <button class="primary" type="submit" disabled={loading}>
          {loading ? 'Creating…' : 'Create lobby'}
        </button>
      </form>
    </article>

    <article class="card">
      <h2>Join a lobby</h2>
      <p class="card-note">Already have a room code? Join your friends instantly.</p>
      <form
        class="form"
        on:submit|preventDefault={handleJoin}
      >
        <label>
          Display name
          <input
            type="text"
            bind:value={joinName}
            placeholder="Your name"
            autocomplete="name"
            required
          />
        </label>
        <label>
          Room code
          <input
            type="text"
            maxlength="4"
            bind:value={joinCode}
            placeholder="ABCD"
            autocomplete="off"
            required
          />
        </label>
        <button type="submit" class="primary outline" disabled={loading}>
          {loading ? 'Joining…' : 'Join lobby'}
        </button>
      </form>
    </article>
  </div>
</section>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    gap: 32px;
  }

  .hero {
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .hero-button {
    margin: 8px auto 0;
  }

  .tagline {
    margin: 0;
    color: rgba(226, 232, 240, 0.75);
  }

  .grid {
    display: grid;
    gap: 20px;
  }

  @media (min-width: 860px) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
