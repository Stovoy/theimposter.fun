<script lang="ts">
  import { gameSession, currentPlayer, isHost } from '../stores/gameSession';
  import { router } from '../router';
  import { describeRoundOutcome } from '../summary';

  let requestingQuestion = false;
  let startingNextRound = false;
  let aborting = false;

  $: state = $gameSession;
  $: lobby = state.lobby;
  $: round = state.round;
  $: me = $currentPlayer;
  $: host = $isHost;

  $: players = lobby?.players ?? [];
  $: playerLookup = new Map(players.map((player) => [player.id, player]));
  $: currentTurn = round?.current_turn_player_id
    ? playerLookup.get(round.current_turn_player_id) ?? null
    : null;
  $: outcomeSummary = round?.resolution
    ? describeRoundOutcome(
        { round_number: round.round_number, resolution: round.resolution },
        players,
      )
    : null;

  const openModal = (modal: 'role' | 'locations' | 'guess') => {
    router.openModal(modal);
  };

  const goToLobby = () => {
    if (lobby) {
      router.replace('lobby', { code: lobby.code });
    } else {
      router.replace('landing');
    }
  };

  const requestNextQuestion = async () => {
    requestingQuestion = true;
    try {
      await gameSession.requestNextQuestion();
    } catch {
      // handled via toasts
    } finally {
      requestingQuestion = false;
    }
  };

  const startNextRound = async () => {
    startingNextRound = true;
    try {
      const nextRound = await gameSession.advanceRound();
      if (lobby) {
        router.replace('round', { code: lobby.code });
      }
    } catch {
      // toast handled
    } finally {
      startingNextRound = false;
    }
  };

  const abortRound = async (scope: 'round' | 'game' = 'round') => {
    aborting = true;
    try {
      await gameSession.abort(scope);
      if (scope === 'game') {
        router.replace('lobby', { code: lobby?.code ?? '' });
      }
    } catch {
      // toast handled
    } finally {
      aborting = false;
    }
  };
</script>

{#if lobby && round}
  <section class="round-layout">
    <header class="round-header">
      <div>
        <h1>Round {round.round_number}</h1>
        <p class="muted">
          Everyone sees identical controls. Coordinate verbally and keep your cards close.
        </p>
      </div>
      <div class="header-actions">
        <button class="ghost small" type="button" on:click={goToLobby}>
          Back to lobby
        </button>
        <button class="ghost small" type="button" on:click={() => router.goTo('scoreboard', { code: lobby.code })}>
          View scoreboard
        </button>
      </div>
    </header>

    <div class="status-strip">
      <span class="chip">
        {#if currentTurn}
          {currentTurn.name} is up
        {:else}
          Waiting for the next turn
        {/if}
      </span>
      <span class="chip chip-muted">{lobby.phase}</span>
      {#if round.resolution}
        <span class="chip chip-accent">Round resolved</span>
      {/if}
    </div>

    <div class="round-grid">
      <article class="card primary-card">
        <h2>Question prompt</h2>
        {#if round.current_question}
          <div class="question">
            <p>{round.current_question.text}</p>
            <ul class="tag-list">
              {#each round.current_question.categories as category}
                <li>{category}</li>
              {/each}
            </ul>
          </div>
        {:else if round.resolution}
          <p>The round is resolved. Review the outcome and head back to the lobby.</p>
        {:else}
          <p>Waiting for a player to draw the next question.</p>
        {/if}

        <div class="actions">
          <button
            class="outline"
            type="button"
            disabled={requestingQuestion || Boolean(round.resolution)}
            on:click={requestNextQuestion}
          >
            {requestingQuestion ? 'Loading…' : 'Next question'}
          </button>
          <button class="outline" type="button" on:click={() => openModal('guess')} disabled={Boolean(round.resolution)}>
            Submit guess
          </button>
          <button class="outline" type="button" on:click={() => openModal('role')}>
            View my role
          </button>
          <button class="outline" type="button" on:click={() => openModal('locations')}>
            View locations
          </button>
        </div>

        {#if outcomeSummary}
          <div class="resolution">
            <h3>Round outcome</h3>
            <p>{outcomeSummary}</p>
          </div>
        {/if}
      </article>

      <article class="card secondary-card">
        <h2>Asked questions</h2>
        {#if round.asked_questions.length}
          <ul class="asked-list">
            {#each round.asked_questions as item}
              <li>
                <div>
                  <p class="question-text">{item.text}</p>
                  <ul class="tag-list small">
                    {#each item.categories as category}
                      <li>{category}</li>
                    {/each}
                  </ul>
                </div>
                <span class="asked-by">Asked by {playerLookup.get(item.asked_by)?.name ?? 'Unknown'}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">No questions have been asked yet.</p>
        {/if}
      </article>

      <article class="card tertiary-card">
        <h2>Turn order</h2>
        <ol class="turn-order">
          {#each round.turn_order as playerId, index}
            <li class:active={round.current_turn_player_id === playerId}>
              <span class="turn-index">{index + 1}</span>
              <span>{playerLookup.get(playerId)?.name ?? 'Unknown player'}</span>
            </li>
          {/each}
        </ol>

        {#if host}
          <div class="host-controls">
            <h3>Host controls</h3>
          <div class="host-buttons">
            <button
              class="primary"
              type="button"
              disabled={!round.resolution || startingNextRound}
              on:click={startNextRound}
            >
              {startingNextRound ? 'Starting…' : 'Start next round'}
            </button>
            <button
              class="secondary"
              type="button"
              disabled={requestingQuestion || Boolean(round.resolution)}
              on:click={requestNextQuestion}
            >
              {requestingQuestion ? 'Loading…' : 'Force next question'}
            </button>
            <button
              class="secondary"
              type="button"
              disabled={aborting}
              on:click={() => abortRound('round')}
              >
                {aborting ? 'Aborting…' : 'Abort round'}
              </button>
              <button
                class="ghost"
                type="button"
                disabled={aborting}
                on:click={() => abortRound('game')}
              >
                Stop game
              </button>
            </div>
          </div>
        {/if}
      </article>
    </div>
  </section>
{:else if lobby}
  <section class="empty">
    <h1>No active round</h1>
    <p>The lobby is waiting for a host to begin a round.</p>
    <button class="primary" type="button" on:click={goToLobby}>
      Back to lobby
    </button>
  </section>
{:else}
  <section class="empty">
    <h1>Round not available</h1>
    <p>We couldn&apos;t find an active round. Try rejoining the lobby.</p>
    <button class="primary" type="button" on:click={() => router.replace('landing')}>
      Back to start
    </button>
  </section>
{/if}

<style>
  .round-layout {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .round-header {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .round-header h1 {
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

  .status-strip {
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

  .chip-muted {
    background: rgba(15, 23, 42, 0.7);
  }

  .round-grid {
    display: grid;
    gap: 18px;
  }

  @media (min-width: 980px) {
    .round-header {
      flex-direction: row;
      justify-content: space-between;
      align-items: flex-start;
    }

    .round-grid {
      grid-template-columns: minmax(0, 1.2fr) minmax(0, 0.8fr);
      grid-auto-flow: row;
    }

    .secondary-card,
    .tertiary-card {
      height: fit-content;
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

  .question {
    padding: 16px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.85);
    border: 1px solid rgba(148, 163, 184, 0.18);
  }

  .question p {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
  }

  .tag-list {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .tag-list li {
    padding: 4px 10px;
    border-radius: 999px;
    background: rgba(59, 130, 246, 0.2);
    font-size: 0.75rem;
    letter-spacing: 0.02em;
  }

  .tag-list.small li {
    background: rgba(15, 23, 42, 0.7);
  }

  .actions {
    display: grid;
    gap: 12px;
  }

  @media (min-width: 600px) {
    .actions {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .resolution {
    padding: 16px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.82);
    border: 1px solid rgba(148, 163, 184, 0.16);
  }

  .resolution h3 {
    margin: 0 0 8px;
  }

  .asked-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .asked-list li {
    padding: 14px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.8);
    border: 1px solid rgba(148, 163, 184, 0.12);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .question-text {
    margin: 0;
    font-weight: 600;
  }

  .asked-by {
    font-size: 0.8rem;
    color: rgba(148, 163, 184, 0.75);
  }

  .turn-order {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .turn-order li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.75);
    border: 1px solid transparent;
  }

  .turn-order li.active {
    border-color: rgba(59, 130, 246, 0.4);
  }

  .turn-index {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: rgba(59, 130, 246, 0.25);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
  }

  .host-controls {
    padding-top: 12px;
    border-top: 1px solid rgba(148, 163, 184, 0.16);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .host-buttons {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
