<script lang="ts">
  import { onMount } from 'svelte';
  import { gameSession } from '../stores/gameSession';
  import { router } from '../router';

  let loading = false;

  const loadLocations = async () => {
    loading = true;
    try {
      await gameSession.fetchLocations();
    } catch {
      // handled globally
    } finally {
      loading = false;
    }
  };

  onMount(() => {
    loadLocations();
  });

  const close = () => {
    router.closeModal();
  };

  $: locations = $gameSession.locations;
</script>

<div class="overlay" role="dialog" aria-modal="true">
  <div class="panel">
    <header>
      <h2>Active locations</h2>
      <button class="icon" type="button" on:click={close} aria-label="Close">
        ×
      </button>
    </header>

    {#if loading && !locations.length}
      <p class="muted">Loading the location list…</p>
    {:else if locations.length}
      <p class="muted">
        Everyone in the round sees this same set of possible locations.
      </p>
      <ul class="locations">
        {#each locations as location}
          <li>{location.name}</li>
        {/each}
      </ul>
    {:else}
      <p class="muted">No locations available for this round.</p>
    {/if}

    <button class="primary" type="button" on:click={close}>
      Close
    </button>
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
    max-height: min(540px, 100%);
    overflow-y: auto;
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

  .locations {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }

  .locations li {
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.85);
    border: 1px solid rgba(148, 163, 184, 0.18);
  }
</style>
