//! # ScyllaDB Repository Implementations
//!
//! Concrete implementations of repository traits using ScyllaDB.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scylla::prepared_statement::PreparedStatement;
use scylla::transport::session::Session;
use scylla::{IntoTypedRows, SessionBuilder};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::cache::{CacheClient, SharedCacheClient};
use crate::error::{PersistenceError, Result};
use crate::repository::traits::*;
use crate::strategy::{ReadStrategy, SharedStrategy, WriteStrategy, SharedWriteStrategy};
use drone_domain::*;

// =============================================================================
// SCYLLA CLIENT
// =============================================================================

/// ScyllaDB client configuration
#[derive(Debug, Clone)]
pub struct ScyllaConfig {
    pub hosts: Vec<String>,
    pub keyspace: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub pool_size: usize,
}

impl Default for ScyllaConfig {
    fn default() -> Self {
        Self {
            hosts: vec!["127.0.0.1:9042".to_string()],
            keyspace: "drone_ops".to_string(),
            username: None,
            password: None,
            pool_size: 10,
        }
    }
}

/// ScyllaDB session wrapper
#[derive(Clone)]
pub struct ScyllaClient {
    session: Arc<Session>,
    prepared_stmts: Arc<PreparedStatements>,
}

/// Pre-prepared statements for performance
struct PreparedStatements {
    // Convoy statements
    get_convoy: PreparedStatement,
    get_active_convoys: PreparedStatement,
    insert_convoy: PreparedStatement,

    // Drone statements
    get_drone: PreparedStatement,
    get_drones_by_convoy: PreparedStatement,
    insert_drone: PreparedStatement,
    update_drone_state: PreparedStatement,
    update_drone_accuracy: PreparedStatement,

    // Waypoint statements
    get_waypoints: PreparedStatement,
    get_waypoint: PreparedStatement,
    insert_waypoint: PreparedStatement,

    // Telemetry statements
    get_telemetry_range: PreparedStatement,
    insert_telemetry: PreparedStatement,

    // Engagement statements
    get_engagements_by_convoy: PreparedStatement,
    get_engagements_by_drone: PreparedStatement,
    insert_engagement: PreparedStatement,
    insert_engagement_by_drone: PreparedStatement,

    // Leaderboard statements
    get_leaderboard: PreparedStatement,
    update_accuracy_counter: PreparedStatement,
    update_leaderboard_entry: PreparedStatement,
}

impl ScyllaClient {
    /// Create a new ScyllaDB client
    pub async fn new(config: ScyllaConfig) -> Result<Self> {
        let mut builder = SessionBuilder::new()
            .known_nodes(&config.hosts);

        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            builder = builder.user(user, pass);
        }

        let session = builder.build().await?;
        session.use_keyspace(&config.keyspace, false).await?;

        let prepared_stmts = Self::prepare_statements(&session).await?;

        Ok(Self {
            session: Arc::new(session),
            prepared_stmts: Arc::new(prepared_stmts),
        })
    }

    async fn prepare_statements(session: &Session) -> Result<PreparedStatements> {
        Ok(PreparedStatements {
            // Convoy
            get_convoy: session
                .prepare("SELECT * FROM convoys WHERE convoy_id = ?")
                .await?,
            get_active_convoys: session
                .prepare("SELECT * FROM active_convoys WHERE status = 'ACTIVE'")
                .await?,
            insert_convoy: session
                .prepare(
                    "INSERT INTO convoys (convoy_id, convoy_callsign, mission_id, mission_type, \
                     status, created_at, mission_start, aor_name, commanding_unit, drone_ids, drone_count) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,

            // Drone
            get_drone: session
                .prepare("SELECT * FROM drones WHERE convoy_id = ? AND drone_id = ?")
                .await?,
            get_drones_by_convoy: session
                .prepare("SELECT * FROM drones WHERE convoy_id = ?")
                .await?,
            insert_drone: session
                .prepare(
                    "INSERT INTO drones (convoy_id, drone_id, tail_number, callsign, platform_type, \
                     serial_number, status, fuel_remaining_pct, total_engagements, successful_hits, \
                     accuracy_pct, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,
            update_drone_state: session
                .prepare(
                    "UPDATE drones SET status = ?, fuel_remaining_pct = ?, updated_at = ? \
                     WHERE convoy_id = ? AND drone_id = ?",
                )
                .await?,
            update_drone_accuracy: session
                .prepare(
                    "UPDATE drones SET total_engagements = ?, successful_hits = ?, \
                     accuracy_pct = ?, updated_at = ? WHERE convoy_id = ? AND drone_id = ?",
                )
                .await?,

            // Waypoint
            get_waypoints: session
                .prepare("SELECT * FROM waypoints WHERE drone_id = ?")
                .await?,
            get_waypoint: session
                .prepare("SELECT * FROM waypoints WHERE drone_id = ? AND sequence_number = ?")
                .await?,
            insert_waypoint: session
                .prepare(
                    "INSERT INTO waypoints (drone_id, sequence_number, waypoint_id, waypoint_name, \
                     waypoint_type, planned_arrival, status) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,

            // Telemetry
            get_telemetry_range: session
                .prepare(
                    "SELECT * FROM telemetry WHERE drone_id = ? AND time_bucket = ? \
                     AND recorded_at >= ? AND recorded_at <= ? LIMIT ?",
                )
                .await?,
            insert_telemetry: session
                .prepare(
                    "INSERT INTO telemetry (drone_id, time_bucket, recorded_at, fuel_remaining_pct, \
                     current_waypoint, velocity_mps, mesh_connectivity) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,

            // Engagement
            get_engagements_by_convoy: session
                .prepare(
                    "SELECT * FROM engagements WHERE convoy_id = ? LIMIT ?",
                )
                .await?,
            get_engagements_by_drone: session
                .prepare(
                    "SELECT * FROM engagements_by_drone WHERE drone_id = ? LIMIT ?",
                )
                .await?,
            insert_engagement: session
                .prepare(
                    "INSERT INTO engagements (convoy_id, engaged_at, engagement_id, drone_id, \
                     drone_callsign, weapon_type, hit, bda_status) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,
            insert_engagement_by_drone: session
                .prepare(
                    "INSERT INTO engagements_by_drone (drone_id, engaged_at, engagement_id, \
                     convoy_id, weapon_type, hit) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .await?,

            // Leaderboard
            get_leaderboard: session
                .prepare(
                    "SELECT * FROM leaderboard WHERE convoy_id = ? LIMIT ?",
                )
                .await?,
            update_accuracy_counter: session
                .prepare(
                    "UPDATE accuracy_counters SET total_engagements = total_engagements + 1, \
                     successful_hits = successful_hits + ? WHERE convoy_id = ? AND drone_id = ?",
                )
                .await?,
            update_leaderboard_entry: session
                .prepare(
                    "INSERT INTO leaderboard (convoy_id, accuracy_pct, drone_id, callsign, \
                     platform_type, total_engagements, successful_hits, rank, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .await?,
        })
    }

    /// Get raw session for advanced queries
    pub fn session(&self) -> Arc<Session> {
        self.session.clone()
    }
}

// =============================================================================
// CACHED REPOSITORY WRAPPER
// =============================================================================

/// Repository wrapper that adds caching layer
pub struct CachedRepository<R> {
    inner: R,
    cache: SharedCacheClient,
    read_strategy: SharedStrategy,
    write_strategy: SharedWriteStrategy,
}

impl<R> CachedRepository<R> {
    pub fn new(
        inner: R,
        cache: SharedCacheClient,
        read_strategy: SharedStrategy,
        write_strategy: SharedWriteStrategy,
    ) -> Self {
        Self {
            inner,
            cache,
            read_strategy,
            write_strategy,
        }
    }
}

// =============================================================================
// SCYLLA LEADERBOARD REPOSITORY
// =============================================================================

/// ScyllaDB implementation of LeaderboardRepository
pub struct ScyllaLeaderboardRepository {
    client: ScyllaClient,
    cache: Option<SharedCacheClient>,
}

impl ScyllaLeaderboardRepository {
    pub fn new(client: ScyllaClient, cache: Option<SharedCacheClient>) -> Self {
        Self { client, cache }
    }
}

#[async_trait]
impl LeaderboardRepository for ScyllaLeaderboardRepository {
    async fn get_leaderboard(
        &self,
        convoy_id: Uuid,
        limit: Option<i32>,
    ) -> Result<Vec<LeaderboardEntry>> {
        let limit = limit.unwrap_or(10);

        // Try cache first if available
        if let Some(cache) = &self.cache {
            if let Ok(cached) = cache.get_leaderboard(convoy_id, limit as usize).await {
                if !cached.is_empty() {
                    tracing::debug!(convoy_id = %convoy_id, "Leaderboard cache hit");
                    // Convert cached data to LeaderboardEntry
                    // (simplified - in practice would need full data)
                    let entries: Vec<LeaderboardEntry> = cached
                        .into_iter()
                        .enumerate()
                        .map(|(idx, (drone_id, accuracy))| LeaderboardEntry {
                            convoy_id,
                            drone_id,
                            callsign: String::new(), // Would need to fetch
                            platform_type: PlatformType::Mq9Reaper,
                            accuracy_pct: accuracy as f32,
                            total_engagements: 0,
                            successful_hits: 0,
                            current_streak: 0,
                            best_streak: 0,
                            rank: (idx + 1) as i16,
                            updated_at: Utc::now(),
                        })
                        .collect();
                    return Ok(entries);
                }
            }
        }

        // Query ScyllaDB
        let result = self
            .client
            .session
            .execute(&self.client.prepared_stmts.get_leaderboard, (convoy_id, limit))
            .await?;

        let mut entries = Vec::new();
        if let Some(rows) = result.rows {
            for row in rows {
                // Parse row into LeaderboardEntry
                // (simplified - actual implementation would parse all columns)
                let (cid, accuracy, did): (Uuid, f32, Uuid) = row.into_typed()?;
                entries.push(LeaderboardEntry {
                    convoy_id: cid,
                    drone_id: did,
                    callsign: String::new(),
                    platform_type: PlatformType::Mq9Reaper,
                    accuracy_pct: accuracy,
                    total_engagements: 0,
                    successful_hits: 0,
                    current_streak: 0,
                    best_streak: 0,
                    rank: (entries.len() + 1) as i16,
                    updated_at: Utc::now(),
                });
            }
        }

        // Populate cache
        if let Some(cache) = &self.cache {
            for entry in &entries {
                let _ = cache
                    .update_leaderboard_score(convoy_id, entry.drone_id, entry.accuracy_pct as f64)
                    .await;
            }
        }

        Ok(entries)
    }

    async fn get_rank(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<Option<i16>> {
        // Try cache first
        if let Some(cache) = &self.cache {
            if let Ok(Some(rank)) = cache.get_drone_rank(convoy_id, drone_id).await {
                return Ok(Some((rank + 1) as i16)); // Convert 0-indexed to 1-indexed
            }
        }

        // Fallback to computing from full leaderboard
        let leaderboard = self.get_leaderboard(convoy_id, Some(100)).await?;
        for (idx, entry) in leaderboard.iter().enumerate() {
            if entry.drone_id == drone_id {
                return Ok(Some((idx + 1) as i16));
            }
        }

        Ok(None)
    }

    async fn update_entry(&self, entry: &LeaderboardEntry) -> Result<()> {
        // Update ScyllaDB
        self.client
            .session
            .execute(
                &self.client.prepared_stmts.update_leaderboard_entry,
                (
                    entry.convoy_id,
                    entry.accuracy_pct,
                    entry.drone_id,
                    &entry.callsign,
                    entry.platform_type.as_str(),
                    entry.total_engagements,
                    entry.successful_hits,
                    entry.rank,
                    entry.updated_at,
                ),
            )
            .await?;

        // Update cache
        if let Some(cache) = &self.cache {
            cache
                .update_leaderboard_score(
                    entry.convoy_id,
                    entry.drone_id,
                    entry.accuracy_pct as f64,
                )
                .await?;
        }

        Ok(())
    }

    async fn increment_counters(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        hit: bool,
    ) -> Result<(i64, i64)> {
        let hit_increment: i64 = if hit { 1 } else { 0 };

        // Update ScyllaDB counter
        self.client
            .session
            .execute(
                &self.client.prepared_stmts.update_accuracy_counter,
                (hit_increment, convoy_id, drone_id),
            )
            .await?;

        // Update cache and get new values
        if let Some(cache) = &self.cache {
            let (total, hits) = cache.increment_engagements(drone_id, hit).await?;
            return Ok((total, hits));
        }

        // If no cache, return placeholder (actual values would need separate query)
        Ok((1, hit_increment))
    }

    async fn rebuild(&self, convoy_id: Uuid) -> Result<()> {
        // Clear existing leaderboard in cache
        if let Some(cache) = &self.cache {
            cache.invalidate_convoy(convoy_id).await?;
        }

        // Query all drones in convoy and rebuild
        let result = self
            .client
            .session
            .execute(
                &self.client.prepared_stmts.get_drones_by_convoy,
                (convoy_id,),
            )
            .await?;

        // Re-populate leaderboard
        // (simplified - would parse and sort all drones)

        Ok(())
    }
}

// =============================================================================
// SHARED CLIENT TYPE
// =============================================================================

pub type SharedScyllaClient = Arc<ScyllaClient>;

pub fn shared_scylla(client: ScyllaClient) -> SharedScyllaClient {
    Arc::new(client)
}
