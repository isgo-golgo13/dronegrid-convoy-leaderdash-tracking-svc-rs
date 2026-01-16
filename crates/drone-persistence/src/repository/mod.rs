//! # Repository Module
//!
//! Repository pattern implementations for domain entity persistence.

pub mod traits;
pub mod scylla_impl;

pub use traits::*;
pub use scylla_impl::{
    CachedRepository, ScyllaClient, ScyllaConfig, ScyllaLeaderboardRepository,
    SharedScyllaClient, shared_scylla,
};
