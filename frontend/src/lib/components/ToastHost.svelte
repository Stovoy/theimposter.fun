<script lang="ts">
  import { onDestroy } from 'svelte';
  import { gameSession } from '../stores/gameSession';

  const AUTO_DISMISS_MS = 3200;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();

  $: toasts = $gameSession.toasts;

  $: {
    toasts.forEach((toast) => {
      if (!toast.persistent && !timers.has(toast.id)) {
        const timer = setTimeout(() => {
          gameSession.dismissToast(toast.id);
          timers.delete(toast.id);
        }, AUTO_DISMISS_MS);
        timers.set(toast.id, timer);
      }
    });

    [...timers.keys()].forEach((id) => {
      if (!toasts.some((toast) => toast.id === id)) {
        const timer = timers.get(id);
        if (timer) {
          clearTimeout(timer);
          timers.delete(id);
        }
      }
    });
  }

  onDestroy(() => {
    timers.forEach((timer) => clearTimeout(timer));
    timers.clear();
  });

  const dismiss = (id: number) => {
    const timer = timers.get(id);
    if (timer) {
      clearTimeout(timer);
      timers.delete(id);
    }
    gameSession.dismissToast(id);
  };
</script>

{#if toasts.length}
  <div class="toast-stack">
    {#each toasts as toast (toast.id)}
      <div class="toast" class:success={toast.type === 'success'} class:error={toast.type === 'error'}>
        <span>{toast.message}</span>
        <button class="icon" type="button" on:click={() => dismiss(toast.id)} aria-label="Dismiss">
          ×
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed;
    top: 24px;
    right: 24px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 30;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(15, 23, 42, 0.9);
    border-radius: 14px;
    border: 1px solid rgba(148, 163, 184, 0.2);
    box-shadow: 0 12px 24px rgba(15, 23, 42, 0.4);
    color: rgba(226, 232, 240, 0.95);
  }

  .toast.success {
    border-color: rgba(59, 130, 246, 0.45);
    background: rgba(37, 99, 235, 0.2);
  }

  .toast.error {
    border-color: rgba(248, 113, 113, 0.35);
    background: rgba(248, 113, 113, 0.12);
  }

  .icon {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 1.2rem;
    cursor: pointer;
  }
</style>
