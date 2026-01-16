//! # Write Strategy Pattern
//!
//! Pluggable strategies for writing data to the persistence layer.
//! Implements write-through, write-behind, and write-around patterns.

use async_trait::async_trait;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;

use crate::error::Result;

/// Strategy for writing data to persistence layer
#[async_trait]
pub trait WriteStrategy: Send + Sync + Debug {
    /// Execute a write operation with cache and DB functions
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        value: &T,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send;

    /// Strategy name for metrics/logging
    fn name(&self) -> &'static str;
}

// =============================================================================
// STRATEGY IMPLEMENTATIONS
// =============================================================================

/// Write-Through Strategy
/// 1. Write to DB first (source of truth)
/// 2. On success: update cache
/// 3. On DB failure: do not update cache
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteThroughStrategy;

#[async_trait]
impl WriteStrategy for WriteThroughStrategy {
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        _value: &T,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send,
    {
        tracing::debug!(
            strategy = "write_through",
            key = cache_key,
            "Executing write"
        );

        // Write to DB first (source of truth)
        db_fn().await?;
        tracing::debug!(key = cache_key, "DB write successful");

        // Then update cache (best effort)
        if let Err(e) = cache_fn().await {
            tracing::warn!(
                key = cache_key,
                error = %e,
                "Cache write failed (DB write succeeded)"
            );
            // Don't fail the operation - DB is authoritative
        } else {
            tracing::debug!(key = cache_key, "Cache write successful");
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "write_through"
    }
}

/// Write-Around Strategy
/// Only writes to DB, invalidates cache
/// Good for write-heavy workloads where data isn't immediately re-read
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteAroundStrategy;

#[async_trait]
impl WriteStrategy for WriteAroundStrategy {
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        _value: &T,
        _cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send,
    {
        tracing::debug!(
            strategy = "write_around",
            key = cache_key,
            "Executing write"
        );

        // Only write to DB
        db_fn().await?;

        // Cache invalidation would be handled separately
        // (not updating, just invalidating)
        tracing::debug!(
            key = cache_key,
            "DB write successful (cache not updated)"
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "write_around"
    }
}

/// Write-Back (Write-Behind) Strategy
/// 1. Write to cache immediately
/// 2. Queue DB write for async processing
/// WARNING: Risk of data loss if cache fails before DB write
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteBackStrategy;

#[async_trait]
impl WriteStrategy for WriteBackStrategy {
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        _value: &T,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send,
    {
        tracing::debug!(
            strategy = "write_back",
            key = cache_key,
            "Executing write"
        );

        // Write to cache first (fast path)
        cache_fn().await?;
        tracing::debug!(key = cache_key, "Cache write successful");

        // In a real implementation, this would queue for async write
        // For now, we do synchronous write but could spawn a task
        tokio::spawn(async move {
            if let Err(e) = db_fn().await {
                tracing::error!(
                    error = %e,
                    "Background DB write failed - DATA LOSS RISK"
                );
            }
        });

        Ok(())
    }

    fn name(&self) -> &'static str {
        "write_back"
    }
}

/// DB-Only Write Strategy
/// Bypasses cache entirely, writes directly to DB
#[derive(Debug, Clone, Copy, Default)]
pub struct DbOnlyWriteStrategy;

#[async_trait]
impl WriteStrategy for DbOnlyWriteStrategy {
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        _value: &T,
        _cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send,
    {
        tracing::debug!(strategy = "db_only", key = cache_key, "Executing write");
        db_fn().await
    }

    fn name(&self) -> &'static str {
        "db_only_write"
    }
}

// =============================================================================
// DYNAMIC STRATEGY
// =============================================================================

/// Dynamic strategy selection based on context
#[derive(Debug, Clone)]
pub enum DynamicWriteStrategy {
    WriteThrough(WriteThroughStrategy),
    WriteAround(WriteAroundStrategy),
    WriteBack(WriteBackStrategy),
    DbOnly(DbOnlyWriteStrategy),
}

impl Default for DynamicWriteStrategy {
    fn default() -> Self {
        Self::WriteThrough(WriteThroughStrategy)
    }
}

#[async_trait]
impl WriteStrategy for DynamicWriteStrategy {
    async fn write<T, CacheFn, CacheFut, DbFn, DbFut>(
        &self,
        cache_key: &str,
        value: &T,
        cache_fn: CacheFn,
        db_fn: DbFn,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
        CacheFn: FnOnce() -> CacheFut + Send,
        CacheFut: Future<Output = Result<()>> + Send,
        DbFn: FnOnce() -> DbFut + Send,
        DbFut: Future<Output = Result<()>> + Send,
    {
        match self {
            Self::WriteThrough(s) => s.write(cache_key, value, cache_fn, db_fn).await,
            Self::WriteAround(s) => s.write(cache_key, value, cache_fn, db_fn).await,
            Self::WriteBack(s) => s.write(cache_key, value, cache_fn, db_fn).await,
            Self::DbOnly(s) => s.write(cache_key, value, cache_fn, db_fn).await,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::WriteThrough(s) => s.name(),
            Self::WriteAround(s) => s.name(),
            Self::WriteBack(s) => s.name(),
            Self::DbOnly(s) => s.name(),
        }
    }
}

/// Arc-wrapped write strategy for sharing across tasks
pub type SharedWriteStrategy = Arc<dyn WriteStrategy>;

/// Create a shared write strategy from a concrete implementation
pub fn shared<S: WriteStrategy + 'static>(strategy: S) -> SharedWriteStrategy {
    Arc::new(strategy)
}
