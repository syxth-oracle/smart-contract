use anchor_lang::prelude::*;
use crate::state::market::{Outcome, OracleSource};

#[event]
pub struct PlatformInitialized {
    pub admin: Pubkey,
    pub fee_bps: u16,
}

#[event]
pub struct MarketCreated {
    pub market_id: u64,
    pub creator: Pubkey,
    pub title: String,
    pub oracle_source: OracleSource,
    pub end_timestamp: i64,
}

#[event]
pub struct BetPlaced {
    pub market_id: u64,
    pub user: Pubkey,
    pub outcome: Outcome,
    pub amount: u64,
    pub shares: u64,
    pub new_yes_total: u64,
    pub new_no_total: u64,
    pub timestamp: i64,
}

#[event]
pub struct BetCancelled {
    pub market_id: u64,
    pub user: Pubkey,
    pub shares_burned: u64,
    pub refund_amount: u64,
}

#[event]
pub struct RoundLocked {
    pub market_id: u64,
    pub round_id: u64,
    pub lock_price: i64,
}

#[event]
pub struct MarketResolved {
    pub market_id: u64,
    pub outcome: Outcome,
    pub resolution_price: i64,
    pub total_collateral: u64,
}

#[event]
pub struct PayoutClaimed {
    pub market_id: u64,
    pub user: Pubkey,
    pub amount: u64,
    pub shares_burned: u64,
}

#[event]
pub struct DisputeOpened {
    pub market_id: u64,
    pub disputer: Pubkey,
    pub bond: u64,
}

#[event]
pub struct DisputeSettled {
    pub market_id: u64,
    pub upheld: bool,
    pub new_outcome: Option<Outcome>,
}

#[event]
pub struct RoundStarted {
    pub market_id: u64,
    pub round_id: u64,
    pub start_ts: i64,
    pub lock_ts: i64,
    pub end_ts: i64,
}
