<script lang="ts">
  import { gameSession } from '../stores/gameSession';
  import { router } from '../router';
  import { describeRoundOutcome } from '../summary';

  $: state = $gameSession;
  $: lobby = state.lobby;

  $: history = lobby?.round_history ?? [];

  const goBack = () => {
    if (lobby) {
      router.replace('lobby', { code: lobby.code });
    } else {
      router.replace('landing');
    }
  };
</script>

{#if lobby}
  <section class="score-layout">
    <header class="score-header">
      <div>
        <h1>Scoreboard</h1>
        <p class="muted">Wins track across the entire session. Celebrate the cleverest crew.</p>
      </div>
      <button class="ghost small" type="button" on:click={goBack}>
        Back to lobby
      </button>
    </header>

    <article class="card">
      <h2>Player standings</h2>
      <table>
        <thead>
          <tr>
            <th>Player</th>
            <th>Crew wins</th>
            <th>Imposter wins</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {#each lobby.players as player}
            <tr>
              <td>{player.name}</td>
              <td>{player.crew_wins}</td>
              <td>{player.imposter_wins}</td>
              <td>{player.crew_wins + player.imposter_wins}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </article>

    <article class="card">
      <h2>Round history</h2>
      {#if history.length}
        <ul class="history">
          {#each history as item (item.round_number)}
            <li>
              <div>
                <h3>Round {item.round_number}</h3>
                <p>{describeRoundOutcome(item, lobby.players) ?? 'Outcome unavailable'}</p>
              </div>
              <span class="winner">Winner: {item.resolution.winner}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted">No rounds have been completed yet. Get a game going!</p>
      {/if}
    </article>
  </section>
{:else}
  <section class="empty">
    <h1>No lobby loaded</h1>
    <p>Join a lobby first to view the scoreboard.</p>
    <button class="primary" type="button" on:click={() => router.replace('landing')}>
      Back to start
    </button>
  </section>
{/if}

<style>
  .score-layout {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .score-header {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .score-header h1 {
    margin: 0;
    font-size: clamp(1.8rem, 4vw, 2.4rem);
  }

  .muted {
    margin: 0;
    color: rgba(148, 163, 184, 0.85);
  }

  @media (min-width: 720px) {
    .score-header {
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

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.95rem;
  }

  th,
  td {
    padding: 12px;
    text-align: left;
  }

  thead {
    background: rgba(30, 64, 175, 0.2);
  }

  tbody tr:nth-child(even) {
    background: rgba(15, 23, 42, 0.7);
  }
  tbody tr {
    border-bottom: 1px solid rgba(148, 163, 184, 0.12);
  }

  .history {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .history li {
    padding: 16px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.8);
    border: 1px solid rgba(148, 163, 184, 0.16);
    display: flex;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .history h3 {
    margin: 0 0 6px;
  }

  .history p {
    margin: 0;
  }

  .winner {
    font-size: 0.85rem;
    align-self: center;
    color: rgba(226, 232, 240, 0.85);
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
