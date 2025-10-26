<script lang="ts">
  import { onMount } from 'svelte';
  import { gameSession } from '../stores/gameSession';
  import { router } from '../router';

  let loading = false;

  const loadAssignment = async () => {
    loading = true;
    try {
      await gameSession.fetchAssignment();
    } catch {
      // toast handled globally
    } finally {
      loading = false;
    }
  };

  onMount(() => {
    loadAssignment();
  });

  const close = () => {
    router.closeModal();
  };

  $: assignment = $gameSession.assignment;
</script>

<div class="overlay" role="dialog" aria-modal="true">
  <div class="panel">
    <header>
      <h2>Your assignment</h2>
      <button class="icon" type="button" on:click={close} aria-label="Close">
        ×
      </button>
    </header>

    {#if loading && !assignment}
      <p class="muted">Retrieving your role…</p>
    {:else if assignment}
      {#if assignment.is_imposter}
        <p>You are the Imposter. Blend in and guess the location before the crew exposes you.</p>
      {:else}
        <div class="assignment">
          <p>
            <strong>Role:</strong> {assignment.role ?? 'Unknown role'}
          </p>
          <p>
            <strong>Location:</strong> {assignment.location_name ?? 'Unknown location'}
          </p>
        </div>
        <p class="muted">Answer questions in character and help the crew identify the imposter.</p>
      {/if}
    {:else}
      <p class="muted">We couldn&apos;t load your assignment. Try again in a moment.</p>
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

  .assignment {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
