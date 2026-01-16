//! # Repository Traits
//!
//! Abstract repository interfaces for domain entities.
//! Implementations can be swapped for different backends (ScyllaDB, mock, etc.)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::Result;
use drone_domain::{
    Convoy, Drone, Engagement, LeaderboardEntry, Telemetry, TimeRange, Waypoint,
};

// =============================================================================
// CONVOY REPOSITORY
// =============================================================================

/// Repository for Convoy entity operations
#[async_trait]
pub trait ConvoyRepository: Send + Sync {
    /// Get convoy by ID
    async fn get_by_id(&self, convoy_id: Uuid) -> Result<Option<Convoy>>;

    /// Get all active convoys
    async fn get_active(&self) -> Result<Vec<Convoy>>;

    /// Create a new convoy
    async fn create(&self, convoy: &Convoy) -> Result<()>;

    /// Update convoy status
    async fn update_status(
        &self,
        convoy_id: Uuid,
        status: drone_domain::ConvoyStatus,
    ) -> Result<()>;

    /// Add drone to convoy roster
    async fn add_drone(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<()>;

    /// Remove drone from convoy roster
    async fn remove_drone(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<()>;

    /// Delete convoy
    async fn delete(&self, convoy_id: Uuid) -> Result<()>;
}

// =============================================================================
// DRONE REPOSITORY
// =============================================================================

/// Repository for Drone entity operations
#[async_trait]
pub trait DroneRepository: Send + Sync {
    /// Get drone by ID
    async fn get_by_id(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<Option<Drone>>;

    /// Get all drones in a convoy
    async fn get_by_convoy(&self, convoy_id: Uuid) -> Result<Vec<Drone>>;

    /// Get drones by status
    async fn get_by_status(&self, status: drone_domain::DroneStatus) -> Result<Vec<Drone>>;

    /// Create a new drone
    async fn create(&self, drone: &Drone) -> Result<()>;

    /// Update drone position and state
    async fn update_state(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        position: drone_domain::Coordinates,
        fuel_pct: f32,
        status: drone_domain::DroneStatus,
    ) -> Result<()>;

    /// Update drone accuracy stats
    async fn update_accuracy(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        total_engagements: i32,
        successful_hits: i32,
    ) -> Result<()>;

    /// Delete drone
    async fn delete(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<()>;
}

// =============================================================================
// WAYPOINT REPOSITORY
// =============================================================================

/// Repository for Waypoint entity operations
#[async_trait]
pub trait WaypointRepository: Send + Sync {
    /// Get all waypoints for a drone
    async fn get_by_drone(&self, drone_id: Uuid) -> Result<Vec<Waypoint>>;

    /// Get specific waypoint
    async fn get_by_sequence(
        &self,
        drone_id: Uuid,
        sequence_number: i16,
    ) -> Result<Option<Waypoint>>;

    /// Create waypoint
    async fn create(&self, waypoint: &Waypoint) -> Result<()>;

    /// Batch create waypoints for a drone
    async fn create_batch(&self, waypoints: &[Waypoint]) -> Result<()>;

    /// Update waypoint status and timing
    async fn update_status(
        &self,
        drone_id: Uuid,
        sequence_number: i16,
        status: drone_domain::WaypointStatus,
        actual_arrival: Option<DateTime<Utc>>,
        actual_departure: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Delete all waypoints for a drone
    async fn delete_by_drone(&self, drone_id: Uuid) -> Result<()>;
}

// =============================================================================
// TELEMETRY REPOSITORY
// =============================================================================

/// Repository for Telemetry entity operations
#[async_trait]
pub trait TelemetryRepository: Send + Sync {
    /// Get telemetry for a drone within a time range
    async fn get_by_drone_range(
        &self,
        drone_id: Uuid,
        range: TimeRange,
        limit: Option<i32>,
    ) -> Result<Vec<Telemetry>>;

    /// Get latest telemetry for a drone
    async fn get_latest(&self, drone_id: Uuid) -> Result<Option<Telemetry>>;

    /// Insert telemetry record
    async fn insert(&self, telemetry: &Telemetry) -> Result<()>;

    /// Batch insert telemetry records
    async fn insert_batch(&self, telemetry: &[Telemetry]) -> Result<()>;
}

// =============================================================================
// ENGAGEMENT REPOSITORY
// =============================================================================

/// Repository for Engagement entity operations
#[async_trait]
pub trait EngagementRepository: Send + Sync {
    /// Get engagements for a convoy
    async fn get_by_convoy(
        &self,
        convoy_id: Uuid,
        limit: Option<i32>,
    ) -> Result<Vec<Engagement>>;

    /// Get engagements for a drone
    async fn get_by_drone(
        &self,
        drone_id: Uuid,
        limit: Option<i32>,
    ) -> Result<Vec<Engagement>>;

    /// Get engagement by ID
    async fn get_by_id(
        &self,
        convoy_id: Uuid,
        engaged_at: DateTime<Utc>,
        engagement_id: Uuid,
    ) -> Result<Option<Engagement>>;

    /// Create engagement record
    async fn create(&self, engagement: &Engagement) -> Result<()>;

    /// Update engagement BDA status
    async fn update_bda(
        &self,
        convoy_id: Uuid,
        engaged_at: DateTime<Utc>,
        engagement_id: Uuid,
        bda_status: &str,
        bda_notes: Option<&str>,
    ) -> Result<()>;
}

// =============================================================================
// LEADERBOARD REPOSITORY
// =============================================================================

/// Repository for Leaderboard operations
#[async_trait]
pub trait LeaderboardRepository: Send + Sync {
    /// Get leaderboard for a convoy (sorted by accuracy desc)
    async fn get_leaderboard(
        &self,
        convoy_id: Uuid,
        limit: Option<i32>,
    ) -> Result<Vec<LeaderboardEntry>>;

    /// Get drone's position in leaderboard
    async fn get_rank(&self, convoy_id: Uuid, drone_id: Uuid) -> Result<Option<i16>>;

    /// Update leaderboard entry after engagement
    async fn update_entry(&self, entry: &LeaderboardEntry) -> Result<()>;

    /// Increment accuracy counters (atomic)
    async fn increment_counters(
        &self,
        convoy_id: Uuid,
        drone_id: Uuid,
        hit: bool,
    ) -> Result<(i64, i64)>; // Returns (total, hits)

    /// Rebuild leaderboard from drone stats
    async fn rebuild(&self, convoy_id: Uuid) -> Result<()>;
}

// =============================================================================
// UNIT OF WORK
// =============================================================================

/// Unit of Work pattern for transactional operations
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    type ConvoyRepo: ConvoyRepository;
    type DroneRepo: DroneRepository;
    type WaypointRepo: WaypointRepository;
    type TelemetryRepo: TelemetryRepository;
    type EngagementRepo: EngagementRepository;
    type LeaderboardRepo: LeaderboardRepository;

    /// Get convoy repository
    fn convoys(&self) -> &Self::ConvoyRepo;

    /// Get drone repository
    fn drones(&self) -> &Self::DroneRepo;

    /// Get waypoint repository
    fn waypoints(&self) -> &Self::WaypointRepo;

    /// Get telemetry repository
    fn telemetry(&self) -> &Self::TelemetryRepo;

    /// Get engagement repository
    fn engagements(&self) -> &Self::EngagementRepo;

    /// Get leaderboard repository
    fn leaderboard(&self) -> &Self::LeaderboardRepo;
}
