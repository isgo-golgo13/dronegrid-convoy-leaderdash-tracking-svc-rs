//! # Strategy Module
//!
//! Pluggable read and write strategies for flexible persistence patterns.

pub mod read_strategy;
pub mod write_strategy;

pub use read_strategy::{
    CacheFirstStrategy, CacheOnlyStrategy, DbOnlyStrategy, DynamicStrategy, ReadStrategy,
    ReadThroughStrategy, SharedStrategy,
};

pub use write_strategy::{
    DbOnlyWriteStrategy, DynamicWriteStrategy, SharedWriteStrategy, WriteAroundStrategy,
    WriteBackStrategy, WriteStrategy, WriteThroughStrategy,
};
