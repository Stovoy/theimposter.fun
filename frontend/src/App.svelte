<script lang="ts">
  import { onMount } from 'svelte';
  import {
    createGame,
    getLobby,
    joinGame,
    updateRules,
    type CreateGameResponse,
    type GameLobby,
    type GameRules,
  } from './lib/api';

  const SESSION_KEY = 'theimposter.session';

  const defaultRules: GameRules = {
    max_players: 12,
    round_time_seconds: 120,
    allow_repeated_questions: false,
  };

  let createName = '';
  let joinName = '';
  let joinCode = '';
  let rules: GameRules = { ...defaultRules };
  let lobby: GameLobby | null = null;
  let hostToken: string | null = null;
  let playerId: string | null = null;
  let loading = false;
  let notice: { type: 'success' | 'error'; message: string } | null = null;

  type Session = {
    code: string;
    playerId: string;
    hostToken?: string;
  };

  const codeMask = (value: string) =>
    value.replace(/[^a-zA-Z0-9]/g, '').slice(0, 4).toUpperCase();

  const loadSession = (): Session | null => {
    try {
      const raw = localStorage.getItem(SESSION_KEY);
      if (!raw) return null;
      return JSON.parse(raw) as Session;
    } catch {
      return null;
    }
  };

  const saveSession = (session: Session) => {
    localStorage.setItem(SESSION_KEY, JSON.stringify(session));
  };

  const clearSession = () => {
    localStorage.removeItem(SESSION_KEY);
  };

  const showNotice = (type: 'success' | 'error', message: string) => {
    notice = { type, message };
    setTimeout(() => {
      if (notice?.message === message) {
        notice = null;
      }
    }, 3200);
  };

  onMount(async () => {
    const session = loadSession();
    if (session) {
      hostToken = session.hostToken ?? null;
      playerId = session.playerId ?? null;
      joinCode = session.code;
      await refreshLobby(session.code);
    }
  });

  const refreshLobby = async (code: string) => {
    try {
      lobby = await getLobby(code);
      rules = { ...lobby.rules };
    } catch (err) {
      showNotice('error', err instanceof Error ? err.message : 'Unable to load lobby');
      lobby = null;
    }
  };

  const handleCreate = async () => {
    if (!createName.trim()) {
      showNotice('error', 'Add a host name first');
      return;
    }

    loading = true;
    try {
      const created: CreateGameResponse = await createGame({
        host_name: createName.trim(),
        rules,
      });
      lobby = await getLobby(created.code);
      hostToken = created.host_token;
      playerId = created.player_id;
      joinCode = created.code;
      saveSession({
        code: created.code,
        playerId: created.player_id,
        hostToken: created.host_token,
      });
      showNotice('success', `Lobby ${created.code} ready to share`);
    } catch (err) {
      showNotice('error', err instanceof Error ? err.message : 'Something went wrong');
    } finally {
      loading = false;
    }
  };

  const handleJoin = async () => {
    const code = codeMask(joinCode);
    if (code.length !== 4 || !joinName.trim()) {
      showNotice('error', 'Add your name and a 4-letter code');
      return;
    }

    loading = true;
    try {
      const joined = await joinGame(code, { player_name: joinName.trim() });
      playerId = joined.player_id;
      hostToken = null;
      joinCode = joined.code;
      await refreshLobby(joined.code);
      saveSession({ code: joined.code, playerId: joined.player_id });
      showNotice('success', 'All set—find your friends in the circle!');
    } catch (err) {
      showNotice('error', err instanceof Error ? err.message : 'Could not join lobby');
    } finally {
      loading = false;
    }
  };

  const handleLeave = () => {
    lobby = null;
    playerId = null;
    hostToken = null;
    clearSession();
  };

  const handleRefresh = async () => {
    if (!joinCode) return;
    await refreshLobby(joinCode);
  };

  const handleRuleChange = (key: keyof GameRules, value: number | boolean) => {
    rules = { ...rules, [key]: value } as GameRules;
  };

  const submitRules = async () => {
    if (!lobby || !hostToken) return;
    loading = true;
    try {
      lobby = await updateRules(lobby.code, hostToken, rules);
      showNotice('success', 'Rules updated');
    } catch (err) {
      showNotice('error', err instanceof Error ? err.message : 'Could not update rules');
    } finally {
      loading = false;
    }
  };

  $: joinCode = codeMask(joinCode);
  $: canEditRules = Boolean(hostToken && lobby && lobby.leader_id === playerId);
  $: isInLobby = Boolean(lobby && playerId);
</script>

<main class="page">
  <header class="hero">
    <h1>The Imposter</h1>
    <p class="tagline">
      Start a lobby, gather your crew, and find the imposter. Designed for quick in-person play.
    </p>
  </header>

  {#if notice}
    <div class:toast-success={notice.type === 'success'} class="toast">
      {notice.message}
    </div>
  {/if}

  <section class="grid">
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
                min="4"
                max="16"
                bind:value={rules.max_players}
                on:change={(event) =>
                  handleRuleChange('max_players', Number(event.currentTarget.value))}
              />
            </label>
            <label>
              Round timer (sec)
              <input
                type="number"
                min="30"
                max="300"
                step="30"
                bind:value={rules.round_time_seconds}
                on:change={(event) =>
                  handleRuleChange('round_time_seconds', Number(event.currentTarget.value))}
              />
            </label>
          </div>
          <label class="checkbox">
            <input
              type="checkbox"
              checked={rules.allow_repeated_questions}
              on:change={(event) =>
                handleRuleChange('allow_repeated_questions', event.currentTarget.checked)}
            />
            Allow repeated questions
          </label>
        </fieldset>

        <button class="primary" type="submit" disabled={loading}>
          {loading ? 'Creating…' : 'Create lobby'}
        </button>
      </form>
    </article>

    <article class="card">
      <h2>Join a lobby</h2>
      <p class="card-note">Ask the host for their room code, then pick a name and jump in.</p>
      <form
        class="form"
        on:submit|preventDefault={handleJoin}
      >
        <label>
          Room code
          <input
            type="text"
            bind:value={joinCode}
            inputmode="text"
            placeholder="ABCD"
            maxlength="4"
            autocapitalize="characters"
            required
          />
        </label>
        <label>
          Your name
          <input
            type="text"
            bind:value={joinName}
            placeholder="Your name"
            autocomplete="name"
            required
          />
        </label>

        <button class="primary" type="submit" disabled={loading}>
          {loading ? 'Joining…' : 'Join lobby'}
        </button>
      </form>
    </article>
  </section>

  {#if isInLobby && lobby}
    <section class="lobby">
      <div class="lobby-header">
        <div>
          <h2>Lobby {lobby.code}</h2>
          <p class="subtle">Share this code with everyone nearby.</p>
        </div>
        <button class="secondary" on:click={handleRefresh} type="button">Refresh</button>
      </div>

      <div class="chips">
        <span class="chip">Players: {lobby.player_count}/{lobby.rules.max_players}</span>
        {#if canEditRules}
          <span class="chip chip-accent">You&apos;re the host</span>
        {/if}
      </div>

      <ul class="players">
        {#each lobby.players as player}
          <li class:me={player.id === playerId}>{player.name}</li>
        {/each}
      </ul>

      {#if canEditRules}
        <form
          class="rules"
          on:submit|preventDefault={submitRules}
        >
          <h3>Adjust rules</h3>
          <div class="field-row">
            <label>
              Max players
              <input
                type="number"
                min="4"
                max="16"
                bind:value={rules.max_players}
                on:change={(event) =>
                  handleRuleChange('max_players', Number(event.currentTarget.value))}
              />
            </label>
            <label>
              Round timer (sec)
              <input
                type="number"
                min="30"
                max="300"
                step="30"
                bind:value={rules.round_time_seconds}
                on:change={(event) =>
                  handleRuleChange('round_time_seconds', Number(event.currentTarget.value))}
              />
            </label>
          </div>
          <label class="checkbox">
            <input
              type="checkbox"
              checked={rules.allow_repeated_questions}
              on:change={(event) =>
                handleRuleChange('allow_repeated_questions', event.currentTarget.checked)}
            />
            Allow repeated questions
          </label>
          <button class="primary" type="submit" disabled={loading}>
            {loading ? 'Saving…' : 'Save rules'}
          </button>
        </form>
      {/if}

      <button class="ghost" type="button" on:click={handleLeave}>Leave lobby</button>
    </section>
  {/if}
</main>
