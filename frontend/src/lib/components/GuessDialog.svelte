<script lang="ts">
  import { onMount } from 'svelte';
  import { gameSession } from '../stores/gameSession';
  import { router } from '../router';

  let loading = false;
  let error = '';
  let selection = '';

  const close = () => {
    router.closeModal();
  };

  const ensureContext = async () => {
    try {
      await gameSession.fetchAssignment();
    } catch {
      // handled via toasts
    }
    try {
      await gameSession.fetchLocations();
    } catch {
      // handled
    }
  };

  onMount(() => {
    ensureContext();
  });

  $: state = $gameSession;
  $: lobby = state.lobby;
  $: assignment = state.assignment;
  $: locations = state.locations;
  $: isImposter = assignment?.is_imposter ?? false;

  const availablePlayers = lobby
    ? lobby.players.filter((player) => player.id !== state.session?.playerId)
    : [];

  const submit = async () => {
    if (!lobby) {
      error = 'Lobby unavailable';
      return;
    }
    if (!assignment) {
      error = 'Assignment missing. Try again.';
      return;
    }

    if (!selection) {
      error = isImposter ? 'Choose a location before guessing.' : 'Pick who you want to accuse.';
      return;
    }

    loading = true;
    error = '';
    try {
      if (isImposter) {
        await gameSession.sendGuess({ locationId: Number(selection) });
      } else {
        await gameSession.sendGuess({ accusedId: selection });
      }
      close();
    } catch {
      // toast handled
    } finally {
      loading = false;
    }
  };
</script>

<div class="overlay" role="dialog" aria-modal="true">
  <div class="panel">
    <header>
      <h2>{isImposter ? 'Guess the location' : 'Accuse the imposter'}</h2>
      <button class="icon" type="button" on:click={close} aria-label="Close">
        ×
      </button>
    </header>

    {#if !assignment || !lobby}
      <p class="muted">
        We need to refresh your assignment before you can guess. Try reopening this window.
      </p>
    {:else}
      {#if isImposter}
        <p class="muted">
          You only get one shot. Choose the location that you believe the crew is defending.
        </p>
        <label class="field">
          Location
          <select bind:value={selection} disabled={loading}>
            <option value="">Select a location</option>
            {#each locations as location}
              <option value={location.id}>{location.name}</option>
            {/each}
          </select>
        </label>
      {:else}
        <p class="muted">
          Call out who you think is the imposter. If you&apos;re wrong, the imposter wins instantly.
        </p>
        <label class="field">
          Player
          <select bind:value={selection} disabled={loading}>
            <option value="">Select a player</option>
            {#each availablePlayers as player}
              <option value={player.id}>{player.name}</option>
            {/each}
          </select>
        </label>
      {/if}
    {/if}

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="actions">
      <button class="ghost" type="button" on:click={close} disabled={loading}>
        Cancel
      </button>
      <button class="primary" type="button" on:click={submit} disabled={loading}>
        {loading ? 'Submitting…' : 'Submit guess'}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(2, 6, 23, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    z-index: 20;
    backdrop-filter: blur(6px);
  }

  .panel {
    width: min(420px, 100%);
    background: rgba(15, 23, 42, 0.95);
    border-radius: 18px;
    border: 1px solid rgba(148, 163, 184, 0.25);
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 24px 40px rgba(15, 23, 42, 0.4);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h2 {
    margin: 0;
    font-size: 1.4rem;
  }

  .icon {
    background: transparent;
    border: none;
    color: rgba(226, 232, 240, 0.9);
    font-size: 1.5rem;
    cursor: pointer;
  }

  .muted {
    margin: 0;
    color: rgba(148, 163, 184, 0.85);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 0.9rem;
  }

  select {
    border: 1px solid rgba(148, 163, 184, 0.25);
    border-radius: 12px;
    padding: 12px 14px;
    background: rgba(15, 23, 42, 0.85);
    color: #f8fafc;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
  }

  .error {
    margin: 0;
    color: #fca5a5;
    font-size: 0.9rem;
  }
</style>
