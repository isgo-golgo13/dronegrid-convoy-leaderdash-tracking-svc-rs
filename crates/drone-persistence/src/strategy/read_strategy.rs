//! # Read Strategy Pattern
//!
//! Pluggable strategies for reading data from the persistence layer.
//! Implements the Strategy pattern for flexible cache/DB access patterns.

use async_trait::async_trait;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;

use crate::error::{PersistenceError, Result};

/// Strategy for reading data from persistence layer
#[async_trait]
pub trait ReadStrategy: Send + Sync + Debug {
    /// Execute a read operation with the given cache key and DB fallback
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send;

    /// Strategy name for metrics/logging
    fn name(&self) -> &'static str;
}

// =============================================================================
// STRATEGY IMPLEMENTATIONS
// =============================================================================

/// Cache-First Strategy
/// 1. Check cache
/// 2. On hit: return cached value
/// 3. On miss: query DB, populate cache, return value
#[derive(Debug, Clone, Default)]
pub struct CacheFirstStrategy {
    /// Whether to populate cache on miss
    pub populate_on_miss: bool,
}

impl CacheFirstStrategy {
    pub fn new(populate_on_miss: bool) -> Self {
        Self { populate_on_miss }
    }
}

#[async_trait]
impl ReadStrategy for CacheFirstStrategy {
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send,
    {
        tracing::debug!(strategy = "cache_first", key = cache_key, "Executing read");

        // Try cache first
        match cache_fn().await {
            Ok(Some(value)) => {
                tracing::debug!(key = cache_key, "Cache hit");
                return Ok(value);
            }
            Ok(None) => {
                tracing::debug!(key = cache_key, "Cache miss");
            }
            Err(e) => {
                tracing::warn!(key = cache_key, error = %e, "Cache error, falling back to DB");
            }
        }

        // Fall back to DB
        let value = db_fn().await?;
        tracing::debug!(key = cache_key, "DB read successful");

        // Note: Cache population is handled by the repository layer
        // since it needs access to the cache client

        Ok(value)
    }

    fn name(&self) -> &'static str {
        "cache_first"
    }
}

/// Database-Only Strategy
/// Always queries the database, bypasses cache entirely
#[derive(Debug, Clone, Copy, Default)]
pub struct DbOnlyStrategy;

#[async_trait]
impl ReadStrategy for DbOnlyStrategy {
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        _cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send,
    {
        tracing::debug!(strategy = "db_only", key = cache_key, "Executing read");
        db_fn().await
    }

    fn name(&self) -> &'static str {
        "db_only"
    }
}

/// Cache-Only Strategy
/// Only reads from cache, returns error on cache miss
/// Useful for hot-path reads where stale data is acceptable
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheOnlyStrategy;

#[async_trait]
impl ReadStrategy for CacheOnlyStrategy {
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        cache_fn: CacheFn,
        _db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send,
    {
        tracing::debug!(strategy = "cache_only", key = cache_key, "Executing read");

        match cache_fn().await? {
            Some(value) => Ok(value),
            None => Err(PersistenceError::CacheMiss(cache_key.to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "cache_only"
    }
}

/// Read-Through Strategy
/// Similar to cache-first but with refresh-ahead behavior
#[derive(Debug, Clone)]
pub struct ReadThroughStrategy {
    /// Threshold (0.0-1.0) of TTL remaining to trigger background refresh
    pub refresh_threshold: f32,
}

impl Default for ReadThroughStrategy {
    fn default() -> Self {
        Self {
            refresh_threshold: 0.2, // Refresh when 20% TTL remaining
        }
    }
}

#[async_trait]
impl ReadStrategy for ReadThroughStrategy {
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send,
    {
        tracing::debug!(
            strategy = "read_through",
            key = cache_key,
            "Executing read"
        );

        // Try cache first
        if let Ok(Some(value)) = cache_fn().await {
            tracing::debug!(key = cache_key, "Cache hit (read-through)");
            // TODO: Check TTL and trigger background refresh if below threshold
            return Ok(value);
        }

        // Fall back to DB
        db_fn().await
    }

    fn name(&self) -> &'static str {
        "read_through"
    }
}

// =============================================================================
// STRATEGY SELECTOR
// =============================================================================

/// Dynamic strategy selection based on context
#[derive(Debug, Clone)]
pub enum DynamicStrategy {
    CacheFirst(CacheFirstStrategy),
    DbOnly(DbOnlyStrategy),
    CacheOnly(CacheOnlyStrategy),
    ReadThrough(ReadThroughStrategy),
}

impl Default for DynamicStrategy {
    fn default() -> Self {
        Self::CacheFirst(CacheFirstStrategy::new(true))
    }
}

#[async_trait]
impl ReadStrategy for DynamicStrategy {
    async fn read<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<T>
    where
        T: Send + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<Option<T>>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<T>> + Send,
    {
        match self {
            Self::CacheFirst(s) => s.read(cache_key, cache_fn, db_fn).await,
            Self::DbOnly(s) => s.read(cache_key, cache_fn, db_fn).await,
            Self::CacheOnly(s) => s.read(cache_key, cache_fn, db_fn).await,
            Self::ReadThrough(s) => s.read(cache_key, cache_fn, db_fn).await,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::CacheFirst(s) => s.name(),
            Self::DbOnly(s) => s.name(),
            Self::CacheOnly(s) => s.name(),
            Self::ReadThrough(s) => s.name(),
        }
    }
}

/// Arc-wrapped strategy for sharing across tasks
pub type SharedStrategy = Arc<dyn ReadStrategy>;

/// Create a shared strategy from a concrete implementation
pub fn shared<S: ReadStrategy + 'static>(strategy: S) -> SharedStrategy {
    Arc::new(strategy)
}
