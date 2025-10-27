use std::{
    collections::{HashMap, HashSet},
    fmt, io,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::{SinkExt, StreamExt};
use rand::{Rng, distributions::Alphanumeric, seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use uuid::Uuid;

type SharedState = Arc<AppState>;

#[derive(Debug, Clone, Deserialize)]
struct LocationDefinition {
    id: u32,
    name: String,
    roles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuestionPrompt {
    id: String,
    text: String,
    categories: Vec<String>,
}

#[derive(Clone)]
struct GameContent {
    locations: Vec<LocationDefinition>,
    questions: Vec<QuestionPrompt>,
    categories: Vec<String>,
}

impl GameContent {
    fn load() -> Result<Self, AppError> {
        let locations: Vec<LocationDefinition> =
            serde_json::from_str(include_str!("../data/locations.json"))
                .map_err(|err| AppError::Unexpected(Box::new(err)))?;
        let questions: Vec<QuestionPrompt> =
            serde_json::from_str(include_str!("../data/questions.json"))
                .map_err(|err| AppError::Unexpected(Box::new(err)))?;

        if locations.is_empty() {
            return Err(AppError::Unexpected(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                "no locations configured",
            ))));
        }
        if questions.is_empty() {
            return Err(AppError::Unexpected(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                "no questions configured",
            ))));
        }

        let mut categories: Vec<String> = questions
            .iter()
            .flat_map(|question| question.categories.iter().cloned())
            .map(|value| value.to_lowercase())
            .collect();
        categories.sort();
        categories.dedup();

        Ok(Self {
            locations,
            questions,
            categories,
        })
    }

    fn random_location_pool(
        &self,
        pool_size: usize,
        player_count: usize,
        rng: &mut impl Rng,
    ) -> Vec<LocationDefinition> {
        let mut candidates: Vec<_> = self
            .locations
            .iter()
            .filter(|loc| loc.roles.len() + 1 >= player_count)
            .collect();
        candidates.shuffle(rng);
        candidates.into_iter().take(pool_size).cloned().collect()
    }

    fn random_question<'a>(
        &'a self,
        categories: &[String],
        allow_repeats: bool,
        used_question_ids: &HashSet<String>,
        rng: &mut impl Rng,
    ) -> Option<&'a QuestionPrompt> {
        let normalized_categories: HashSet<String> = categories
            .iter()
            .map(|value| value.to_lowercase())
            .collect();

        let mut pool: Vec<&QuestionPrompt> =
            self.questions
                .iter()
                .filter(|question| {
                    normalized_categories.is_empty()
                        || question.categories.iter().any(|category| {
                            normalized_categories.contains(&category.to_lowercase())
                        })
                })
                .collect();

        if !allow_repeats {
            pool.retain(|question| !used_question_ids.contains(&question.id));
        }

        if pool.is_empty() {
            return None;
        }

        Some(pool.choose(rng).copied().unwrap())
    }

    fn default_categories(&self) -> Vec<String> {
        self.categories.clone()
    }

    fn normalize_categories(&self, requested: &[String]) -> Result<Vec<String>, AppError> {
        if requested.is_empty() {
            return Ok(self.categories.clone());
        }

        let valid: HashSet<&str> = self.categories.iter().map(String::as_str).collect();
        let mut cleaned = Vec::new();
        for category in requested {
            let normalized = category.trim().to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            if !valid.contains(normalized.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "unknown category: {}",
                    category
                )));
            }
            if !cleaned.iter().any(|value: &String| value == &normalized) {
                cleaned.push(normalized);
            }
        }

        if cleaned.is_empty() {
            Ok(self.categories.clone())
        } else {
            Ok(cleaned)
        }
    }

    fn max_location_pool(&self) -> usize {
        self.locations.len()
    }

    fn max_player_capacity(&self) -> u8 {
        self.locations
            .iter()
            .map(|location| (location.roles.len() + 1) as u8)
            .max()
            .unwrap_or(8)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
struct PlayerWins {
    crew: u32,
    imposter: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum RoundWinner {
    Crew,
    Imposter,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum RoundOutcome {
    CrewIdentifiedImposter {
        accuser: Uuid,
        impostor: Uuid,
    },
    CrewMisdirected {
        accuser: Uuid,
        accused: Uuid,
        impostor: Uuid,
    },
    ImposterIdentifiedLocation {
        impostor: Uuid,
        location_id: u32,
        location_name: String,
    },
    ImposterFailedLocationGuess {
        impostor: Uuid,
        guessed_location_id: u32,
        actual_location_id: u32,
        actual_location_name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RoundResolution {
    winner: RoundWinner,
    outcome: RoundOutcome,
    ended_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RoundSummary {
    round_number: u32,
    resolution: RoundResolution,
}

#[derive(Clone)]
struct RoundState {
    round_number: u32,
    location: LocationDefinition,
    imposter_id: Uuid,
    assignments: HashMap<Uuid, PlayerRoleAssignment>,
    turn_order: Vec<Uuid>,
    current_turn_index: usize,
    current_question: Option<QuestionPrompt>,
    used_question_ids: HashSet<String>,
    asked_questions: Vec<AskedQuestion>,
    started_at: SystemTime,
    resolution: Option<RoundResolution>,
}

impl RoundState {
    fn new(
        round_number: u32,
        location: LocationDefinition,
        players: &HashMap<Uuid, Player>,
        rules: &GameRules,
        content: &GameContent,
        rng: &mut impl Rng,
    ) -> Result<Self, AppError> {
        let player_count = players.len();
        if player_count < 3 {
            return Err(AppError::BadRequest(
                "at least three players are required to start a round".into(),
            ));
        }

        if player_count - 1 > location.roles.len() {
            return Err(AppError::BadRequest(
                "selected location does not support this many players".into(),
            ));
        }

        let mut player_ids: Vec<Uuid> = players.keys().cloned().collect();
        player_ids.shuffle(rng);
        let imposter_index = rng.gen_range(0..player_ids.len());
        let imposter_id = player_ids[imposter_index];

        let mut assignments = HashMap::new();
        let mut available_roles = location.roles.clone();
        available_roles.shuffle(rng);
        let mut role_iter = available_roles.into_iter();

        for player_id in &player_ids {
            if *player_id == imposter_id {
                assignments.insert(*player_id, PlayerRoleAssignment::Imposter);
            } else {
                let role = role_iter
                    .next()
                    .ok_or_else(|| AppError::BadRequest("not enough roles available".into()))?;
                assignments.insert(*player_id, PlayerRoleAssignment::Civilian { role });
            }
        }

        let mut turn_order = player_ids.clone();
        turn_order.shuffle(rng);

        let mut used_question_ids = HashSet::new();
        let initial_question = content
            .random_question(
                &rules.question_categories,
                rules.allow_repeated_questions,
                &used_question_ids,
                rng,
            )
            .cloned()
            .ok_or_else(|| {
                AppError::BadRequest("no questions available for selected categories".into())
            })?;
        used_question_ids.insert(initial_question.id.clone());

        Ok(Self {
            round_number,
            location,
            imposter_id,
            assignments,
            turn_order,
            current_turn_index: 0,
            current_question: Some(initial_question),
            used_question_ids,
            asked_questions: Vec::new(),
            started_at: SystemTime::now(),
            resolution: None,
        })
    }

    fn current_turn(&self) -> Option<Uuid> {
        if self.turn_order.is_empty() {
            return None;
        }
        let index = self.current_turn_index % self.turn_order.len();
        self.turn_order.get(index).copied()
    }

    fn is_active(&self) -> bool {
        self.resolution.is_none()
    }

    fn public_state(&self) -> RoundPublicState {
        RoundPublicState {
            round_number: self.round_number,
            turn_order: self.turn_order.clone(),
            current_turn_player_id: self.current_turn(),
            current_question: self.current_question.as_ref().map(QuestionView::from),
            asked_questions: self
                .asked_questions
                .iter()
                .map(AskedQuestionView::from)
                .collect(),
            started_at_ms: timestamp_ms(self.started_at),
            resolution: self.resolution.clone(),
        }
    }

    fn assignment_for(&self, player_id: &Uuid) -> Option<PlayerAssignmentView> {
        let assignment = self.assignments.get(player_id)?;
        match assignment {
            PlayerRoleAssignment::Imposter => Some(PlayerAssignmentView {
                round_number: self.round_number,
                is_imposter: true,
                location_id: None,
                location_name: None,
                role: None,
            }),
            PlayerRoleAssignment::Civilian { role } => Some(PlayerAssignmentView {
                round_number: self.round_number,
                is_imposter: false,
                location_id: Some(self.location.id),
                location_name: Some(self.location.name.clone()),
                role: Some(role.clone()),
            }),
        }
    }

    fn next_question(
        &mut self,
        player_id: Uuid,
        rules: &GameRules,
        content: &GameContent,
        rng: &mut impl Rng,
    ) -> Result<(QuestionPrompt, Uuid), AppError> {
        if !self.is_active() {
            return Err(AppError::BadRequest("round already resolved".into()));
        }

        let expected_turn = self
            .current_turn()
            .ok_or_else(|| AppError::BadRequest("no turn available".into()))?;
        if expected_turn != player_id {
            return Err(AppError::Forbidden("not your turn to draw".into()));
        }

        if let Some(current) = self.current_question.take() {
            self.asked_questions.push(AskedQuestion {
                id: current.id.clone(),
                text: current.text.clone(),
                categories: current
                    .categories
                    .iter()
                    .map(|category| category.to_lowercase())
                    .collect(),
                asked_by: player_id,
                asked_at: SystemTime::now(),
            });
        }

        if !self.turn_order.is_empty() {
            self.current_turn_index = (self.current_turn_index + 1) % self.turn_order.len();
        }

        let mut question = content
            .random_question(
                &rules.question_categories,
                rules.allow_repeated_questions,
                &self.used_question_ids,
                rng,
            )
            .cloned();

        if question.is_none() && !rules.allow_repeated_questions {
            self.used_question_ids.clear();
            question = content
                .random_question(
                    &rules.question_categories,
                    rules.allow_repeated_questions,
                    &self.used_question_ids,
                    rng,
                )
                .cloned();
        }

        let question = question
            .ok_or_else(|| AppError::BadRequest("no further questions available".into()))?;

        self.used_question_ids.insert(question.id.clone());
        let next_turn = self
            .current_turn()
            .ok_or_else(|| AppError::BadRequest("unable to determine next turn".into()))?;
        self.current_question = Some(question.clone());
        Ok((question, next_turn))
    }

    fn resolve_guess(
        &mut self,
        player_id: Uuid,
        action: GuessAction,
    ) -> Result<RoundResolution, AppError> {
        if !self.is_active() {
            return Err(AppError::BadRequest("round already resolved".into()));
        }

        let assignment = self
            .assignments
            .get(&player_id)
            .ok_or_else(|| AppError::BadRequest("player not part of this round".into()))?
            .clone();

        let ended_at_ms = timestamp_ms(SystemTime::now());

        let resolution = match (assignment, action) {
            (PlayerRoleAssignment::Imposter, GuessAction::GuessLocation { location_id }) => {
                let is_correct = location_id == self.location.id;

                if is_correct {
                    RoundResolution {
                        winner: RoundWinner::Imposter,
                        outcome: RoundOutcome::ImposterIdentifiedLocation {
                            impostor: player_id,
                            location_id: self.location.id,
                            location_name: self.location.name.clone(),
                        },
                        ended_at_ms,
                    }
                } else {
                    RoundResolution {
                        winner: RoundWinner::Crew,
                        outcome: RoundOutcome::ImposterFailedLocationGuess {
                            impostor: player_id,
                            guessed_location_id: location_id,
                            actual_location_id: self.location.id,
                            actual_location_name: self.location.name.clone(),
                        },
                        ended_at_ms,
                    }
                }
            }
            (PlayerRoleAssignment::Imposter, GuessAction::AccusePlayer { .. }) => {
                return Err(AppError::BadRequest(
                    "imposter must guess the location".into(),
                ));
            }
            (PlayerRoleAssignment::Civilian { .. }, GuessAction::AccusePlayer { accused_id }) => {
                if !self.assignments.contains_key(&accused_id) {
                    return Err(AppError::BadRequest("accused player not found".into()));
                }
                if accused_id == player_id {
                    return Err(AppError::BadRequest("you cannot accuse yourself".into()));
                }

                if accused_id == self.imposter_id {
                    RoundResolution {
                        winner: RoundWinner::Crew,
                        outcome: RoundOutcome::CrewIdentifiedImposter {
                            accuser: player_id,
                            impostor: self.imposter_id,
                        },
                        ended_at_ms,
                    }
                } else {
                    RoundResolution {
                        winner: RoundWinner::Imposter,
                        outcome: RoundOutcome::CrewMisdirected {
                            accuser: player_id,
                            accused: accused_id,
                            impostor: self.imposter_id,
                        },
                        ended_at_ms,
                    }
                }
            }
            (PlayerRoleAssignment::Civilian { .. }, GuessAction::GuessLocation { .. }) => {
                return Err(AppError::BadRequest(
                    "crew members must accuse a player".into(),
                ));
            }
        };

        self.resolution = Some(resolution.clone());
        Ok(resolution)
    }
}

#[derive(Clone)]
struct AskedQuestion {
    id: String,
    text: String,
    categories: Vec<String>,
    asked_by: Uuid,
    asked_at: SystemTime,
}

#[derive(Clone)]
enum PlayerRoleAssignment {
    Imposter,
    Civilian { role: String },
}

#[derive(Clone)]
enum GuessAction {
    AccusePlayer { accused_id: Uuid },
    GuessLocation { location_id: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct QuestionView {
    id: String,
    text: String,
    categories: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AskedQuestionView {
    id: String,
    text: String,
    categories: Vec<String>,
    asked_by: Uuid,
    asked_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RoundPublicState {
    round_number: u32,
    turn_order: Vec<Uuid>,
    current_turn_player_id: Option<Uuid>,
    current_question: Option<QuestionView>,
    asked_questions: Vec<AskedQuestionView>,
    started_at_ms: u64,
    resolution: Option<RoundResolution>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlayerAssignmentView {
    round_number: u32,
    is_imposter: bool,
    location_id: Option<u32>,
    location_name: Option<String>,
    role: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LocationOption {
    id: u32,
    name: String,
}

impl From<&QuestionPrompt> for QuestionView {
    fn from(value: &QuestionPrompt) -> Self {
        Self {
            id: value.id.clone(),
            text: value.text.clone(),
            categories: value
                .categories
                .iter()
                .map(|category| category.to_lowercase())
                .collect(),
        }
    }
}

impl From<&AskedQuestion> for AskedQuestionView {
    fn from(value: &AskedQuestion) -> Self {
        Self {
            id: value.id.clone(),
            text: value.text.clone(),
            categories: value.categories.clone(),
            asked_by: value.asked_by,
            asked_at_ms: timestamp_ms(value.asked_at),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    init_tracing();

    let content = GameContent::load()?;
    let state = Arc::new(AppState::new(content));
    let lobby_ttl = lobby_ttl_duration();
    let cleanup_interval = cleanup_interval_duration();
    state.spawn_cleanup(lobby_ttl, cleanup_interval);
    let app = app_router(Arc::clone(&state));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);

    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}

fn lobby_ttl_duration() -> Duration {
    const DEFAULT_TTL_SECS: u64 = 60 * 60;

    if let Some(seconds) = env_u64("LOBBY_TTL_SECONDS") {
        return Duration::from_secs(seconds);
    }

    if let Some(minutes) = env_u64("LOBBY_TTL_MINUTES") {
        return Duration::from_secs(minutes.saturating_mul(60));
    }

    Duration::from_secs(DEFAULT_TTL_SECS)
}

fn cleanup_interval_duration() -> Duration {
    const DEFAULT_INTERVAL_SECS: u64 = 5 * 60;

    if let Some(seconds) = env_u64("LOBBY_CLEANUP_INTERVAL_SECONDS") {
        return Duration::from_secs(seconds);
    }

    Duration::from_secs(DEFAULT_INTERVAL_SECS)
}

fn env_u64(var: &str) -> Option<u64> {
    match std::env::var(var) {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(value) => Some(value),
            Err(_) => {
                warn!(variable = %var, value = %raw, "failed to parse environment override as u64");
                None
            }
        },
        Err(_) => None,
    }
}

fn app_router(state: SharedState) -> Router {
    Router::new()
        .route("/healthz", get(health_check))
        .route("/api/games", post(create_game))
        .route(
            "/api/games/:code",
            get(fetch_game_details).patch(update_rules),
        )
        .route("/api/games/:code/join", post(join_game))
        .route("/api/games/:code/start", post(start_game))
        .route("/api/games/:code/abort", post(abort_game))
        .route("/api/games/:code/round", get(get_round_state))
        .route("/api/games/:code/stream", get(stream_game))
        .route("/api/games/:code/round/question", post(draw_next_question))
        .route("/api/games/:code/round/guess", post(submit_guess))
        .route("/api/games/:code/round/next", post(start_next_round))
        .route(
            "/api/games/:code/round/assignment/:player_id",
            get(get_assignment),
        )
        .route("/api/games/:code/locations", get(get_game_locations))
        .route("/api/content/categories", get(get_question_categories))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

struct AppState {
    games: RwLock<HashMap<RoomCode, Game>>,
    content: Arc<GameContent>,
}

impl AppState {
    fn new(content: GameContent) -> Self {
        Self {
            games: RwLock::new(HashMap::new()),
            content: Arc::new(content),
        }
    }

    fn content(&self) -> Arc<GameContent> {
        Arc::clone(&self.content)
    }

    async fn purge_expired_lobbies(&self, ttl: Duration) -> usize {
        if ttl.is_zero() {
            return 0;
        }

        let mut games = self.games.write().await;
        let now = SystemTime::now();
        let expired: Vec<RoomCode> = games
            .iter()
            .filter_map(|(code, game)| {
                if !matches!(game.phase, GamePhase::Lobby | GamePhase::AwaitingNextRound) {
                    return None;
                }
                match now.duration_since(game.last_active) {
                    Ok(elapsed) if elapsed >= ttl => Some(code.clone()),
                    _ => None,
                }
            })
            .collect();

        for code in &expired {
            games.remove(code);
        }

        if !expired.is_empty() {
            info!(count = expired.len(), "expired inactive lobbies");
        }

        expired.len()
    }

    fn spawn_cleanup(self: &Arc<Self>, ttl: Duration, interval: Duration) {
        if ttl.is_zero() {
            info!("lobby expiration disabled (ttl set to zero)");
            return;
        }

        let interval = if interval.is_zero() {
            Duration::from_secs(60)
        } else {
            interval
        };

        let state = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = state.purge_expired_lobbies(ttl).await;
            }
        });
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
struct RoomCode(String);

impl RoomCode {
    const LENGTH: usize = 4;

    fn new(value: String) -> Result<Self, AppError> {
        if value.len() != Self::LENGTH || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::BadRequest(
                "room codes are 4 alphanumeric characters".into(),
            ));
        }
        Ok(Self(value.to_uppercase()))
    }

    fn generate(existing: &HashSet<RoomCode>) -> Self {
        let mut rng = thread_rng();
        loop {
            let candidate: String = (0..Self::LENGTH)
                .map(|_| rng.sample(Alphanumeric) as char)
                .map(|c| c.to_ascii_uppercase())
                .collect();
            let code = Self(candidate);
            if !existing.contains(&code) {
                return code;
            }
        }
    }
}

impl fmt::Display for RoomCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone)]
struct Game {
    code: RoomCode,
    host_token: Uuid,
    rules: GameRules,
    leader_id: Uuid,
    players: HashMap<Uuid, Player>,
    created_at: SystemTime,
    last_active: SystemTime,
    round_counter: u32,
    phase: GamePhase,
    current_round: Option<RoundState>,
    last_round: Option<RoundSummary>,
    round_history: Vec<RoundSummary>,
    location_pool: Vec<LocationDefinition>,
    used_location_ids: HashSet<u32>,
    events: broadcast::Sender<GameEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct GameSnapshot {
    lobby: GameLobby,
    round: Option<RoundPublicState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GameEvent {
    Snapshot(GameSnapshot),
    Lobby { lobby: GameLobby },
    Round { round: Option<RoundPublicState> },
    Pong,
}

impl Game {
    fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            lobby: self.lobby_view(),
            round: self.current_round_view(),
        }
    }

    fn current_round_view(&self) -> Option<RoundPublicState> {
        self.current_round
            .as_ref()
            .map(|round| round.public_state())
    }

    fn lobby_view(&self) -> GameLobby {
        GameLobby {
            code: self.code.clone(),
            leader_id: self.leader_id,
            rules: self.rules.clone(),
            players: self
                .players
                .values()
                .cloned()
                .map(PlayerSummary::from)
                .collect(),
            player_count: self.players.len() as u32,
            created_at_ms: timestamp_ms(self.created_at),
            phase: self.phase,
            last_round: self.last_round.clone(),
            round_history: self.round_history.clone(),
        }
    }

    fn ensure_host(&self, token: &Uuid) -> Result<(), AppError> {
        if &self.host_token != token {
            return Err(AppError::Forbidden("host token invalid".into()));
        }
        Ok(())
    }

    fn ensure_player(&self, player_id: &Uuid) -> Result<(), AppError> {
        if !self.players.contains_key(player_id) {
            return Err(AppError::Forbidden("player not part of this game".into()));
        }
        Ok(())
    }

    fn touch(&mut self) {
        self.last_active = SystemTime::now();
    }

    fn location_options(&self) -> Vec<LocationOption> {
        self.location_pool
            .iter()
            .map(|location| LocationOption {
                id: location.id,
                name: location.name.clone(),
            })
            .collect()
    }

    fn round_state(&self) -> Result<&RoundState, AppError> {
        self.current_round
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("no active round".into()))
    }

    fn round_state_mut(&mut self) -> Result<&mut RoundState, AppError> {
        self.current_round
            .as_mut()
            .ok_or_else(|| AppError::BadRequest("no active round".into()))
    }

    fn public_round_state(&self) -> Result<RoundPublicState, AppError> {
        Ok(self.round_state()?.public_state())
    }

    fn assignment_for(&self, player_id: Uuid) -> Result<PlayerAssignmentView, AppError> {
        self.ensure_player(&player_id)?;
        self.round_state()?
            .assignment_for(&player_id)
            .ok_or_else(|| AppError::NotFound("assignment not found".into()))
    }

    fn begin_round(&mut self, content: &GameContent) -> Result<RoundPublicState, AppError> {
        match self.phase {
            GamePhase::Lobby | GamePhase::AwaitingNextRound => {}
            GamePhase::InRound => {
                return Err(AppError::BadRequest("round already in progress".into()));
            }
        }

        if self.players.len() < 3 {
            return Err(AppError::BadRequest(
                "at least three players are required to start".into(),
            ));
        }

        let mut rng = thread_rng();

        if self.location_pool.is_empty() {
            let pool_size =
                usize::from(self.rules.location_pool_size).min(content.max_location_pool());
            let pool = content.random_location_pool(pool_size, self.players.len(), &mut rng);
            if pool.is_empty() {
                return Err(AppError::BadRequest(
                    "no locations available for the current player count".into(),
                ));
            }
            self.location_pool = pool;
            self.used_location_ids.clear();
        }

        let mut candidates: Vec<LocationDefinition> = self
            .location_pool
            .iter()
            .filter(|location| location.roles.len() + 1 >= self.players.len())
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(AppError::BadRequest(
                "no locations support the current player count".into(),
            ));
        }

        candidates.shuffle(&mut rng);
        let selected = if let Some(location) = candidates
            .iter()
            .find(|location| !self.used_location_ids.contains(&location.id))
        {
            location.clone()
        } else {
            self.used_location_ids.clear();
            candidates
                .first()
                .cloned()
                .ok_or_else(|| AppError::BadRequest("no locations available".into()))?
        };

        let next_round_number = self.round_counter.saturating_add(1);
        let selected_id = selected.id;
        let round = RoundState::new(
            next_round_number,
            selected,
            &self.players,
            &self.rules,
            content,
            &mut rng,
        )?;

        self.round_counter = next_round_number;
        self.phase = GamePhase::InRound;
        self.current_round = Some(round);
        if let Some(public_state) = self
            .current_round
            .as_ref()
            .map(|current| current.public_state())
        {
            self.used_location_ids.insert(selected_id);
            self.last_round = None;
            self.touch();
            return Ok(public_state);
        }

        Err(AppError::Unexpected(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "round failed to initialize",
        ))))
    }

    fn draw_next_question(
        &mut self,
        player_id: Uuid,
        content: &GameContent,
    ) -> Result<NextQuestionResponse, AppError> {
        self.ensure_player(&player_id)?;
        let mut rng = thread_rng();
        let rules = self.rules.clone();
        let round = self.round_state_mut()?;
        let (question, next_player) = round.next_question(player_id, &rules, content, &mut rng)?;
        let asked_total = round.asked_questions.len();
        self.touch();
        Ok(NextQuestionResponse {
            question: QuestionView::from(&question),
            next_turn_player_id: next_player,
            asked_total,
        })
    }

    fn abort(&mut self, scope: AbortScope) -> Result<GameLobby, AppError> {
        match scope {
            AbortScope::Round => {
                if self.phase != GamePhase::InRound {
                    return Err(AppError::BadRequest(
                        "no active round is currently running".into(),
                    ));
                }
                if let Some(current) = self.current_round.as_ref() {
                    self.used_location_ids.remove(&current.location.id);
                }
                self.current_round = None;
                self.phase = GamePhase::AwaitingNextRound;
            }
            AbortScope::Game => {
                if let Some(current) = self.current_round.as_ref() {
                    self.used_location_ids.remove(&current.location.id);
                }
                self.current_round = None;
                self.phase = GamePhase::Lobby;
                self.last_round = None;
                self.round_counter = 0;
                self.location_pool.clear();
                self.used_location_ids.clear();
                self.round_history.clear();
            }
        }

        self.touch();
        Ok(self.lobby_view())
    }

    fn submit_guess(
        &mut self,
        player_id: Uuid,
        action: GuessAction,
    ) -> Result<RoundResolution, AppError> {
        self.ensure_player(&player_id)?;
        let (round_number, assignments, impostor_id, resolution) = {
            let round = self.round_state_mut()?;
            let resolution = round.resolve_guess(player_id, action)?;
            let assignments = round.assignments.clone();
            let impostor_id = round.imposter_id;
            (round.round_number, assignments, impostor_id, resolution)
        };

        match resolution.winner {
            RoundWinner::Crew => {
                for (player_id, assignment) in assignments {
                    if matches!(assignment, PlayerRoleAssignment::Civilian { .. }) {
                        if let Some(player) = self.players.get_mut(&player_id) {
                            player.wins.crew = player.wins.crew.saturating_add(1);
                        }
                    }
                }
            }
            RoundWinner::Imposter => {
                if let Some(player) = self.players.get_mut(&impostor_id) {
                    player.wins.imposter = player.wins.imposter.saturating_add(1);
                }
            }
        }

        let summary = RoundSummary {
            round_number,
            resolution: resolution.clone(),
        };
        self.last_round = Some(summary.clone());
        self.round_history.push(summary);
        self.phase = GamePhase::AwaitingNextRound;
        self.touch();
        Ok(resolution)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
enum GamePhase {
    Lobby,
    InRound,
    AwaitingNextRound,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GameLobby {
    code: RoomCode,
    leader_id: Uuid,
    rules: GameRules,
    players: Vec<PlayerSummary>,
    player_count: u32,
    created_at_ms: u64,
    phase: GamePhase,
    last_round: Option<RoundSummary>,
    round_history: Vec<RoundSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct GameRules {
    max_players: u8,
    round_time_seconds: u16,
    allow_repeated_questions: bool,
    location_pool_size: u8,
    question_categories: Vec<String>,
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            max_players: 8,
            round_time_seconds: 120,
            allow_repeated_questions: false,
            location_pool_size: 10,
            question_categories: Vec::new(),
        }
    }
}

impl GameRules {
    fn normalize(mut self, content: &GameContent) -> Result<Self, AppError> {
        let min_players: u8 = 3;
        let max_players = content.max_player_capacity().max(min_players);
        self.max_players = self.max_players.clamp(min_players, max_players);

        let min_round: u16 = 30;
        let max_round: u16 = 600;
        self.round_time_seconds = self.round_time_seconds.clamp(min_round, max_round);

        let min_pool: u8 = 1;
        if self.location_pool_size == 0 {
            self.location_pool_size = min_pool;
        }
        let max_pool = content.max_location_pool().max(usize::from(min_pool));
        let max_pool_u8 = max_pool.min(u8::MAX as usize) as u8;
        self.location_pool_size = self.location_pool_size.clamp(min_pool, max_pool_u8);

        self.question_categories = content.normalize_categories(&self.question_categories)?;
        Ok(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlayerSummary {
    id: Uuid,
    name: String,
    crew_wins: u32,
    imposter_wins: u32,
}

impl From<Player> for PlayerSummary {
    fn from(value: Player) -> Self {
        Self {
            id: value.id,
            name: value.name,
            crew_wins: value.wins.crew,
            imposter_wins: value.wins.imposter,
        }
    }
}

#[derive(Clone)]
struct Player {
    id: Uuid,
    name: String,
    wins: PlayerWins,
}

impl Player {
    fn new(name: String) -> Result<Self, AppError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("player name required".into()));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            name: trimmed.to_owned(),
            wins: PlayerWins::default(),
        })
    }
}

#[derive(Deserialize)]
struct CreateGameRequest {
    host_name: String,
    #[serde(default)]
    rules: Option<GameRules>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateGameResponse {
    code: RoomCode,
    host_token: Uuid,
    leader_id: Uuid,
    player_id: Uuid,
    rules: GameRules,
}

async fn create_game(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let host_player = Player::new(payload.host_name)?;
    let content = state.content();
    let rules = payload.rules.unwrap_or_default().normalize(&content)?;
    let host_token = Uuid::new_v4();

    let mut games_lock = state.games.write().await;
    let existing_codes: HashSet<RoomCode> = games_lock.keys().cloned().collect();
    let code = RoomCode::generate(&existing_codes);
    let (events_tx, _) = broadcast::channel(64);

    let mut players = HashMap::new();
    players.insert(host_player.id, host_player.clone());

    let game = Game {
        code: code.clone(),
        host_token,
        rules: rules.clone(),
        leader_id: host_player.id,
        players,
        created_at: SystemTime::now(),
        last_active: SystemTime::now(),
        round_counter: 0,
        phase: GamePhase::Lobby,
        current_round: None,
        last_round: None,
        round_history: Vec::new(),
        location_pool: Vec::new(),
        used_location_ids: HashSet::new(),
        events: events_tx.clone(),
    };

    games_lock.insert(code.clone(), game);
    drop(games_lock);

    let response = CreateGameResponse {
        code,
        host_token,
        leader_id: host_player.id,
        player_id: host_player.id,
        rules,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

#[derive(Deserialize)]
struct JoinGameRequest {
    player_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinGameResponse {
    player_id: Uuid,
    code: RoomCode,
}

#[derive(Deserialize)]
struct StartGameRequest {
    host_token: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AbortScope {
    Round,
    Game,
}

impl Default for AbortScope {
    fn default() -> Self {
        Self::Round
    }
}

#[derive(Deserialize)]
struct AbortRequest {
    host_token: Uuid,
    #[serde(default)]
    scope: AbortScope,
}

#[derive(Deserialize)]
struct NextQuestionRequest {
    player_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct NextQuestionResponse {
    question: QuestionView,
    next_turn_player_id: Uuid,
    asked_total: usize,
}

#[derive(Deserialize)]
struct GuessRequest {
    player_id: Uuid,
    #[serde(default)]
    accused_player_id: Option<Uuid>,
    #[serde(default)]
    location_id: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GuessResponse {
    resolution: RoundResolution,
}

#[derive(Deserialize)]
struct NextRoundRequest {
    host_token: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocationListResponse {
    locations: Vec<LocationOption>,
}

async fn join_game(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<JoinGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    if game.phase != GamePhase::Lobby {
        return Err(AppError::BadRequest("game already in progress".into()));
    }

    if game.players.len() >= game.rules.max_players as usize {
        return Err(AppError::BadRequest("game is full".into()));
    }

    let player = Player::new(payload.player_name)?;
    let player_id = player.id;
    game.players.insert(player_id, player);
    game.touch();
    let lobby_update = game.lobby_view();
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby_update.clone(),
    });

    Ok((StatusCode::OK, Json(JoinGameResponse { player_id, code })))
}

async fn start_game(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<StartGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let content = state.content();
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    game.ensure_host(&payload.host_token)?;
    let public_state = game.begin_round(content.as_ref())?;
    let lobby = game.lobby_view();
    let round_update = public_state.clone();
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby.clone(),
    });
    let _ = game.events.send(GameEvent::Round {
        round: Some(round_update.clone()),
    });
    Ok((StatusCode::OK, Json(public_state)))
}

#[derive(Deserialize)]
struct UpdateRulesRequest {
    host_token: Uuid,
    rules: GameRules,
}

async fn update_rules(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<UpdateRulesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    if payload.host_token != game.host_token {
        return Err(AppError::Forbidden("host token invalid".into()));
    }

    let content = state.content();
    game.rules = payload.rules.normalize(&content)?;
    game.touch();
    let lobby = game.lobby_view();
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby.clone(),
    });
    Ok((StatusCode::OK, Json(lobby)))
}

async fn fetch_game_details(
    State(state): State<SharedState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;
    game.touch();
    let lobby = game.lobby_view();
    drop(games);
    Ok((StatusCode::OK, Json(lobby)))
}

async fn get_round_state(
    State(state): State<SharedState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    let public_state = game.public_round_state()?;
    game.touch();
    drop(games);
    Ok((StatusCode::OK, Json(public_state)))
}

async fn stream_game(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let (events, snapshot) = {
        let games = state.games.read().await;
        let game = games
            .get(&code)
            .ok_or_else(|| AppError::NotFound("game not found".into()))?;
        (game.events.clone(), game.snapshot())
    };
    let state_clone = Arc::clone(&state);
    let code_clone = code.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        handle_socket(socket, state_clone, code_clone, events, snapshot).await;
    }))
}

async fn draw_next_question(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<NextQuestionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let content = state.content();
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    let response = game.draw_next_question(payload.player_id, content.as_ref())?;
    let round = game.public_round_state()?;
    let _ = game.events.send(GameEvent::Round {
        round: Some(round.clone()),
    });
    Ok((StatusCode::OK, Json(response)))
}

async fn submit_guess(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<GuessRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    let action = match (payload.accused_player_id, payload.location_id) {
        (Some(accused_id), None) => GuessAction::AccusePlayer { accused_id },
        (None, Some(location_id)) => GuessAction::GuessLocation { location_id },
        _ => {
            return Err(AppError::BadRequest(
                "provide an accused_player_id or location_id, but not both".into(),
            ));
        }
    };

    let resolution = game.submit_guess(payload.player_id, action)?;
    let round = game.public_round_state()?;
    let lobby = game.lobby_view();
    let _ = game.events.send(GameEvent::Round {
        round: Some(round.clone()),
    });
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby.clone(),
    });
    Ok((StatusCode::OK, Json(GuessResponse { resolution })))
}

async fn start_next_round(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<NextRoundRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let content = state.content();
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    game.ensure_host(&payload.host_token)?;
    let public_state = game.begin_round(content.as_ref())?;
    let lobby = game.lobby_view();
    let round_update = public_state.clone();
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby.clone(),
    });
    let _ = game.events.send(GameEvent::Round {
        round: Some(round_update.clone()),
    });
    Ok((StatusCode::OK, Json(public_state)))
}

async fn abort_game(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(payload): Json<AbortRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    game.ensure_host(&payload.host_token)?;
    let lobby = game.abort(payload.scope)?;
    let round = game.current_round_view();
    let _ = game.events.send(GameEvent::Lobby {
        lobby: lobby.clone(),
    });
    let _ = game.events.send(GameEvent::Round { round });
    Ok((StatusCode::OK, Json(lobby)))
}

async fn get_assignment(
    State(state): State<SharedState>,
    Path((code, player_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let player_id = Uuid::parse_str(&player_id)
        .map_err(|_| AppError::BadRequest("invalid player id".into()))?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    let assignment = game.assignment_for(player_id)?;
    game.touch();
    drop(games);
    Ok((StatusCode::OK, Json(assignment)))
}

async fn get_game_locations(
    State(state): State<SharedState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let mut games = state.games.write().await;
    let game = games
        .get_mut(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;

    if game.location_pool.is_empty() {
        return Err(AppError::BadRequest(
            "location pool has not been generated yet".into(),
        ));
    }

    game.touch();
    let locations = game.location_options();
    drop(games);
    Ok((StatusCode::OK, Json(LocationListResponse { locations })))
}

#[derive(Debug, Serialize, Deserialize)]
struct CategoriesResponse {
    categories: Vec<String>,
}

async fn get_question_categories(
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, AppError> {
    let content = state.content();
    Ok((
        StatusCode::OK,
        Json(CategoriesResponse {
            categories: content.default_categories(),
        }),
    ))
}

async fn handle_socket(
    socket: WebSocket,
    state: SharedState,
    code: RoomCode,
    events: broadcast::Sender<GameEvent>,
    initial: GameSnapshot,
) {
    info!(room = %code, "realtime subscriber connected");
    let (mut sender, mut receiver) = socket.split();
    if let Some(message) = event_message(&GameEvent::Snapshot(initial.clone())) {
        if sender.send(message).await.is_err() {
            let _ = sender.close().await;
            warn!(room = %code, "failed to deliver initial snapshot");
            return;
        }
    }

    let mut rx = events.subscribe();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
            inbound = receiver.next() => {
                match inbound {
                    Some(Ok(Message::Close(frame))) => {
                        let _ = sender.send(Message::Close(frame)).await;
                        break;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if text.trim().eq_ignore_ascii_case("ping") {
                            if let Some(msg) = event_message(&GameEvent::Pong) {
                                if sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) | Some(Ok(Message::Pong(_))) => {
                        // ignore
                    }
                    Some(Err(err)) => {
                        warn!(room = %code, error = %err, "websocket receive error");
                        break;
                    }
                    None => break,
                }
            }
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(event) => {
                        if let Some(message) = event_message(&event) {
                            if sender.send(message).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Some(snapshot) = latest_snapshot(&state, &code).await {
                            if let Some(message) = event_message(&GameEvent::Snapshot(snapshot)) {
                                if sender.send(message).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    let _ = sender.close().await;
    info!(room = %code, "realtime subscriber disconnected");
}

fn event_message(event: &GameEvent) -> Option<Message> {
    match serde_json::to_string(event) {
        Ok(payload) => Some(Message::Text(payload)),
        Err(err) => {
            warn!(error = %err, "failed to serialize game event");
            None
        }
    }
}

async fn latest_snapshot(state: &SharedState, code: &RoomCode) -> Option<GameSnapshot> {
    let games = state.games.read().await;
    games.get(code).map(Game::snapshot)
}

async fn health_check() -> &'static str {
    "ok"
}

fn timestamp_ms(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|dur| dur.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[derive(Debug, Error)]
enum AppError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error(transparent)]
    Unexpected(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.to_string();
        let body = Json(ErrorResponse { message });
        (status, body).into_response()
    }
}

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    message: String,
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Unexpected(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tower::ServiceExt;

    #[tokio::test]
    async fn create_game_initializes_lobby() {
        let content = GameContent::load().expect("content should load");
        let state = Arc::new(AppState::new(content));
        let app = super::app_router(state.clone());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_name": "Alice" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateGameResponse = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(created.rules.max_players, GameRules::default().max_players);
        assert_eq!(state.games.read().await.len(), 1);
        assert_eq!(
            state
                .games
                .read()
                .await
                .get(&created.code)
                .unwrap()
                .players
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn join_game_adds_player() {
        let content = GameContent::load().expect("content should load");
        let state = Arc::new(AppState::new(content));
        let app = super::app_router(state.clone());

        // Create a lobby to join.
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_name": "Alice" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateGameResponse = serde_json::from_slice(&body_bytes).unwrap();

        let join_uri = format!("/api/games/{}/join", created.code);
        let join_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(join_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "player_name": "Bob" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(join_response.status(), StatusCode::OK);

        let updated = state
            .games
            .read()
            .await
            .get(&created.code)
            .unwrap()
            .players
            .len();
        assert_eq!(updated, 2);
    }

    #[tokio::test]
    async fn host_can_start_game_without_readying() {
        let content = GameContent::load().expect("content should load");
        let state = Arc::new(AppState::new(content));
        let app = super::app_router(state.clone());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_name": "Alice" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateGameResponse = serde_json::from_slice(&body).unwrap();
        let code = format!("{}", created.code);
        let host_token = created.host_token;

        let mut player_ids = vec![created.player_id];
        for name in ["Bob", "Cara"] {
            let join_uri = format!("/api/games/{}/join", code);
            let join_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&join_uri)
                        .header("content-type", "application/json")
                        .body(Body::from(json!({ "player_name": name }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(join_response.status(), StatusCode::OK);
            let join_body = axum::body::to_bytes(join_response.into_body(), usize::MAX)
                .await
                .unwrap();
            let joined: JoinGameResponse = serde_json::from_slice(&join_body).unwrap();
            player_ids.push(joined.player_id);
        }

        let start_uri = format!("/api/games/{}/start", code);
        let start_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&start_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_token": host_token }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start_response.status(), StatusCode::OK);
        let start_body = axum::body::to_bytes(start_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let round: RoundPublicState = serde_json::from_slice(&start_body).unwrap();
        assert_eq!(round.round_number, 1);
        assert_eq!(round.turn_order.len(), player_ids.len());
    }

    #[tokio::test]
    async fn host_can_abort_round() {
        let content = GameContent::load().expect("content should load");
        let state = Arc::new(AppState::new(content));
        let app = super::app_router(state.clone());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_name": "Alice" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateGameResponse = serde_json::from_slice(&body).unwrap();
        let code = format!("{}", created.code);
        let host_token = created.host_token;

        let mut player_ids = vec![created.player_id];
        for name in ["Bob", "Cara"] {
            let join_uri = format!("/api/games/{}/join", code);
            let join_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&join_uri)
                        .header("content-type", "application/json")
                        .body(Body::from(json!({ "player_name": name }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(join_response.status(), StatusCode::OK);
            let join_body = axum::body::to_bytes(join_response.into_body(), usize::MAX)
                .await
                .unwrap();
            let joined: JoinGameResponse = serde_json::from_slice(&join_body).unwrap();
            player_ids.push(joined.player_id);
        }

        let start_uri = format!("/api/games/{}/start", code);
        let start_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&start_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_token": host_token }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start_response.status(), StatusCode::OK);
        let start_body = axum::body::to_bytes(start_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let started_round: RoundPublicState = serde_json::from_slice(&start_body).unwrap();
        assert_eq!(started_round.round_number, 1);
        assert_eq!(started_round.turn_order.len(), player_ids.len());

        let abort_uri = format!("/api/games/{}/abort", code);
        let abort_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&abort_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "host_token": host_token,
                            "scope": "round"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(abort_response.status(), StatusCode::OK);
        let abort_body = axum::body::to_bytes(abort_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let lobby: GameLobby = serde_json::from_slice(&abort_body).unwrap();
        assert_eq!(lobby.phase, GamePhase::AwaitingNextRound);

        let round_uri = format!("/api/games/{}/round", code);
        let round_fetch = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&round_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(round_fetch.status(), StatusCode::BAD_REQUEST);

        let next_round_uri = format!("/api/games/{}/round/next", code);
        let next_round_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&next_round_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_token": host_token }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(next_round_response.status(), StatusCode::OK);
        let next_round_body = axum::body::to_bytes(next_round_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resumed_round: RoundPublicState = serde_json::from_slice(&next_round_body).unwrap();
        assert_eq!(resumed_round.round_number, 2);
        assert_eq!(resumed_round.turn_order.len(), player_ids.len());
    }

    #[tokio::test]
    async fn imposter_wrong_location_guess_rewards_crew() {
        let content = GameContent::load().expect("content should load");
        let state = Arc::new(AppState::new(content));
        let app = super::app_router(state.clone());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "host_name": "Alice" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateGameResponse = serde_json::from_slice(&body).unwrap();
        let code = format!("{}", created.code);

        let mut player_ids = vec![created.player_id];
        for name in ["Bob", "Cara"] {
            let join_uri = format!("/api/games/{}/join", code);
            let join_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&join_uri)
                        .header("content-type", "application/json")
                        .body(Body::from(json!({ "player_name": name }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(join_response.status(), StatusCode::OK);
            let join_body = axum::body::to_bytes(join_response.into_body(), usize::MAX)
                .await
                .unwrap();
            let joined: JoinGameResponse = serde_json::from_slice(&join_body).unwrap();
            player_ids.push(joined.player_id);
        }

        let start_uri = format!("/api/games/{}/start", code);
        let start_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&start_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "host_token": created.host_token }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(start_response.status(), StatusCode::OK);
        let start_body = axum::body::to_bytes(start_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let round_state: RoundPublicState = serde_json::from_slice(&start_body).unwrap();
        assert_eq!(round_state.round_number, 1);
        assert_eq!(round_state.turn_order.len(), player_ids.len());
        let current_turn = round_state
            .current_turn_player_id
            .expect("round should provide first turn");

        let question_uri = format!("/api/games/{}/round/question", code);
        let question_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&question_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "player_id": current_turn }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(question_response.status(), StatusCode::OK);
        let question_body = axum::body::to_bytes(question_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let next_question: NextQuestionResponse = serde_json::from_slice(&question_body).unwrap();
        assert!(next_question.asked_total >= 1);

        let round_fetch_uri = format!("/api/games/{}/round", code);
        let round_fetch_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&round_fetch_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(round_fetch_response.status(), StatusCode::OK);
        let round_fetch_body = axum::body::to_bytes(round_fetch_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let refreshed_round: RoundPublicState = serde_json::from_slice(&round_fetch_body).unwrap();
        assert_eq!(
            refreshed_round.current_turn_player_id,
            Some(next_question.next_turn_player_id)
        );
        assert_eq!(
            refreshed_round.asked_questions.len(),
            next_question.asked_total
        );

        let mut assignments: HashMap<Uuid, PlayerAssignmentView> = HashMap::new();
        for player_id in &player_ids {
            let assign_uri = format!("/api/games/{}/round/assignment/{}", code, player_id);
            let assignment_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(&assign_uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(assignment_response.status(), StatusCode::OK);
            let assignment_body = axum::body::to_bytes(assignment_response.into_body(), usize::MAX)
                .await
                .unwrap();
            let assignment: PlayerAssignmentView =
                serde_json::from_slice(&assignment_body).unwrap();
            assignments.insert(*player_id, assignment);
        }

        let (imposter_id, location_id) = assignments.iter().fold(
            (None, None),
            |(mut imposter, mut location), (player_id, assignment)| {
                if assignment.is_imposter {
                    imposter = Some(*player_id);
                } else if location.is_none() {
                    location = assignment.location_id;
                }
                (imposter, location)
            },
        );

        let imposter_id = imposter_id.expect("expected one imposter");
        let location_id = location_id.expect("crew assignment should include location");

        let locations_uri = format!("/api/games/{}/locations", code);
        let locations_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&locations_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(locations_response.status(), StatusCode::OK);
        let locations_body = axum::body::to_bytes(locations_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let location_list: LocationListResponse = serde_json::from_slice(&locations_body).unwrap();
        assert!(!location_list.locations.is_empty());
        let wrong_location_id = location_list
            .locations
            .iter()
            .find(|option| option.id != location_id)
            .map(|option| option.id)
            .unwrap_or(location_id);

        assert_ne!(
            location_id, wrong_location_id,
            "need alternative location id"
        );

        let guess_uri = format!("/api/games/{}/round/guess", code);
        let guess_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&guess_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "player_id": imposter_id,
                            "location_id": wrong_location_id
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(guess_response.status(), StatusCode::OK);
        let guess_body = axum::body::to_bytes(guess_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let guess_result: GuessResponse = serde_json::from_slice(&guess_body).unwrap();

        assert!(matches!(guess_result.resolution.winner, RoundWinner::Crew));
        match guess_result.resolution.outcome {
            RoundOutcome::ImposterFailedLocationGuess {
                guessed_location_id,
                actual_location_id,
                ..
            } => {
                assert_eq!(guessed_location_id, wrong_location_id);
                assert_eq!(actual_location_id, location_id);
            }
            other => panic!("unexpected outcome: {:?}", other),
        }

        let lobby_uri = format!("/api/games/{}", code);
        let lobby_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&lobby_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(lobby_response.status(), StatusCode::OK);
        let lobby_body = axum::body::to_bytes(lobby_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let lobby: GameLobby = serde_json::from_slice(&lobby_body).unwrap();
        assert_eq!(lobby.phase, GamePhase::AwaitingNextRound);

        for player in lobby.players {
            if player.id == imposter_id {
                assert_eq!(player.imposter_wins, 0);
            } else {
                assert_eq!(player.crew_wins, 1);
            }
        }
    }
}
