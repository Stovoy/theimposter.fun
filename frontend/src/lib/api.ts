const API_BASE = (import.meta.env.VITE_API_BASE ?? "").replace(/\/$/, "");

export interface GameRules {
  max_players: number;
  round_time_seconds: number;
  allow_repeated_questions: boolean;
  location_pool_size: number;
  question_categories: string[];
}

export type GamePhase = "Lobby" | "InRound" | "AwaitingNextRound";
export type RoundWinner = "Crew" | "Imposter";

export type RoundOutcome =
  | {
      CrewIdentifiedImposter: {
        accuser: string;
        impostor: string;
      };
    }
  | {
      CrewMisdirected: {
        accuser: string;
        accused: string;
        impostor: string;
      };
    }
  | {
      ImposterIdentifiedLocation: {
        impostor: string;
        location_id: number;
        location_name: string;
      };
    }
  | {
      ImposterFailedLocationGuess: {
        impostor: string;
        guessed_location_id: number;
        actual_location_id: number;
        actual_location_name: string;
      };
    };

export interface RoundResolution {
  winner: RoundWinner;
  outcome: RoundOutcome;
  ended_at_ms: number;
}

export interface RoundSummary {
  round_number: number;
  resolution: RoundResolution;
}

export interface QuestionView {
  id: string;
  text: string;
  categories: string[];
}

export interface AskedQuestionView {
  id: string;
  text: string;
  categories: string[];
  asked_by: string;
  asked_at_ms: number;
}

export interface RoundPublicState {
  round_number: number;
  turn_order: string[];
  current_turn_player_id: string | null;
  current_question: QuestionView | null;
  asked_questions: AskedQuestionView[];
  started_at_ms: number;
  resolution: RoundResolution | null;
}

export interface PlayerAssignmentView {
  round_number: number;
  is_imposter: boolean;
  location_id: number | null;
  location_name: string | null;
  role: string | null;
}

export interface PlayerSummary {
  id: string;
  name: string;
  crew_wins: number;
  imposter_wins: number;
  is_ready: boolean;
}

export interface GameLobby {
  code: string;
  leader_id: string;
  rules: GameRules;
  players: PlayerSummary[];
  player_count: number;
  ready_player_count: number;
  all_players_ready: boolean;
  created_at_ms: number;
  phase: GamePhase;
  last_round: RoundSummary | null;
  round_history: RoundSummary[];
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

export interface LocationOption {
  id: number;
  name: string;
}

export interface NextQuestionResponse {
  question: QuestionView;
  next_turn_player_id: string;
  asked_total: number;
}

export interface GuessResponse {
  resolution: RoundResolution;
}

interface CategoriesResponse {
  categories: string[];
}

interface LocationListResponse {
  locations: LocationOption[];
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

export async function setReady(code: string, payload: { player_id: string; is_ready: boolean }) {
  return request<GameLobby>(`/api/games/${code}/ready`, {
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

export async function getRoundState(code: string) {
  return request<RoundPublicState>(`/api/games/${code}/round`, {
    method: "GET",
  });
}

export async function startGame(code: string, hostToken: string) {
  return request<RoundPublicState>(`/api/games/${code}/start`, {
    method: "POST",
    body: JSON.stringify({ host_token: hostToken }),
  });
}

export type AbortScope = "round" | "game";

export async function abortGame(
  code: string,
  payload: { host_token: string; scope?: AbortScope },
) {
  return request<GameLobby>(`/api/games/${code}/abort`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function startNextRound(code: string, hostToken: string) {
  return request<RoundPublicState>(`/api/games/${code}/round/next`, {
    method: "POST",
    body: JSON.stringify({ host_token: hostToken }),
  });
}

export async function drawNextQuestion(code: string, playerId: string) {
  return request<NextQuestionResponse>(`/api/games/${code}/round/question`, {
    method: "POST",
    body: JSON.stringify({ player_id: playerId }),
  });
}

type GuessPayload =
  | { player_id: string; accused_player_id: string }
  | { player_id: string; location_id: number };

export async function submitGuess(code: string, payload: GuessPayload) {
  return request<GuessResponse>(`/api/games/${code}/round/guess`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function getAssignment(code: string, playerId: string) {
  return request<PlayerAssignmentView>(
    `/api/games/${code}/round/assignment/${playerId}`,
    { method: "GET" },
  );
}

export async function getLocations(code: string) {
  const response = await request<LocationListResponse>(
    `/api/games/${code}/locations`,
    { method: "GET" },
  );
  return response.locations;
}

export async function getCategories() {
  const response = await request<CategoriesResponse>(`/api/content/categories`, {
    method: "GET",
  });
  return response.categories;
}
