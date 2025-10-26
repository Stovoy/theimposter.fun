const API_BASE = (import.meta.env.VITE_API_BASE ?? "").replace(/\/$/, "");

export interface GameRules {
  max_players: number;
  round_time_seconds: number;
  allow_repeated_questions: boolean;
}

export interface CreateGamePayload {
  host_name: string;
  rules?: GameRules;
}

export interface CreateGameResponse {
  code: string;
  host_token: string;
  leader_id: string;
  player_id: string;
  rules: GameRules;
}

export interface JoinGamePayload {
  player_name: string;
}

export interface JoinGameResponse {
  player_id: string;
  code: string;
}

export interface PlayerSummary {
  id: string;
  name: string;
}

export interface GameLobby {
  code: string;
  leader_id: string;
  rules: GameRules;
  players: PlayerSummary[];
  player_count: number;
  created_at_ms: number;
}

interface ApiErrorBody {
  message?: string;
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init.headers ?? {}),
    },
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = (await response.json()) as ApiErrorBody;
      if (body?.message) {
        message = body.message;
      }
    } catch {
      // ignore JSON parsing errors
    }
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export async function createGame(payload: CreateGamePayload) {
  return request<CreateGameResponse>("/api/games", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function joinGame(code: string, payload: JoinGamePayload) {
  return request<JoinGameResponse>(`/api/games/${code}/join`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function updateRules(code: string, hostToken: string, rules: GameRules) {
  return request<GameLobby>(`/api/games/${code}`, {
    method: "PATCH",
    body: JSON.stringify({ host_token: hostToken, rules }),
  });
}

export async function getLobby(code: string) {
  return request<GameLobby>(`/api/games/${code}`, {
    method: "GET",
  });
}
