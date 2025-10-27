<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { router } from './lib/router';
  import { gameSession } from './lib/stores/gameSession';
  import LandingView from './lib/views/LandingView.svelte';
  import LobbyView from './lib/views/LobbyView.svelte';
  import RoundView from './lib/views/RoundView.svelte';
  import ScoreboardView from './lib/views/ScoreboardView.svelte';
  import RoleModal from './lib/components/RoleModal.svelte';
  import LocationModal from './lib/components/LocationModal.svelte';
  import GuessDialog from './lib/components/GuessDialog.svelte';
  import ToastHost from './lib/components/ToastHost.svelte';
  import type { RouteName } from './lib/router';

  let lobbyPollingActive = false;
  let roundPollingActive = false;

  $: route = $router;
  $: state = $gameSession;

  const syncRouteWithSession = () => {
    if (state.status !== 'ready') return;

    const lobbyCode = state.lobby?.code ?? state.session?.code ?? null;
    const inRoundPhase = state.lobby?.phase === 'InRound';
    const shouldShowRound = inRoundPhase || Boolean(state.round);

    if (shouldShowRound && route.name !== 'round' && route.name !== 'scoreboard' && lobbyCode) {
      router.replace('round', { code: lobbyCode });
      return;
    }

    if (route.name === 'round' && !shouldShowRound) {
      if (lobbyCode) {
        router.replace('lobby', { code: lobbyCode });
      } else {
        router.replace('landing');
      }
    }
  };

  $: managePolling(route.name, Boolean(state.session));
  $: syncRouteWithSession();

  const managePolling = (routeName: RouteName, hasSession: boolean) => {
    const shouldLobbyPoll = hasSession && routeName !== 'landing';
    const shouldRoundPoll = hasSession && routeName === 'round';

    if (shouldLobbyPoll && !lobbyPollingActive) {
      try {
        gameSession.startLobbyPolling();
        lobbyPollingActive = true;
      } catch {
        // handled via toasts
      }
    } else if (!shouldLobbyPoll && lobbyPollingActive) {
      gameSession.stopLobbyPolling();
      lobbyPollingActive = false;
    }

    if (shouldRoundPoll && !roundPollingActive) {
      try {
        gameSession.startRoundPolling();
        roundPollingActive = true;
      } catch {
        // handled via toasts
      }
    } else if (!shouldRoundPoll && roundPollingActive) {
      gameSession.stopRoundPolling();
      roundPollingActive = false;
    }
  };

  onMount(async () => {
    await gameSession.initialize();
    const session = get(gameSession).session;
    const currentRoute = router.readCurrentRoute();
    if (session && currentRoute.name === 'landing') {
      router.replace('lobby', { code: session.code });
    }
  });
</script>

<ToastHost />

{#if !state.isOnline}
  <div class="offline-banner" role="status" aria-live="polite">
    <strong>Offline.</strong>
    <span>Reconnect to resume syncing. We&apos;ll retry automatically.</span>
  </div>
{/if}

<main class="page">
  {#if state.status !== 'ready'}
    <section class="loading">
      <div class="spinner" aria-hidden="true"></div>
      <p>Loading game data…</p>
    </section>
  {:else}
    {#if route.name === 'landing'}
      <LandingView />
    {:else if route.name === 'lobby'}
      <LobbyView />
    {:else if route.name === 'round'}
      <RoundView />
    {:else if route.name === 'scoreboard'}
      <ScoreboardView />
    {/if}
  {/if}
</main>

{#if route.modal === 'role'}
  <RoleModal />
{:else if route.modal === 'locations'}
  <LocationModal />
{:else if route.modal === 'guess'}
  <GuessDialog />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 24px 18px 64px;
    max-width: 960px;
    margin: 0 auto;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    min-height: 60vh;
    text-align: center;
  }

  .loading p {
    margin: 0;
    color: rgba(148, 163, 184, 0.85);
  }

  .spinner {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 4px solid rgba(59, 130, 246, 0.2);
    border-top-color: rgba(59, 130, 246, 0.8);
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .offline-banner {
    margin: 0 auto 18px;
    padding: 12px 18px;
    max-width: 960px;
    border-radius: 14px;
    border: 1px solid rgba(248, 113, 113, 0.3);
    background: rgba(248, 113, 113, 0.1);
    color: rgba(248, 113, 113, 0.9);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
</style>
