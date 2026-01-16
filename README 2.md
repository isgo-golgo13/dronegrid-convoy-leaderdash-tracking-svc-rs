# 🛩️ Drone Convoy Tracker

**Military-grade drone convoy tracking system with real-time accuracy leaderboard**

[![Rust](https://img.shields.io/badge/rust-1.83+-orange.svg)](https://www.rust-lang.org)
[![ScyllaDB](https://img.shields.io/badge/scylladb-5.4-blue.svg)](https://www.scylladb.com)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

---

## Overview

A high-performance, real-time tactical HUD for tracking drone convoy operations in Afghanistan theater. Built entirely in Rust with a Leptos WASM frontend, async-graphql API, and ScyllaDB/Redis persistence layer.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         TACTICAL HUD                                │
│              Logo • Mission Clock • Connection Status               │
├──────────────┬─────────────────────────────┬────────────────────────┤
│  LEFT PANEL  │         MAIN AREA           │    RIGHT PANEL         │
│              │                             │                        │
│  Leaderboard │     Afghanistan Map         │   Convoy Stats         │
│  (Live)      │     (Leaflet + OSM)         │                        │
│              │                             │   Telemetry Chart      │
│  Drone List  │     Drone Markers           │   (Charming/ECharts)   │
│              │     Waypoint Paths          │                        │
│              │                             │   Engagement Feed      │
├──────────────┴─────────────────────────────┴────────────────────────┤
│                         STATUS BAR                                  │
└─────────────────────────────────────────────────────────────────────┘
```

## Features

- 📊 **Real-time Accuracy Leaderboard** - Track drone engagement accuracy with live updates
- 🗺️ **Tactical Map** - Afghanistan AOR with drone positions and waypoint paths
- 📈 **Telemetry Charts** - Altitude, fuel, and mission progress visualization
- 🔄 **WebSocket Subscriptions** - GraphQL subscriptions for live data streaming
- 🎯 **Engagement Feed** - Real-time hit/miss tracking with weapon types
- 🌙 **Military Dark Theme** - Tactical HUD aesthetic with CRT scanlines

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Leptos WASM    │────▶│  GraphQL API    │────▶│   ScyllaDB      │
│  Frontend       │     │  (async-graphql)│     │   (OLTP)        │
│                 │◀────│                 │◀────│                 │
└─────────────────┘     └────────┬────────┘     └─────────────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │     Redis       │
                        │   (Cache)       │
                        └─────────────────┘
```

### Crates

| Crate | Description |
|-------|-------------|
| `drone-domain` | Core domain entities, enums, value objects |
| `drone-persistence` | Repository pattern with cache-aside strategy |
| `drone-graphql-api` | Axum + async-graphql server with subscriptions |
| `drone-frontend` | Leptos WASM tactical HUD |

## Quick Start

### Prerequisites

- Rust 1.83+
- Docker & Docker Compose
- `trunk` (WASM bundler): `cargo install trunk`

### Development

```bash
# Install dependencies
make setup

# Start databases (ScyllaDB + Redis)
make dev-db

# Initialize schema
make db-init

# Start development servers (API + Frontend)
make dev
```

The HUD will be available at: http://localhost:3000

### Production Build

```bash
# Full production build
make prod

# Or build Docker images
make docker

# Start full stack
make docker-up
```

## GraphQL API

### Playground

Available at http://localhost:8080/graphql when `ENABLE_PLAYGROUND=true`

### Example Queries

```graphql
# Get leaderboard
query {
  leaderboard(convoyId: "uuid", limit: 10) {
    entries {
      rank
      callsign
      accuracyPct
      totalEngagements
      successfulHits
      currentStreak
    }
    averageAccuracy
  }
}

# Record engagement
mutation {
  recordEngagement(input: {
    convoyId: "uuid"
    droneId: "uuid"
    hit: true
    weaponType: AGM114_HELLFIRE
  }) {
    success
    newRank
    rankChange
    newAccuracyPct
  }
}

# Subscribe to live events
subscription {
  engagementEvents(convoyId: "uuid") {
    droneId
    callsign
    hit
    weaponType
    newAccuracyPct
  }
}
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVER_ADDR` | `0.0.0.0:8080` | API server address |
| `SCYLLA_HOSTS` | `localhost:9042` | ScyllaDB hosts |
| `SCYLLA_KEYSPACE` | `drone_ops` | ScyllaDB keyspace |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL |
| `ENABLE_PLAYGROUND` | `true` | Enable GraphQL Playground |
| `RUST_LOG` | `info` | Log level |

## Project Structure

```
drone-convoy-tracker/
├── Cargo.toml              # Workspace manifest
├── Makefile                # Build system
├── crates/
│   ├── drone-domain/       # Domain entities
│   ├── drone-persistence/  # Repository layer
│   ├── drone-graphql-api/  # GraphQL server
│   └── drone-frontend/     # Leptos WASM UI
├── schema/
│   ├── cql/                # ScyllaDB schema
│   └── redis/              # Redis cache patterns
└── docker/
    ├── docker-compose.yml  # Full stack
    ├── Dockerfile.api      # API image
    └── Dockerfile.frontend # Frontend image
```

## Make Targets

```bash
make help           # Show all targets
make setup          # Install dependencies
make dev            # Start dev environment
make test           # Run tests
make lint           # Run linters
make prod           # Production build
make docker         # Build Docker images
make clean          # Clean artifacts
```

## Tech Stack

- **Language**: Rust 1.83+
- **Frontend**: Leptos 0.7 (WASM), Charming (ECharts), Leaflet.js
- **API**: Axum 0.8, async-graphql 7.0
- **Database**: ScyllaDB 5.4
- **Cache**: Redis 7
- **Build**: Trunk, Make

## License

MIT License - See [LICENSE](LICENSE) for details.

---

**Classification: UNCLASSIFIED // FOR OFFICIAL USE ONLY**

*Built with ❤️ by EngineVector*
