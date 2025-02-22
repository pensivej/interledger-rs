//! # interledger-store-redis
//!
//! A Store that uses [Redis](https://redis.io/) as the database for storing account details, balances, the routing table, etc.

mod account;
mod crypto;
mod store;

pub use account::Account;
pub use redis::{ConnectionInfo, IntoConnectionInfo};
pub use store::{RedisStore, RedisStoreBuilder};
