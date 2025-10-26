# The Imposter

Mobile-first social deduction party game where one player is secretly the imposter and everyone else shares a location role. Players meet in person, ask questions, and try to uncover the imposter before time runs out.

- 🧭 Core mechanics & etiquette: see [`RULES.md`](RULES.md)
- 🗺️ Build roadmap & open tasks: see [`TODO.md`](TODO.md)
- 🌐 Production target: `theimposter.app` with Caddy TLS termination on DigitalOcean

This repository contains:

- **Rust backend** (`backend/`): real-time ready API for lobby management.
- **Svelte front-end** (`frontend/`): mobile-first web client for creating and joining lobbies.
- **Caddy gateway** (`deploy/caddy/`): TLS termination, static asset hosting, and reverse proxy routing.

## Getting Started

### Prerequisites

- Rust toolchain (1.82 or newer recommended)
- Node.js 22.12+ with npm

### Backend

```bash
cd backend
cargo run
```

The API listens on `http://localhost:8080`. Key routes:

- `POST /api/games` – create a lobby, returns room code, host token, and host player id.
- `POST /api/games/{code}/join` – join an existing lobby.
- `PATCH /api/games/{code}` – host-only rules update.
- `GET /api/games/{code}` – fetch lobby snapshot (players, rules, counts).
- `GET /healthz` – health probe for load balancers.

Run tests with:

```bash
cargo test
```

> The first test run will compile dependencies and can take a couple of minutes.

### Front-end

```bash
cd frontend
npm install
npm run dev
```

Visit `http://localhost:5173` for the dev UI. Configure an API base when proxying to remote servers via `.env`:

```
VITE_API_BASE=https://api.theimposter.app
```

Production build:

```bash
npm run build
```

### Docker Compose (local stack)

The repository ships with a Compose setup that builds both images and runs Caddy in front of the API:

```bash
docker compose up --build
```

The default domain is `theimposter.test`; add it to `/etc/hosts` pointing at `127.0.0.1` for local development. Override the domain during deployment:

```bash
DOMAIN=theimposter.app docker compose up --build -d
```

> For production, map ports 80/443 to the host and provide a valid DNS record so Caddy can obtain certificates automatically.

## Deployment Overview

1. **Backend image** (`backend/Dockerfile`): multi-stage build producing a slim Debian runtime with the compiled binary.
2. **Caddy image** (`deploy/caddy/Dockerfile`): builds the Svelte client, layers it into a Caddy image, and uses `deploy/caddy/Caddyfile` to proxy `/api/*` to the Rust service and serve the SPA.
3. **DigitalOcean droplet** (or App Platform) can run `docker compose` with both services. Persistent Caddy data/config volumes support automatic TLS renewals.

Suggested production environment variables:

- `DOMAIN=theimposter.app`
- `RUST_LOG=info,theimposter_backend=debug`

## Game Flow Highlights

1. Host creates a lobby, receives a short room code and management token.
2. Players enter the 4-character code on their phones to join.
3. Host can tweak rules (max players, round timers, question categories, location pool size) before the game starts.
4. During each round, all non-imposters receive location + role hints, while the imposter stays in the dark.
5. Players ask each other questions in person to deduce the imposter.
6. Any player can declare a guess at any time. Imposters guess the location; non-imposters accuse a player.
7. Wins are tracked separately for imposter victories and normal victories and surfaced in the lobby/end screen.

Future iterations can extend the in-memory game state with persistence, real-time messaging, or WebSocket-based turn coordination.

---

Happy sleuthing! 🎭
