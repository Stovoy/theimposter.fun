use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::SocketAddr,
    sync::Arc,
    time::SystemTime,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use rand::{Rng, distributions::Alphanumeric, thread_rng};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

type SharedState = Arc<AppState>;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    init_tracing();

    let state = Arc::new(AppState::default());
    let app = app_router(state);

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

fn app_router(state: SharedState) -> Router {
    Router::new()
        .route("/healthz", get(health_check))
        .route("/api/games", post(create_game))
        .route(
            "/api/games/:code",
            get(fetch_game_details).patch(update_rules),
        )
        .route("/api/games/:code/join", post(join_game))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[derive(Default)]
struct AppState {
    games: RwLock<HashMap<RoomCode, Game>>,
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

    fn as_str(&self) -> &str {
        &self.0
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
}

impl Game {
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
        }
    }
}

#[derive(Clone, Serialize)]
struct GameLobby {
    code: RoomCode,
    leader_id: Uuid,
    rules: GameRules,
    players: Vec<PlayerSummary>,
    player_count: u32,
    created_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct GameRules {
    max_players: u8,
    round_time_seconds: u16,
    allow_repeated_questions: bool,
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            max_players: 12,
            round_time_seconds: 120,
            allow_repeated_questions: false,
        }
    }
}

#[derive(Clone, Serialize)]
struct PlayerSummary {
    id: Uuid,
    name: String,
}

impl From<Player> for PlayerSummary {
    fn from(value: Player) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

#[derive(Clone)]
struct Player {
    id: Uuid,
    name: String,
    joined_at: SystemTime,
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
            joined_at: SystemTime::now(),
        })
    }
}

#[derive(Deserialize)]
struct CreateGameRequest {
    host_name: String,
    #[serde(default)]
    rules: Option<GameRules>,
}

#[derive(Serialize)]
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
    let rules = payload.rules.unwrap_or_default();
    let host_token = Uuid::new_v4();

    let mut games_lock = state.games.write().await;
    let existing_codes: HashSet<RoomCode> = games_lock.keys().cloned().collect();
    let code = RoomCode::generate(&existing_codes);

    let mut players = HashMap::new();
    players.insert(host_player.id, host_player.clone());

    let game = Game {
        code: code.clone(),
        host_token,
        rules: rules.clone(),
        leader_id: host_player.id,
        players,
        created_at: SystemTime::now(),
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

#[derive(Serialize)]
struct JoinGameResponse {
    player_id: Uuid,
    code: RoomCode,
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

    if game.players.len() >= game.rules.max_players as usize {
        return Err(AppError::BadRequest("game is full".into()));
    }

    let player = Player::new(payload.player_name)?;
    let player_id = player.id;
    game.players.insert(player_id, player);

    Ok((StatusCode::OK, Json(JoinGameResponse { player_id, code })))
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

    game.rules = payload.rules;
    Ok((StatusCode::OK, Json(game.lobby_view())))
}

async fn fetch_game_details(
    State(state): State<SharedState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let code = RoomCode::new(code)?;
    let games = state.games.read().await;
    let game = games
        .get(&code)
        .ok_or_else(|| AppError::NotFound("game not found".into()))?;
    Ok((StatusCode::OK, Json(game.lobby_view())))
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

#[derive(Serialize)]
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
    use tower::ServiceExt;

    #[tokio::test]
    async fn create_game_initializes_lobby() {
        let state = Arc::new(AppState::default());
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
        let state = Arc::new(AppState::default());
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
}
