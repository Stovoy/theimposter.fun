import type { BrowserContext, Route } from '@playwright/test';
import { defaultRules } from '../../src/lib/rules';
import type {
  GameLobby,
  GamePhase,
  GameRules,
  JoinGamePayload,
  PlayerSummary,
  RoundPublicState,
} from '../../src/lib/api';

type ReadyRequest = {
  player_id: string;
  is_ready: boolean;
};

type CreateGamePayload = {
  host_name: string;
  rules?: GameRules;
};

type LobbyState = {
  code: string;
  createdAt: number;
  hostToken: string;
  leaderId: string;
  rules: GameRules;
  players: PlayerState[];
  phase: GamePhase;
  lastRound: GameLobby['last_round'];
  roundHistory: GameLobby['round_history'];
  currentRound: RoundPublicState | null;
  roundSequence: number;
};

type PlayerState = PlayerSummary;

const jsonResponse = (status: number, data: unknown) => ({
  status,
  contentType: 'application/json',
  body: JSON.stringify(data),
});

const notFound = (message = 'Not found') => jsonResponse(404, { message });

export class MockGameServer {
  private readonly lobbies = new Map<string, LobbyState>();
  private readonly categories = ['general_knowledge', 'mystery', 'travel'];
  private lobbyCounter = 0;
  private playerCounter = 0;
  private presetCodes: string[] = [];

  constructor(options?: { presetCodes?: string[] }) {
    this.presetCodes = options?.presetCodes ? [...options.presetCodes] : [];
  }

  async attach(context: BrowserContext) {
    await context.route('**/api/**', (route) => this.handleRoute(route));
  }

  private nextCode() {
    if (this.presetCodes.length) {
      return this.presetCodes.shift() as string;
    }
    const code = (this.lobbyCounter++).toString(36).slice(-4).padStart(4, '0');
    return code.substring(0, 4).toUpperCase();
  }

  private nextPlayerId() {
    this.playerCounter += 1;
    return `player-${this.playerCounter}`;
  }

  private findLobby(code: string) {
    return this.lobbies.get(code.toUpperCase());
  }

  private buildLobbySnapshot(lobby: LobbyState): GameLobby {
    const readyCount = lobby.players.filter((player) => player.is_ready).length;
    return {
      code: lobby.code,
      leader_id: lobby.leaderId,
      rules: lobby.rules,
      players: lobby.players.map((player) => ({ ...player })),
      player_count: lobby.players.length,
      ready_player_count: readyCount,
      all_players_ready: lobby.players.length > 0 && readyCount === lobby.players.length,
      created_at_ms: lobby.createdAt,
      phase: lobby.phase,
      last_round: lobby.lastRound,
      round_history: lobby.roundHistory.slice(),
    };
  }

  private async handleRoute(route: Route) {
    const request = route.request();
    const url = new URL(request.url());
    const pathname = url.pathname;
    const method = request.method();

    if (pathname === '/api/content/categories' && method === 'GET') {
      await route.fulfill(jsonResponse(200, { categories: this.categories }));
      return;
    }

    if (pathname === '/api/games' && method === 'POST') {
      const payload = (await request.postDataJSON()) as CreateGamePayload;
      const code = this.nextCode() || 'ABCD';
      const hostId = this.nextPlayerId();
      const hostToken = `host-token-${hostId}`;
      const rules = { ...(payload.rules ?? defaultRules) };
      const player: PlayerState = {
        id: hostId,
        name: payload.host_name,
        crew_wins: 0,
        imposter_wins: 0,
        is_ready: false,
      };

      const lobby: LobbyState = {
        code,
        createdAt: Date.now(),
        hostToken,
        leaderId: hostId,
        players: [player],
        rules,
        phase: 'Lobby',
        lastRound: null,
        roundHistory: [],
        currentRound: null,
        roundSequence: 1,
      };
      this.lobbies.set(code, lobby);

      await route.fulfill(
        jsonResponse(200, {
          code,
          host_token: hostToken,
          leader_id: hostId,
          player_id: hostId,
          rules,
        }),
      );
      return;
    }

    const gameMatch = pathname.match(/^\/api\/games\/([A-Za-z0-9]{1,10})(.*)$/);
    if (!gameMatch) {
      await route.fulfill(notFound());
      return;
    }

    const lobbyCode = gameMatch[1].toUpperCase();
    const suffix = gameMatch[2];
    const lobby = this.findLobby(lobbyCode);
    if (!lobby) {
      await route.fulfill(notFound('Lobby not found'));
      return;
    }

    const segments = suffix.split('/').filter(Boolean);
    const action = segments[0];

    if (!action && method === 'GET') {
      await route.fulfill(jsonResponse(200, this.buildLobbySnapshot(lobby)));
      return;
    }

    if (action === 'join' && method === 'POST') {
      const payload = (await request.postDataJSON()) as JoinGamePayload;
      const playerId = this.nextPlayerId();
      lobby.players.push({
        id: playerId,
        name: payload.player_name,
        crew_wins: 0,
        imposter_wins: 0,
        is_ready: false,
      });
      await route.fulfill(jsonResponse(200, { code: lobby.code, player_id: playerId }));
      return;
    }

    if (action === 'ready' && method === 'POST') {
      const payload = (await request.postDataJSON()) as ReadyRequest;
      const player = lobby.players.find((item) => item.id === payload.player_id);
      if (!player) {
        await route.fulfill(notFound('Player not found'));
        return;
      }
      player.is_ready = payload.is_ready;
      await route.fulfill(jsonResponse(200, this.buildLobbySnapshot(lobby)));
      return;
    }

    if (action === 'start' && method === 'POST') {
      lobby.phase = 'InRound';
      const round: RoundPublicState = {
        round_number: lobby.roundSequence,
        turn_order: lobby.players.map((player) => player.id),
        current_turn_player_id: lobby.players[0]?.id ?? null,
        current_question: null,
        asked_questions: [],
        started_at_ms: Date.now(),
        resolution: null,
      };
      lobby.roundSequence += 1;
      lobby.currentRound = round;
      await route.fulfill(jsonResponse(200, round));
      return;
    }

    if (action === 'round' && method === 'GET') {
      if (lobby.currentRound) {
        await route.fulfill(jsonResponse(200, lobby.currentRound));
      } else {
        await route.fulfill(jsonResponse(200, null));
      }
      return;
    }

    if (action === 'round' && segments[1] === 'next' && method === 'POST') {
      lobby.phase = 'InRound';
      const round: RoundPublicState = {
        round_number: lobby.roundSequence,
        turn_order: lobby.players.map((player) => player.id),
        current_turn_player_id: lobby.players[0]?.id ?? null,
        current_question: null,
        asked_questions: [],
        started_at_ms: Date.now(),
        resolution: null,
      };
      lobby.roundSequence += 1;
      lobby.currentRound = round;
      await route.fulfill(jsonResponse(200, round));
      return;
    }

    await route.fulfill(notFound());
  }
}
