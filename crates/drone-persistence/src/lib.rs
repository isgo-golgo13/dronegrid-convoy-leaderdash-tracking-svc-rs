//! # Drone Persistence Library
//!
//! Production-grade persistence layer for the Drone Convoy Tracking System.
//!
//! ## Architecture
//!
//! This crate implements the Repository pattern with pluggable Strategy pattern
//! for flexible cache/database access patterns:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     Application Layer                        │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Repository Traits                          │
//! │  (ConvoyRepository, DroneRepository, LeaderboardRepository)  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  Cached Repository Wrapper                   │
//! │              (applies read/write strategies)                 │
//! └─────────────────────────────────────────────────────────────┘
//!                    │                   │
//!                    ▼                   ▼
//! ┌─────────────────────────┐   ┌──────────────────────────────┐
//! │     Redis Cache         │   │        ScyllaDB              │
//! │  (Leaderboard, State)   │   │   (Source of Truth)          │
//! └─────────────────────────┘   └──────────────────────────────┘
//! ```
//!
//! ## Features
//!
//! - `scylla`: Enable ScyllaDB backend (default)
//! - `redis`: Enable Redis cache layer (default)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use drone_persistence::{
//!     cache::{CacheClient, CacheConfig},
//!     repository::{ScyllaClient, ScyllaConfig, ScyllaLeaderboardRepository},
//!     strategy::{CacheFirstStrategy, WriteThroughStrategy},
//! };
//!
//! // Initialize clients
//! let scylla = ScyllaClient::new(ScyllaConfig::default()).await?;
//! let cache = CacheClient::new(CacheConfig::default()).await?;
//!
//! // Create repository with caching
//! let leaderboard_repo = ScyllaLeaderboardRepository::new(
//!     scylla,
//!     Some(Arc::new(cache)),
//! );
//!
//! // Use repository
//! let leaderboard = leaderboard_repo.get_leaderboard(convoy_id, Some(10)).await?;
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

pub mod cache;
pub mod error;
pub mod repository;
pub mod strategy;

// Re-export commonly used types
pub use cache::{CacheClient, CacheConfig, SharedCacheClient};
pub use error::{PersistenceError, Result};
pub use repository::{
    ConvoyRepository, DroneRepository, EngagementRepository, LeaderboardRepository,
    ScyllaClient, ScyllaConfig, TelemetryRepository, WaypointRepository,
};
pub use strategy::{
    CacheFirstStrategy, DbOnlyStrategy, DynamicStrategy, ReadStrategy,
    WriteThroughStrategy, WriteStrategy,
};

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the persistence layer with default configuration
///
/// # Errors
///
/// Returns an error if either ScyllaDB or Redis connection fails.
pub async fn init_default() -> Result<(ScyllaClient, CacheClient)> {
    let scylla = ScyllaClient::new(ScyllaConfig::default()).await?;
    let cache = CacheClient::new(CacheConfig::default()).await?;
    Ok((scylla, cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
