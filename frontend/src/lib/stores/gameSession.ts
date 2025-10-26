import { derived, writable } from "svelte/store";
import {
  abortGame,
  buildGameStreamUrl,
  createGame,
  drawNextQuestion,
  getAssignment,
  getCategories,
  getLobby,
  getLocations,
  getRoundState,
  joinGame,
  setReady,
  startGame,
  startNextRound,
  submitGuess,
  updateRules,
  type AbortScope,
  type CreateGameResponse,
  type GameEvent,
  type GameLobby,
  type GameRules,
  type GuessResponse,
  type LocationOption,
  type NextQuestionResponse,
  type PlayerAssignmentView,
  type PlayerSummary,
  type RoundPublicState,
  type RoundSummary,
} from "../api";

const SESSION_KEY = "theimposter.session";

type NoticeType = "success" | "error" | "info";

export type Session = {
  code: string;
  playerId: string;
  hostToken?: string;
};

export type Toast = {
  id: number;
  type: NoticeType;
  message: string;
  persistent?: boolean;
};

type ClientStatus = "idle" | "initializing" | "ready";

export interface GameClientState {
  status: ClientStatus;
  session: Session | null;
  lobby: GameLobby | null;
  round: RoundPublicState | null;
  assignment: PlayerAssignmentView | null;
  locations: LocationOption[];
  categories: string[];
  syncingLobby: boolean;
  syncingRound: boolean;
  lastLobbySyncMs: number | null;
  lastRoundSyncMs: number | null;
  pendingActions: number;
  lastError: string | null;
  toasts: Toast[];
  isOnline: boolean;
  offlineSince: number | null;
  lastLobbyError: string | null;
  realtimeConnected: boolean;
  realtimeAttempts: number;
  lastRealtimeError: string | null;
}

const detectOnline = () =>
  typeof navigator !== "undefined" ? navigator.onLine : true;

const initialState: GameClientState = {
  status: "idle",
  session: null,
  lobby: null,
  round: null,
  assignment: null,
  locations: [],
  categories: [],
  syncingLobby: false,
  syncingRound: false,
  lastLobbySyncMs: null,
  lastRoundSyncMs: null,
  pendingActions: 0,
  lastError: null,
  toasts: [],
  isOnline: detectOnline(),
  offlineSince: detectOnline() ? null : Date.now(),
  lastLobbyError: null,
  realtimeConnected: false,
  realtimeAttempts: 0,
  lastRealtimeError: null,
};

const canUseStorage =
  typeof window !== "undefined" && typeof window.localStorage !== "undefined";

const loadStoredSession = (): Session | null => {
  if (!canUseStorage) return null;
  try {
    const raw = window.localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Session;
    if (!parsed?.code || !parsed?.playerId) return null;
    return parsed;
  } catch {
    return null;
  }
};

const saveStoredSession = (session: Session | null) => {
  if (!canUseStorage) return;
  if (!session) {
    window.localStorage.removeItem(SESSION_KEY);
    return;
  }
  window.localStorage.setItem(SESSION_KEY, JSON.stringify(session));
};

const errorMessage = (err: unknown) =>
  err instanceof Error ? err.message : "Something went wrong";

const DEFAULT_POLL_INTERVAL = 4500;

export const createGameSessionStore = () => {
  const internal = writable<GameClientState>(initialState);
  let currentState = initialState;

  internal.subscribe((value) => {
    currentState = value;
  });

  let lobbyTimer: ReturnType<typeof setInterval> | null = null;
  let roundTimer: ReturnType<typeof setInterval> | null = null;
  let toastCounter = 0;
  let realtime: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let manualDisconnect = false;
  let reconnectAttempts = 0;

  const updateState = (fn: (state: GameClientState) => GameClientState) => {
    internal.update((state) => fn(state));
  };

  const updateOnlineStatus = (isOnline: boolean) => {
    updateState((state) => ({
      ...state,
      isOnline,
      offlineSince: isOnline ? null : state.offlineSince ?? Date.now(),
    }));
  };

  if (typeof window !== "undefined") {
    updateOnlineStatus(detectOnline());
    window.addEventListener("online", () => updateOnlineStatus(true));
    window.addEventListener("offline", () => updateOnlineStatus(false));
  }

  const setStatus = (status: ClientStatus) => {
    updateState((state) => ({ ...state, status }));
  };

  const setPending = (delta: number) => {
    updateState((state) => ({
      ...state,
      pendingActions: Math.max(0, state.pendingActions + delta),
    }));
  };

  const pushToast = (type: NoticeType, message: string, persistent = false) => {
    const id = ++toastCounter;
    updateState((state) => ({
      ...state,
      toasts: [...state.toasts, { id, type, message, persistent }],
    }));
    return id;
  };

  const dismissToast = (id: number) => {
    updateState((state) => ({
      ...state,
      toasts: state.toasts.filter((toast) => toast.id !== id),
    }));
  };

  const clearToasts = () => {
    updateState((state) => ({ ...state, toasts: [] }));
  };

  const updateRealtimeStatus = (connected: boolean, error?: string | null) => {
    updateState((state) => ({
      ...state,
      realtimeConnected: connected,
      realtimeAttempts: reconnectAttempts,
      lastRealtimeError: error ?? (connected ? null : state.lastRealtimeError),
    }));
  };

  const clearReconnectSchedule = () => {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  const startFallbackPolling = () => {
    startLobbyPolling();
    if (currentState.round || currentState.lobby?.phase === "InRound") {
      startRoundPolling();
    }
  };

  function applyLobbyUpdate(lobby: GameLobby) {
    const now = Date.now();
    updateState((state) => ({
      ...state,
      lobby,
      syncingLobby: false,
      lastLobbySyncMs: now,
      lastError: null,
      lastLobbyError: null,
    }));
  }

  function applyRoundUpdate(round: RoundPublicState | null) {
    const now = Date.now();
    updateState((state) => ({
      ...state,
      round,
      assignment: round ? state.assignment : null,
      syncingRound: false,
      lastRoundSyncMs: now,
      lastError: null,
    }));
  }

  function handleRealtimeEvent(event: GameEvent) {
    switch (event.type) {
      case "snapshot":
        applyLobbyUpdate(event.lobby);
        applyRoundUpdate(event.round ?? null);
        break;
      case "lobby":
        applyLobbyUpdate(event.lobby);
        break;
      case "round":
        applyRoundUpdate(event.round ?? null);
        break;
      case "pong":
      default:
        break;
    }
  }

  function connectRealtime() {
    if (typeof window === "undefined") return;
    if (realtime || manualDisconnect) return;

    const session = currentState.session ?? loadStoredSession();
    if (!session) return;

    let url: string;
    try {
      url = buildGameStreamUrl(session.code);
    } catch (err) {
      updateRealtimeStatus(false, errorMessage(err));
      return;
    }

    manualDisconnect = false;
    try {
      realtime = new WebSocket(url);
    } catch (err) {
      updateRealtimeStatus(false, errorMessage(err));
      scheduleReconnect(errorMessage(err));
      return;
    }

    updateRealtimeStatus(false);

    realtime.onopen = () => {
      clearReconnectSchedule();
      reconnectAttempts = 0;
      updateRealtimeStatus(true);
      stopLobbyPolling();
      stopRoundPolling();
      Promise.all([
        refreshLobby({ silent: true }).catch(() => {}),
        refreshRound({ silent: true }).catch(() => {}),
      ]).catch(() => {
        // errors already surfaced
      });
    };

    realtime.onmessage = (event) => {
      if (typeof event.data !== "string") {
        return;
      }
      try {
        const data = JSON.parse(event.data) as GameEvent;
        handleRealtimeEvent(data);
      } catch (err) {
        updateRealtimeStatus(currentState.realtimeConnected, errorMessage(err));
      }
    };

    realtime.onerror = () => {
      updateRealtimeStatus(false, "Realtime connection error");
    };

    realtime.onclose = (event) => {
      realtime = null;
      const reason = event.reason || null;
      updateRealtimeStatus(false, reason);
      if (manualDisconnect) {
        manualDisconnect = false;
        reconnectAttempts = 0;
        return;
      }
      startFallbackPolling();
      scheduleReconnect(reason ?? undefined);
    };
  }

  function scheduleReconnect(reason?: string) {
    if (typeof window === "undefined") return;
    if (manualDisconnect) return;
    clearReconnectSchedule();
    reconnectAttempts = Math.min(reconnectAttempts + 1, 8);
    updateRealtimeStatus(false, reason ?? currentState.lastRealtimeError);
    startFallbackPolling();
    const delay = Math.min(30000, 2000 * 2 ** (reconnectAttempts - 1));
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null;
      connectRealtime();
    }, delay);
  }

  function teardownRealtime(manual = false) {
    clearReconnectSchedule();
    manualDisconnect = manual;
    if (realtime) {
      try {
        realtime.close();
      } catch {
        // ignore errors when closing
      }
    } else {
      manualDisconnect = false;
      reconnectAttempts = 0;
      updateRealtimeStatus(false);
    }
  }

  const ensureCategories = async () => {
    try {
      const categories = await getCategories();
      updateState((state) => ({
        ...state,
        categories,
      }));
      return categories;
    } catch (err) {
      const message = errorMessage(err);
      pushToast("error", message, true);
      updateState((state) => ({ ...state, lastError: message }));
      throw err;
    }
  };

  const getActiveSession = () => {
    const session = currentState.session ?? loadStoredSession();
    if (!session) {
      throw new Error("No active session");
    }
    if (!currentState.session) {
      updateState((state) => ({ ...state, session }));
    }
    return session;
  };

  const refreshLobby = async (options?: { silent?: boolean }) => {
    const { silent = false } = options ?? {};
    const session = getActiveSession();
    if (!silent) {
      updateState((state) => ({ ...state, syncingLobby: true }));
    }

    try {
      const lobby = await getLobby(session.code);
      updateState((state) => ({
        ...state,
        lobby,
        syncingLobby: false,
        lastLobbySyncMs: Date.now(),
        lastError: null,
        lastLobbyError: null,
      }));
      return lobby;
    } catch (err) {
      const message = errorMessage(err);
      if (!silent) {
        pushToast("error", message, true);
      }
      updateState((state) => ({
        ...state,
        syncingLobby: false,
        lastError: message,
        lastLobbyError: message,
      }));
      throw err;
    }
  };

  const refreshRound = async (options?: { silent?: boolean }) => {
    const { silent = false } = options ?? {};
    const session = getActiveSession();
    if (!silent) {
      updateState((state) => ({ ...state, syncingRound: true }));
    }
    try {
      const round = await getRoundState(session.code);
      updateState((state) => ({
        ...state,
        round,
        syncingRound: false,
        lastRoundSyncMs: Date.now(),
        lastError: null,
      }));
      return round;
    } catch (err) {
      const message = errorMessage(err);
      if (!silent) {
        pushToast("error", message);
      }
      updateState((state) => ({
        ...state,
        syncingRound: false,
        lastError: message,
      }));
      throw err;
    }
  };

  const stopLobbyPolling = () => {
    if (lobbyTimer) {
      clearInterval(lobbyTimer);
      lobbyTimer = null;
    }
    updateState((state) => ({ ...state, syncingLobby: false }));
  };

  const stopRoundPolling = () => {
    if (roundTimer) {
      clearInterval(roundTimer);
      roundTimer = null;
    }
    updateState((state) => ({ ...state, syncingRound: false }));
  };

  const startLobbyPolling = (interval = DEFAULT_POLL_INTERVAL) => {
    if (currentState.realtimeConnected) {
      stopLobbyPolling();
      return;
    }
    stopLobbyPolling();
    refreshLobby({ silent: true }).catch(() => {
      // error already handled in refresh
    });
    lobbyTimer = setInterval(() => {
      refreshLobby({ silent: true }).catch(() => {
        // handled
      });
    }, interval);
  };

  const startRoundPolling = (interval = DEFAULT_POLL_INTERVAL) => {
    if (currentState.realtimeConnected) {
      stopRoundPolling();
      return;
    }
    stopRoundPolling();
    refreshRound({ silent: true }).catch(() => {
      // handled
    });
    roundTimer = setInterval(() => {
      refreshRound({ silent: true }).catch(() => {
        // handled
      });
    }, interval);
  };

  const resetRoundState = () => {
    stopRoundPolling();
    updateState((state) => ({
      ...state,
      round: null,
      assignment: null,
      locations: [],
      syncingRound: false,
      lastRoundSyncMs: null,
    }));
  };

  const initialize = async () => {
    if (currentState.status !== "idle") return;
    setStatus("initializing");
    try {
      await ensureCategories();
    } catch {
      // categories errors already surfaced; continue initialization
    }

    const session = loadStoredSession();
    if (session) {
      updateState((state) => ({ ...state, session, status: "ready" }));
      try {
        await refreshLobby({ silent: true });
      } catch {
        // ignore initial failure, user can retrigger
      }
      connectRealtime();
      startLobbyPolling();
      if (currentState.lobby?.phase === "InRound") {
        startRoundPolling();
      }
    } else {
      setStatus("ready");
    }
  };

  const createLobby = async (hostName: string, rules: GameRules) => {
    await initialize();
    return withAction(async () => {
      const response: CreateGameResponse = await createGame({
        host_name: hostName,
        rules,
      });
      const session: Session = {
        code: response.code,
        playerId: response.player_id,
        hostToken: response.host_token,
      };
      saveStoredSession(session);
      updateState((state) => ({
        ...state,
        session,
        lobby: null,
        round: null,
        assignment: null,
        status: "ready",
      }));
      await refreshLobby({ silent: true });
      pushToast("success", `Lobby ${response.code} ready to share`);
      connectRealtime();
      startLobbyPolling();
      return response;
    });
  };

  const joinLobby = async (code: string, playerName: string) => {
    await initialize();
    return withAction(async () => {
      const response = await joinGame(code, { player_name: playerName });
      const session: Session = {
        code: response.code,
        playerId: response.player_id,
      };
      saveStoredSession(session);
      updateState((state) => ({
        ...state,
        session,
        lobby: null,
        round: null,
        assignment: null,
        status: "ready",
      }));
      await refreshLobby({ silent: true });
      pushToast("success", "Joined lobby successfully");
      connectRealtime();
      startLobbyPolling();
      return response;
    }, { persistentErrors: true });
  };

  const leaveSession = () => {
    teardownRealtime(true);
    stopLobbyPolling();
    stopRoundPolling();
    saveStoredSession(null);
    updateState((state) => ({
      ...initialState,
      categories: state.categories,
      status: "ready",
      isOnline: state.isOnline,
      offlineSince: state.offlineSince,
    }));
  };

  const setRules = async (rules: GameRules) => {
    const session = getActiveSession();
    const hostToken = session.hostToken;
    if (!hostToken) {
      throw new Error("Host token required to update rules");
    }
    return withAction(async () => {
      const lobby = await updateRules(session.code, hostToken, rules);
      updateState((state) => ({ ...state, lobby }));
      pushToast("success", "Rules updated");
      return lobby;
    });
  };

  const toggleReady = async (isReady: boolean) => {
    const session = getActiveSession();
    return withAction(async () => {
      const lobby = await setReady(session.code, {
        player_id: session.playerId,
        is_ready: isReady,
      });
      updateState((state) => ({ ...state, lobby }));
      return lobby;
    });
  };

  const beginRound = async () => {
    const session = getActiveSession();
    const hostToken = session.hostToken;
    if (!hostToken) {
      throw new Error("Host token required to start the game");
    }
    return withAction(async () => {
      const round = await startGame(session.code, hostToken);
      updateState((state) => ({ ...state, round }));
      if (!currentState.realtimeConnected) {
        await refreshLobby({ silent: true }).catch(() => {
          /* ignore*/
        });
      }
      startRoundPolling();
      return round;
    });
  };

  const advanceRound = async () => {
    const session = getActiveSession();
    const hostToken = session.hostToken;
    if (!hostToken) {
      throw new Error("Host token required to start the next round");
    }
    return withAction(async () => {
      const round = await startNextRound(session.code, hostToken);
      updateState((state) => ({ ...state, round }));
      if (!currentState.realtimeConnected) {
        await refreshLobby({ silent: true }).catch(() => {
          /* ignore */
        });
      }
      startRoundPolling();
      return round;
    });
  };

  const abort = async (scope: AbortScope = "round") => {
    const session = getActiveSession();
    const hostToken = session.hostToken;
    if (!hostToken) {
      throw new Error("Host token required to abort");
    }
    return withAction(async () => {
      const lobby = await abortGame(session.code, {
        host_token: hostToken,
        scope,
      });
      updateState((state) => ({ ...state, lobby }));
      if (scope === "round") {
        resetRoundState();
      } else {
        stopLobbyPolling();
        stopRoundPolling();
        resetRoundState();
      }
      return lobby;
    });
  };

  const fetchAssignment = async () => {
    const session = getActiveSession();
    return withAction(async () => {
      const assignment = await getAssignment(session.code, session.playerId);
      updateState((state) => ({ ...state, assignment }));
      return assignment;
    });
  };

  const fetchLocations = async () => {
    const session = getActiveSession();
    return withAction(async () => {
      const locations = await getLocations(session.code);
      updateState((state) => ({ ...state, locations }));
      return locations;
    });
  };

  const requestNextQuestion = async () => {
    const session = getActiveSession();
    return withAction(async () => {
      const response: NextQuestionResponse = await drawNextQuestion(
        session.code,
        session.playerId,
      );
      if (!currentState.realtimeConnected) {
        await refreshRound({ silent: true }).catch(() => {
          /* ignore */
        });
      }
      return response;
    });
  };

  const sendGuess = async (payload: { accusedId?: string; locationId?: number }) => {
    const session = getActiveSession();
    return withAction(async () => {
      let result: GuessResponse | null = null;
      if (payload.accusedId) {
        result = await submitGuess(session.code, {
          player_id: session.playerId,
          accused_player_id: payload.accusedId,
        });
      } else if (payload.locationId) {
        result = await submitGuess(session.code, {
          player_id: session.playerId,
          location_id: payload.locationId,
        });
      } else {
        throw new Error("Guess payload missing accused or location");
      }
      if (!currentState.realtimeConnected) {
        await Promise.all([
          refreshRound({ silent: true }).catch(() => {
            /* ignore */
          }),
          refreshLobby({ silent: true }).catch(() => {
            /* ignore */
          }),
        ]);
      }
      return result;
    });
  };

  const withAction = async <T>(
    fn: () => Promise<T>,
    options?: { persistentErrors?: boolean },
  ) => {
    setPending(1);
    try {
      return await fn();
    } catch (err) {
      const message = errorMessage(err);
      pushToast("error", message, options?.persistentErrors);
      updateState((state) => ({ ...state, lastError: message }));
      throw err;
    } finally {
      setPending(-1);
    }
  };

  return {
    subscribe: internal.subscribe,
    initialize,
    createLobby,
    joinLobby,
    leaveSession,
    refreshLobby,
    refreshRound,
    startLobbyPolling,
    stopLobbyPolling,
    startRoundPolling,
    stopRoundPolling,
    setRules,
    toggleReady,
    beginRound,
    advanceRound,
    abort,
    fetchAssignment,
    fetchLocations,
    requestNextQuestion,
    sendGuess,
    pushToast,
    dismissToast,
    clearToasts,
    resetRoundState,
    getActiveSession,
  };
};

export const gameSession = createGameSessionStore();

export const currentPlayer = derived(gameSession, ($game) => {
  const session = $game.session;
  if (!session || !$game.lobby) return null;
  return (
    $game.lobby.players.find((player) => player.id === session.playerId) ?? null
  );
});

export const isHost = derived(gameSession, ($game) => {
  if (!$game.session || !$game.lobby) return false;
  return $game.session.playerId === $game.lobby.leader_id;
});

export const isReady = derived(gameSession, ($game) => {
  const session = $game.session;
  if (!session || !$game.lobby) {
    return false;
  }
  const me =
    $game.lobby.players.find((player) => player.id === session.playerId) ?? null;
  return Boolean(me?.is_ready);
});

export const hostControlsVisible = derived(
  [gameSession, isHost],
  ([$game, $isHost]) => {
    if (!$isHost || !$game.lobby) return false;
    return $game.lobby.leader_id === $game.session?.playerId;
  },
);

export const roundHistory = derived(gameSession, ($game) => {
  return $game.lobby?.round_history ?? [];
});

export const playersById = derived(gameSession, ($game) => {
  const map = new Map<string, PlayerSummary>();
  $game.lobby?.players.forEach((player) => {
    map.set(player.id, player);
  });
  return map;
});

export const currentQuestion = derived(gameSession, ($game) => {
  return $game.round?.current_question ?? null;
});

export const askedQuestions = derived(gameSession, ($game) => {
  return $game.round?.asked_questions ?? [];
});

export const currentRoundNumber = derived(gameSession, ($game) => {
  return $game.round?.round_number ?? null;
});

export const currentWinner = derived(gameSession, ($game) => {
  const resolution = $game.round?.resolution;
  return resolution?.winner ?? null;
});

export const roundSummary = derived(gameSession, ($game) => {
  const summary: RoundSummary[] = [];
  if ($game.lobby?.last_round) {
    summary.push($game.lobby.last_round);
  }
  return summary;
});
