//! ScyllaDB repository implementation.

use chrono::Utc;
use scylla::{Session, SessionBuilder, FromRow};
use std::sync::Arc;
use uuid::Uuid;

use crate::cache::SharedCacheClient;
use crate::error::Result;
use crate::strategy::{ReadStrategy, WriteStrategy};
use drone_domain::{
    Convoy, ConvoyStatus, Engagement, LeaderboardEntry,
    MissionType, PlatformType, Telemetry, Waypoint, WeaponType,
};

// =============================================================================
// SCYLLA CONFIGURATION
// =============================================================================

/// ScyllaDB connection configuration.
#[derive(Debug, Clone)]
pub struct ScyllaConfig {
    pub hosts: Vec<String>,
    pub keyspace: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Default for ScyllaConfig {
    fn default() -> Self {
        Self {
            hosts: vec!["localhost:9042".to_string()],
            keyspace: "drone_ops".to_string(),
            username: None,
            password: None,
        }
    }
}

// =============================================================================
// SCYLLA CLIENT
// =============================================================================

/// ScyllaDB client wrapper.
pub struct ScyllaClient {
    session: Arc<Session>,
    pub config: ScyllaConfig,
}

impl ScyllaClient {
    /// Create a new ScyllaDB client.
    pub async fn new(config: ScyllaConfig) -> Result<Self> {
        let mut builder = SessionBuilder::new()
            .known_nodes(&config.hosts);

        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            builder = builder.user(user, pass);
        }

        let session = builder.build().await?;

        // Use keyspace
        session
            .query_unpaged(format!("USE {}", config.keyspace), ())
            .await?;

        Ok(Self {
            session: Arc::new(session),
            config,
        })
    }

    /// Get session reference.
    pub fn session(&self) -> &Session {
        &self.session
    }
}

// =============================================================================
// LEADERBOARD REPOSITORY
// =============================================================================

/// Repository for leaderboard operations.
pub struct ScyllaLeaderboardRepository {
    client: Arc<ScyllaClient>,
    cache: Option<SharedCacheClient>,
    read_strategy: ReadStrategy,
    write_strategy: WriteStrategy,
}

impl ScyllaLeaderboardRepository {
    /// Create a new leaderboard repository with default strategies.
    pub fn new(client: Arc<ScyllaClient>, cache: Option<SharedCacheClient>) -> Self {
        Self {
            client,
            cache,
            read_strategy: ReadStrategy::CacheFirst,
            write_strategy: WriteStrategy::WriteThrough,
        }
    }

    /// Create with custom strategies.
    pub fn with_strategies(
        client: Arc<ScyllaClient>,
        cache: Option<SharedCacheClient>,
        read_strategy: ReadStrategy,
        write_strategy: WriteStrategy,
    ) -> Self {
        Self {
            client,
            cache,
            read_strategy,
            write_strategy,
        }
    }

    /// Set read strategy.
    pub fn set_read_strategy(&mut self, strategy: ReadStrategy) {
        self.read_strategy = strategy;
    }

    /// Set write strategy.
    pub fn set_write_strategy(&mut self, strategy: WriteStrategy) {
        self.write_strategy = strategy;
    }

    /// Get leaderboard for a convoy.
    pub async fn get_leaderboard(
        &self,
        convoy_id: Uuid,
        limit: i32,
    ) -> Result<Vec<LeaderboardEntry>> {
        // Try cache first
        if let Some(ref cache) = self.cache {
            if let Ok(Some(entries)) = cache.get_leaderboard(convoy_id).await {
                return Ok(entries);
            }
        }

        // Query from database
        let query = r#"
            SELECT convoy_id, drone_id, callsign, platform_type, 
                   total_engagements, successful_hits, accuracy_pct, rank
            FROM leaderboard
            WHERE convoy_id = ?
            LIMIT ?
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id, limit))
            .await?;

        let mut entries = Vec::new();
        if let Some(rows) = result.rows {
            for row in rows {
                // Parse row manually
                let cols = row.columns;
                if cols.len() >= 8 {
                    let cid: Uuid = cols[0].as_ref()
                        .and_then(|v| v.as_uuid())
                        .unwrap_or(convoy_id);
                    let did: Uuid = cols[1].as_ref()
                        .and_then(|v| v.as_uuid())
                        .unwrap_or_default();
                    let callsign: String = cols[2].as_ref()
                        .and_then(|v| v.as_text())
                        .unwrap_or("UNKNOWN")
                        .to_string();
                    let platform: String = cols[3].as_ref()
                        .and_then(|v| v.as_text())
                        .unwrap_or("MQ9_REAPER")
                        .to_string();
                    let total: i32 = cols[4].as_ref()
                        .and_then(|v| v.as_int())
                        .unwrap_or(0);
                    let hits: i32 = cols[5].as_ref()
                        .and_then(|v| v.as_int())
                        .unwrap_or(0);
                    let accuracy: f32 = cols[6].as_ref()
                        .and_then(|v| v.as_float())
                        .unwrap_or(0.0);
                    let rank: i32 = cols[7].as_ref()
                        .and_then(|v| v.as_int())
                        .unwrap_or(0);
                    
                    entries.push(LeaderboardEntry {
                        convoy_id: cid,
                        drone_id: did,
                        callsign,
                        platform_type: platform.parse().unwrap_or(PlatformType::Mq9Reaper),
                        total_engagements: total as u32,
                        successful_hits: hits as u32,
                        accuracy_pct: accuracy,
                        rank: rank as u32,
                        last_updated: Utc::now(),
                    });
                }
            }
        }

        // Update cache
        if let Some(ref cache) = self.cache {
            let _ = cache.set_leaderboard(convoy_id, &entries).await;
        }

        Ok(entries)
    }

    /// Update leaderboard entry after engagement.
    pub async fn update_entry(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        hit: bool,
    ) -> Result<LeaderboardEntry> {
        // Get current stats
        let query = r#"
            SELECT total_engagements, successful_hits, callsign, platform_type
            FROM leaderboard
            WHERE convoy_id = ? AND drone_id = ?
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id, drone_id))
            .await?;

        let (total, hits, callsign, platform) = if let Some(rows) = result.rows {
            if let Some(row) = rows.into_iter().next() {
                let cols = row.columns;
                let t = cols.get(0).and_then(|c| c.as_ref()).and_then(|v| v.as_int()).unwrap_or(0);
                let h = cols.get(1).and_then(|c| c.as_ref()).and_then(|v| v.as_int()).unwrap_or(0);
                let c = cols.get(2).and_then(|c| c.as_ref()).and_then(|v| v.as_text()).unwrap_or("UNKNOWN").to_string();
                let p = cols.get(3).and_then(|c| c.as_ref()).and_then(|v| v.as_text()).unwrap_or("MQ9_REAPER").to_string();
                (t, h, c, p)
            } else {
                (0, 0, "UNKNOWN".to_string(), "MQ9_REAPER".to_string())
            }
        } else {
            (0, 0, "UNKNOWN".to_string(), "MQ9_REAPER".to_string())
        };

        let new_total = total + 1;
        let new_hits = if hit { hits + 1 } else { hits };
        let new_accuracy = if new_total > 0 {
            (new_hits as f32 / new_total as f32) * 100.0
        } else {
            0.0
        };

        // Update entry
        let update = r#"
            UPDATE leaderboard
            SET total_engagements = ?, 
                successful_hits = ?, 
                accuracy_pct = ?
            WHERE convoy_id = ? AND drone_id = ?
        "#;

        self.client.session
            .query_unpaged(update, (new_total, new_hits, new_accuracy, convoy_id, drone_id))
            .await?;

        // Invalidate cache
        if let Some(ref cache) = self.cache {
            let _ = cache.invalidate_leaderboard(convoy_id).await;
        }

        Ok(LeaderboardEntry {
            convoy_id,
            drone_id,
            callsign,
            platform_type: platform.parse().unwrap_or(PlatformType::Mq9Reaper),
            total_engagements: new_total as u32,
            successful_hits: new_hits as u32,
            accuracy_pct: new_accuracy,
            rank: 0,
            last_updated: Utc::now(),
        })
    }
}

// =============================================================================
// ENGAGEMENT REPOSITORY
// =============================================================================

/// Repository for engagement operations.
pub struct ScyllaEngagementRepository {
    client: Arc<ScyllaClient>,
}

impl ScyllaEngagementRepository {
    /// Create a new engagement repository.
    pub fn new(client: Arc<ScyllaClient>) -> Self {
        Self { client }
    }

    /// Record a new engagement.
    pub async fn record(&self, engagement: &Engagement) -> Result<()> {
        let query = r#"
            INSERT INTO engagements (
                convoy_id, drone_id, engagement_id, timestamp,
                weapon_type, hit, target_type, range_km, altitude_m
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.client.session
            .query_unpaged(
                query,
                (
                    engagement.convoy_id,
                    engagement.drone_id,
                    engagement.engagement_id,
                    engagement.timestamp,
                    engagement.weapon_type.to_string(),
                    engagement.hit,
                    engagement.target_type.as_ref().map(|t| t.to_string()),
                    engagement.range_km,
                    engagement.altitude_m,
                ),
            )
            .await?;

        Ok(())
    }

    /// Get engagements for a drone.
    pub async fn get_by_drone(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        limit: i32,
    ) -> Result<Vec<Engagement>> {
        let query = r#"
            SELECT convoy_id, drone_id, engagement_id, timestamp,
                   weapon_type, hit, target_type, range_km, altitude_m
            FROM engagements
            WHERE convoy_id = ? AND drone_id = ?
            LIMIT ?
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id, drone_id, limit))
            .await?;

        let mut engagements = Vec::new();
        if let Some(rows) = result.rows {
            for row in rows {
                let cols = row.columns;
                if cols.len() >= 9 {
                    let cid = cols[0].as_ref().and_then(|v| v.as_uuid()).unwrap_or(convoy_id);
                    let did = cols[1].as_ref().and_then(|v| v.as_uuid()).unwrap_or(drone_id);
                    let eid = cols[2].as_ref().and_then(|v| v.as_uuid()).unwrap_or_default();
                    let ts = Utc::now(); // Simplified - would parse from column
                    let weapon = cols[4].as_ref().and_then(|v| v.as_text()).unwrap_or("AGM114_HELLFIRE").to_string();
                    let hit = cols[5].as_ref().and_then(|v| v.as_boolean()).unwrap_or(false);
                    let target = cols[6].as_ref().and_then(|v| v.as_text()).map(|s| s.to_string());
                    let range = cols[7].as_ref().and_then(|v| v.as_double());
                    let alt = cols[8].as_ref().and_then(|v| v.as_double());

                    engagements.push(Engagement {
                        convoy_id: cid,
                        drone_id: did,
                        engagement_id: eid,
                        timestamp: ts,
                        weapon_type: weapon.parse().unwrap_or(WeaponType::Agm114Hellfire),
                        hit,
                        target_type: target.and_then(|t| t.parse().ok()),
                        range_km: range,
                        altitude_m: alt,
                    });
                }
            }
        }

        Ok(engagements)
    }
}

// =============================================================================
// TELEMETRY REPOSITORY
// =============================================================================

/// Repository for telemetry operations.
pub struct ScyllaTelemetryRepository {
    client: Arc<ScyllaClient>,
}

impl ScyllaTelemetryRepository {
    /// Create a new telemetry repository.
    pub fn new(client: Arc<ScyllaClient>) -> Self {
        Self { client }
    }

    /// Record telemetry snapshot.
    pub async fn record(&self, telemetry: &Telemetry) -> Result<()> {
        let query = r#"
            INSERT INTO telemetry (
                convoy_id, drone_id, timestamp,
                latitude, longitude, altitude_m,
                heading_deg, speed_mps, fuel_pct
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            USING TTL 86400
        "#;

        self.client.session
            .query_unpaged(
                query,
                (
                    telemetry.convoy_id,
                    telemetry.drone_id,
                    telemetry.timestamp,
                    telemetry.latitude,
                    telemetry.longitude,
                    telemetry.altitude_m,
                    telemetry.heading_deg,
                    telemetry.speed_mps,
                    telemetry.fuel_pct,
                ),
            )
            .await?;

        Ok(())
    }

    /// Get latest telemetry for a drone.
    pub async fn get_latest(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
    ) -> Result<Option<Telemetry>> {
        let query = r#"
            SELECT convoy_id, drone_id, timestamp,
                   latitude, longitude, altitude_m,
                   heading_deg, speed_mps, fuel_pct
            FROM telemetry
            WHERE convoy_id = ? AND drone_id = ?
            LIMIT 1
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id, drone_id))
            .await?;

        if let Some(rows) = result.rows {
            if let Some(row) = rows.into_iter().next() {
                let cols = row.columns;
                if cols.len() >= 9 {
                    return Ok(Some(Telemetry {
                        convoy_id: cols[0].as_ref().and_then(|v| v.as_uuid()).unwrap_or(convoy_id),
                        drone_id: cols[1].as_ref().and_then(|v| v.as_uuid()).unwrap_or(drone_id),
                        timestamp: Utc::now(),
                        latitude: cols[3].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                        longitude: cols[4].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                        altitude_m: cols[5].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                        heading_deg: cols[6].as_ref().and_then(|v| v.as_float()).unwrap_or(0.0),
                        speed_mps: cols[7].as_ref().and_then(|v| v.as_float()).unwrap_or(0.0),
                        fuel_pct: cols[8].as_ref().and_then(|v| v.as_float()).unwrap_or(0.0),
                    }));
                }
            }
        }

        Ok(None)
    }
}

// =============================================================================
// CONVOY REPOSITORY
// =============================================================================

/// Repository for convoy operations.
pub struct ScyllaConvoyRepository {
    client: Arc<ScyllaClient>,
}

impl ScyllaConvoyRepository {
    /// Create a new convoy repository.
    pub fn new(client: Arc<ScyllaClient>) -> Self {
        Self { client }
    }

    /// Get convoy by ID.
    pub async fn get(&self, convoy_id: Uuid) -> Result<Option<Convoy>> {
        let query = r#"
            SELECT convoy_id, callsign, mission_type, status,
                   created_at, updated_at
            FROM convoys
            WHERE convoy_id = ?
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id,))
            .await?;

        if let Some(rows) = result.rows {
            if let Some(row) = rows.into_iter().next() {
                let cols = row.columns;
                if cols.len() >= 6 {
                    return Ok(Some(Convoy {
                        convoy_id: cols[0].as_ref().and_then(|v| v.as_uuid()).unwrap_or(convoy_id),
                        callsign: cols[1].as_ref().and_then(|v| v.as_text()).unwrap_or("UNKNOWN").to_string(),
                        mission_type: cols[2].as_ref().and_then(|v| v.as_text()).unwrap_or("STRIKE").parse().unwrap_or(MissionType::Strike),
                        status: cols[3].as_ref().and_then(|v| v.as_text()).unwrap_or("ACTIVE").parse().unwrap_or(ConvoyStatus::Active),
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Create a new convoy.
    pub async fn create(&self, convoy: &Convoy) -> Result<()> {
        let query = r#"
            INSERT INTO convoys (
                convoy_id, callsign, mission_type, status,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?)
        "#;

        self.client.session
            .query_unpaged(
                query,
                (
                    convoy.convoy_id,
                    &convoy.callsign,
                    convoy.mission_type.to_string(),
                    convoy.status.to_string(),
                    convoy.created_at,
                    convoy.updated_at,
                ),
            )
            .await?;

        Ok(())
    }
}

// =============================================================================
// WAYPOINT REPOSITORY  
// =============================================================================

/// Repository for waypoint operations.
pub struct ScyllaWaypointRepository {
    client: Arc<ScyllaClient>,
}

impl ScyllaWaypointRepository {
    /// Create a new waypoint repository.
    pub fn new(client: Arc<ScyllaClient>) -> Self {
        Self { client }
    }

    /// Get waypoints for a drone.
    pub async fn get_waypoints(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
    ) -> Result<Vec<Waypoint>> {
        let query = r#"
            SELECT convoy_id, drone_id, waypoint_id, sequence,
                   name, latitude, longitude, altitude_m
            FROM waypoints
            WHERE convoy_id = ? AND drone_id = ?
        "#;

        let result = self.client.session
            .query_unpaged(query, (convoy_id, drone_id))
            .await?;

        let mut waypoints = Vec::new();
        if let Some(rows) = result.rows {
            for row in rows {
                let cols = row.columns;
                if cols.len() >= 8 {
                    waypoints.push(Waypoint {
                        convoy_id: cols[0].as_ref().and_then(|v| v.as_uuid()).unwrap_or(convoy_id),
                        drone_id: cols[1].as_ref().and_then(|v| v.as_uuid()).unwrap_or(drone_id),
                        waypoint_id: cols[2].as_ref().and_then(|v| v.as_uuid()).unwrap_or_default(),
                        sequence: cols[3].as_ref().and_then(|v| v.as_int()).unwrap_or(0) as u32,
                        name: cols[4].as_ref().and_then(|v| v.as_text()).unwrap_or("WP").to_string(),
                        latitude: cols[5].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                        longitude: cols[6].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                        altitude_m: cols[7].as_ref().and_then(|v| v.as_double()).unwrap_or(0.0),
                    });
                }
            }
        }

        Ok(waypoints)
    }
}
