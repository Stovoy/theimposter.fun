<script lang="ts">
  import { gameSession, currentPlayer, isHost, isReady } from '../stores/gameSession';
  import { router } from '../router';
  import { clampRuleValue, formatCategory } from '../rules';
  import { describeRoundOutcome } from '../summary';
  import type { GameRules } from '../api';

  let ruleDraft: GameRules | null = null;
  let savingRules = false;
  let readying = false;
  let starting = false;

  $: state = $gameSession;
  $: lobby = state.lobby;
  $: session = state.session;
  $: me = $currentPlayer;
  $: host = $isHost;
  $: readyStatus = $isReady;
  $: selectedCategories = ruleDraft
    ? ruleDraft.question_categories.map((item) => item.toLowerCase())
    : [];

  $: if (lobby && (!ruleDraft || !host)) {
    ruleDraft = { ...lobby.rules };
  } else if (!lobby) {
    ruleDraft = null;
  }

  $: lastRoundSummary = lobby
    ? describeRoundOutcome(lobby.last_round, lobby.players)
    : null;
  $: readyCount = lobby?.ready_player_count ?? 0;
  $: everyoneReady = lobby?.all_players_ready ?? false;

  const phaseLabel = () => {
    if (!lobby) return '';
    switch (lobby.phase) {
      case 'Lobby':
        return 'Lobby';
      case 'InRound':
        return 'Round in progress';
      case 'AwaitingNextRound':
        return 'Ready for next round';
      default:
        return lobby.phase;
    }
  };

  const handleRuleChange = (key: keyof GameRules, value: number | boolean) => {
    if (!ruleDraft) return;
    let next: number | boolean = value;
    if (typeof value === 'number') {
      next = clampRuleValue(key, value);
    }
    ruleDraft = { ...ruleDraft, [key]: next } as GameRules;
  };

  const saveRules = async () => {
    if (!ruleDraft) return;
    savingRules = true;
    try {
      await gameSession.setRules(ruleDraft);
    } catch {
      // toast surfaced
    } finally {
      savingRules = false;
    }
  };

  const handleReadyToggle = async () => {
    if (!lobby) return;
    readying = true;
    try {
      await gameSession.toggleReady(!readyStatus);
    } catch {
      // toast handled globally
    } finally {
      readying = false;
    }
  };

  const handleStartRound = async () => {
    if (!lobby) return;
    starting = true;
    try {
      await gameSession.beginRound();
      router.replace('round', { code: lobby.code });
    } catch {
      // toast handled
    } finally {
      starting = false;
    }
  };

  const leaveLobby = () => {
    gameSession.leaveSession();
    router.replace('landing');
  };

  const openModal = (modal: 'role' | 'locations') => {
    router.openModal(modal);
  };

  const toggleCategory = (category: string) => {
    if (!ruleDraft) return;
    const key = category.toLowerCase();
    const current = new Set(selectedCategories);
    if (current.has(key)) {
      current.delete(key);
    } else {
      current.add(key);
    }
    ruleDraft = {
      ...ruleDraft,
      question_categories: Array.from(current),
    } as GameRules;
  };
</script>

{#if lobby}
  <section class="lobby-layout">
    <header class="lobby-header">
      <div>
        <h1>Lobby {lobby.code}</h1>
        <p class="muted">
          Share the code with friends. Everyone stays in sync while you prepare the next round.
        </p>
      </div>
      <div class="header-actions">
        <button type="button" class="ghost small" on:click={() => router.goTo('scoreboard', { code: lobby.code })}>
          View scoreboard
        </button>
        <button type="button" class="ghost small" on:click={() => router.replace('lobby', { code: lobby.code })}>
          Refresh
        </button>
      </div>
    </header>

    <div class="chips">
      <span class="chip">Players: {lobby.player_count}/{lobby.rules.max_players}</span>
      <span class="chip chip-ready" class:chip-accent={everyoneReady}>
        Ready: {readyCount}/{lobby.player_count}
      </span>
      <span class="chip chip-muted">{phaseLabel()}</span>
      {#if host}
        <span class="chip chip-accent">You&apos;re the host</span>
      {/if}
    </div>

    <div class="lobby-grid">
      <article class="card roster">
        <h2>Players</h2>
        <ul class="players">
          {#each lobby.players as player}
            <li class:me={player.id === session?.playerId} class:ready={player.is_ready}>
              <div class="player-main">
                <span class="player-name">{player.name}</span>
                <span class="player-wins">{player.crew_wins} crew · {player.imposter_wins} imp</span>
              </div>
              <span class="player-status" class:ready={player.is_ready}>
                {player.is_ready ? 'Ready' : 'Waiting'}
              </span>
            </li>
          {/each}
        </ul>

        {#if lastRoundSummary}
          <div class="last-round">
            <h3>Last round</h3>
            <p>{lastRoundSummary}</p>
          </div>
        {/if}

        <div class="lobby-actions">
          {#if lobby.phase !== 'InRound'}
            <button
              class="secondary"
              type="button"
              disabled={readying}
              on:click={handleReadyToggle}
            >
              {readying ? 'Updating…' : readyStatus ? 'Cancel ready' : 'Ready up'}
            </button>
          {/if}
          <button class="ghost" type="button" on:click={leaveLobby}>Leave lobby</button>
        </div>
      </article>

      <article class="card controls">
        <h2>Controls</h2>
        <p class="muted">Stay prepared for the round and jump in as soon as everyone is ready.</p>

        <div class="control-buttons">
          <button
            class="primary"
            type="button"
            disabled={!host || lobby.phase === 'InRound' || starting}
            on:click={handleStartRound}
          >
            {starting ? 'Starting…' : 'Start round'}
          </button>
          <button type="button" class="outline" on:click={() => openModal('role')}>
            View my role
          </button>
          <button type="button" class="outline" on:click={() => openModal('locations')}>
            View locations
          </button>
        </div>

        {#if host && ruleDraft}
          <form
            class="rules"
            on:submit|preventDefault={saveRules}
          >
            <h3>Adjust rules</h3>
            <div class="field-row">
              <label>
                Max players
                <input
                  type="number"
                  min="3"
                  max="8"
                  bind:value={ruleDraft.max_players}
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
                  bind:value={ruleDraft.round_time_seconds}
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
                  bind:value={ruleDraft.location_pool_size}
                  on:change={(event) =>
                    handleRuleChange('location_pool_size', Number(event.currentTarget.value))}
                />
                <span class="helper-text">Locations drawn at the start of the game.</span>
              </label>
              <label class="checkbox switch">
                <input
                  type="checkbox"
                  checked={ruleDraft.allow_repeated_questions}
                  on:change={(event) =>
                    handleRuleChange('allow_repeated_questions', event.currentTarget.checked)}
                />
                Allow repeated questions
              </label>
            </div>

            {#if state.categories.length}
              <p class="category-header">Question categories</p>
              <div class="category-list">
                {#each state.categories as category}
                  <label class="checkbox category-item">
                    <input
                      type="checkbox"
                      checked={selectedCategories.includes(category.toLowerCase())}
                      on:change={() => toggleCategory(category)}
                    />
                    {formatCategory(category)}
                  </label>
                {/each}
              </div>
              <p class="category-note">Clear every category to allow any question.</p>
            {/if}
            <button class="primary" type="submit" disabled={savingRules}>
              {savingRules ? 'Saving…' : 'Save rules'}
            </button>
          </form>
        {/if}
      </article>
    </div>
  </section>
{:else}
  <section class="empty">
    <h1>Lobby not found</h1>
    <p>We couldn&apos;t load the lobby. Try joining again or double-check the code.</p>
    <button class="primary" type="button" on:click={() => router.replace('landing')}>
      Back to start
    </button>
  </section>
{/if}

<style>
  .lobby-layout {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .lobby-header {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .lobby-header h1 {
    margin: 0;
    font-size: clamp(1.8rem, 4vw, 2.4rem);
  }

  .muted {
    margin: 0;
    color: rgba(148, 163, 184, 0.85);
  }

  .header-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: rgba(59, 130, 246, 0.12);
    border-radius: 999px;
    font-size: 0.85rem;
  }

  .chip-accent {
    background: rgba(45, 212, 191, 0.18);
  }

  .chip-ready {
    background: rgba(147, 197, 253, 0.2);
  }

  .chip-muted {
    background: rgba(15, 23, 42, 0.7);
  }

  .lobby-grid {
    display: grid;
    gap: 18px;
  }

  @media (min-width: 900px) {
    .lobby-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .header-actions {
      justify-content: flex-end;
    }

    .lobby-header {
      flex-direction: row;
      justify-content: space-between;
      align-items: flex-start;
    }
  }

  .card {
    background: rgba(15, 23, 42, 0.75);
    border: 1px solid rgba(148, 163, 184, 0.12);
    border-radius: 20px;
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 20px 35px rgba(15, 23, 42, 0.3);
  }

  .players {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .players li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(15, 23, 42, 0.8);
    padding: 12px 16px;
    border-radius: 14px;
    border: 1px solid transparent;
    transition: border-color 0.2s ease, transform 0.2s ease;
  }

  .players li.me {
    border-color: rgba(59, 130, 246, 0.4);
    transform: translateY(-1px);
  }

  .player-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .player-name {
    font-weight: 600;
    font-size: 1rem;
  }

  .player-wins {
    font-size: 0.8rem;
    color: rgba(148, 163, 184, 0.8);
  }

  .player-status {
    font-size: 0.85rem;
    color: rgba(148, 163, 184, 0.95);
    padding: 6px 12px;
    border-radius: 999px;
    background: rgba(59, 130, 246, 0.15);
  }

  .player-status.ready {
    background: rgba(45, 212, 191, 0.18);
    color: rgba(15, 118, 110, 0.95);
  }

  .last-round {
    margin-top: 12px;
    padding: 16px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.8);
    border: 1px solid rgba(148, 163, 184, 0.18);
  }

  .last-round h3 {
    margin: 0 0 8px;
    font-size: 1rem;
  }

  .last-round p {
    margin: 0;
    color: rgba(226, 232, 240, 0.85);
  }

  .lobby-actions {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .controls .rules {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .control-buttons {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .small {
    padding: 10px 12px;
    font-size: 0.85rem;
  }

  .empty {
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 16px;
    align-items: center;
  }
</style>
