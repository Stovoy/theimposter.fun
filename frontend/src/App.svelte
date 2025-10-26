<script lang="ts">
  import { onMount } from 'svelte';
  import {
    createGame,
    getCategories,
    getLobby,
    joinGame,
    updateRules,
    type CreateGameResponse,
    type GameLobby,
    type GamePhase,
    type GameRules,
    type PlayerSummary,
    type RoundSummary,
  } from './lib/api';

  const SESSION_KEY = 'theimposter.session';

  const defaultRules: GameRules = {
    max_players: 8,
    round_time_seconds: 120,
    allow_repeated_questions: false,
    location_pool_size: 10,
    question_categories: [],
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
  let availableCategories: string[] = [];
  let lastRoundSummary: string | null = null;

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

  const phaseLabel = (phase: GamePhase) => {
    switch (phase) {
      case 'Lobby':
        return 'Lobby';
      case 'InRound':
        return 'Round in progress';
      case 'AwaitingNextRound':
        return 'Ready for next round';
      default:
        return phase;
    }
  };

  const formatCategory = (category: string) =>
    category
      .split('_')
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(' ');

  const outcomeSummary = (summary: RoundSummary | null, players: PlayerSummary[]) => {
    if (!summary) return null;
    const roster = new Map(players.map((player) => [player.id, player.name]));
    const { outcome, winner } = summary.resolution;

    if ('CrewIdentifiedImposter' in outcome) {
      const info = outcome.CrewIdentifiedImposter;
      const accuser = roster.get(info.accuser) ?? 'Crew';
      const impostor = roster.get(info.impostor) ?? 'the imposter';
      return `${accuser} exposed ${impostor}. The crew scored a win.`;
    }

    if ('CrewMisdirected' in outcome) {
      const info = outcome.CrewMisdirected;
      const accuser = roster.get(info.accuser) ?? 'A crew member';
      const accused = roster.get(info.accused) ?? 'someone innocent';
      const impostor = roster.get(info.impostor) ?? 'the imposter';
      return `${accuser} accused ${accused} and missed. ${impostor} stole the round.`;
    }

    if ('ImposterIdentifiedLocation' in outcome) {
      const info = outcome.ImposterIdentifiedLocation;
      const impostor = roster.get(info.impostor) ?? 'The imposter';
      return `${impostor} guessed the location (${info.location_name}) and won the round.`;
    }

    if ('ImposterFailedLocationGuess' in outcome) {
      const info = outcome.ImposterFailedLocationGuess;
      const impostor = roster.get(info.impostor) ?? 'The imposter';
      return `${impostor} guessed the wrong location. The crew held the line at ${info.actual_location_name}.`;
    }

    return winner === 'Crew' ? 'The crew took the round.' : 'The imposter claimed victory.';
  };

  const clampValue = (key: keyof GameRules, value: number) => {
    if (key === 'max_players') return Math.min(Math.max(value, 3), 8);
    if (key === 'round_time_seconds') return Math.min(Math.max(value, 30), 600);
    if (key === 'location_pool_size') return Math.min(Math.max(value, 1), 15);
    return value;
  };

  const normalizedCategories = () =>
    Array.from(new Set(rules.question_categories.map((category) => category.toLowerCase())));

  const toggleCategory = (category: string) => {
    const normalized = category.toLowerCase();
    const current = new Set(normalizedCategories());
    if (current.has(normalized)) {
      current.delete(normalized);
    } else {
      current.add(normalized);
    }
    rules = { ...rules, question_categories: Array.from(current) } as GameRules;
  };

  const isCategorySelected = (category: string) =>
    normalizedCategories().includes(category.toLowerCase());

  const ensureCategories = async () => {
    try {
      const categories = await getCategories();
      availableCategories = categories;
      if (rules.question_categories.length === 0) {
        rules = {
          ...rules,
          question_categories: categories.slice(),
        } as GameRules;
      } else {
        const filtered = normalizedCategories().filter((category) =>
          categories.includes(category),
        );
        if (filtered.length !== rules.question_categories.length) {
          rules = { ...rules, question_categories: filtered } as GameRules;
        }
      }
    } catch (err) {
      showNotice(
        'error',
        err instanceof Error ? err.message : 'Could not load question categories',
      );
    }
  };

  onMount(async () => {
    await ensureCategories();
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
      if (availableCategories.length) {
        const filtered = normalizedCategories().filter((category) =>
          availableCategories.includes(category),
        );
        if (filtered.length !== rules.question_categories.length) {
          rules = { ...rules, question_categories: filtered } as GameRules;
        }
      }
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
      const categories = normalizedCategories();
      const payloadRules: GameRules = {
        ...rules,
        question_categories: categories.length ? categories : availableCategories,
      };
      const created: CreateGameResponse = await createGame({
        host_name: createName.trim(),
        rules: payloadRules,
      });
      hostToken = created.host_token;
      playerId = created.player_id;
      joinCode = created.code;
      await refreshLobby(created.code);
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
    rules = {
      ...defaultRules,
      question_categories: availableCategories.length ? availableCategories.slice() : [],
    };
  };

  const handleRefresh = async () => {
    if (!joinCode) return;
    await refreshLobby(joinCode);
  };

  const handleRuleChange = (key: keyof GameRules, raw: number | boolean) => {
    let next: number | boolean = raw;
    if (typeof raw === 'number') {
      next = clampValue(key, raw);
    }
    rules = { ...rules, [key]: next } as GameRules;
  };

  const submitRules = async () => {
    if (!lobby || !hostToken) return;
    loading = true;
    try {
      const categories = normalizedCategories();
      lobby = await updateRules(lobby.code, hostToken, {
        ...rules,
        question_categories: categories.length ? categories : availableCategories,
      });
      rules = { ...lobby.rules };
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
  $: lastRoundSummary = lobby ? outcomeSummary(lobby.last_round, lobby.players) : null;
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
                min="3"
                max="8"
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
                max="600"
                step="30"
                bind:value={rules.round_time_seconds}
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
                bind:value={rules.location_pool_size}
                on:change={(event) =>
                  handleRuleChange('location_pool_size', Number(event.currentTarget.value))}
              />
              <span class="helper-text">Locations drawn at the start of the game.</span>
            </label>
            <label class="checkbox switch">
              <input
                type="checkbox"
                checked={rules.allow_repeated_questions}
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
        <span class="chip chip-muted">{phaseLabel(lobby.phase)}</span>
        {#if canEditRules}
          <span class="chip chip-accent">You&apos;re the host</span>
        {/if}
      </div>

      <ul class="players">
        {#each lobby.players as player}
          <li class:me={player.id === playerId}>
            <span class="player-name">{player.name}</span>
            <span class="player-wins">{player.crew_wins} crew · {player.imposter_wins} imp</span>
          </li>
        {/each}
      </ul>

      {#if lastRoundSummary}
        <div class="last-round">
          <h3>Last round</h3>
          <p>{lastRoundSummary}</p>
        </div>
      {/if}

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
                min="3"
                max="8"
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
                max="600"
                step="30"
                bind:value={rules.round_time_seconds}
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
                bind:value={rules.location_pool_size}
                on:change={(event) =>
                  handleRuleChange('location_pool_size', Number(event.currentTarget.value))}
              />
              <span class="helper-text">Locations drawn at the start of the game.</span>
            </label>
            <label class="checkbox switch">
              <input
                type="checkbox"
                checked={rules.allow_repeated_questions}
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
            <p class="category-note">Clear every category to allow any question.</p>
          {/if}
          <button class="primary" type="submit" disabled={loading}>
            {loading ? 'Saving…' : 'Save rules'}
          </button>
        </form>
      {/if}

      <button class="ghost" type="button" on:click={handleLeave}>Leave lobby</button>
    </section>
  {/if}
</main>
