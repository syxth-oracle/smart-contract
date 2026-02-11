use anchor_lang::prelude::*;

#[account]
pub struct Market {
    pub market_id: u64,
    pub creator: Pubkey,
    pub title: String,              // max 128 chars
    pub description: String,        // max 512 chars
    pub category: MarketCategory,
    pub status: MarketStatus,
    pub collateral_mint: Pubkey,    // wSOL / SPL collateral mint
    pub yes_mint: Pubkey,
    pub no_mint: Pubkey,
    pub vault: Pubkey,
    pub total_yes_shares: u64,
    pub total_no_shares: u64,
    pub total_collateral: u64,
    pub oracle_source: OracleSource,
    pub oracle_feed: Pubkey,
    pub oracle_threshold: i64,      // price threshold for binary resolution
    pub start_timestamp: i64,
    pub lock_timestamp: i64,        // no more bets after this
    pub end_timestamp: i64,         // resolution time
    pub resolved_outcome: Option<Outcome>,
    pub resolution_price: Option<i64>,
    pub resolved_at: Option<i64>,    // timestamp when market was resolved
    pub min_bet: u64,               // minimum collateral per bet
    pub max_bet: u64,               // maximum collateral per bet (0 = unlimited)
    pub fee_bps: u16,
    pub is_recurring: bool,
    pub round_duration: Option<i64>,
    pub current_round: u64,
    pub bump: u8,
}

impl Market {
    // 8 (discriminator)
    // 8 (market_id) + 32 (creator)
    // 4 + 128 (title) + 4 + 512 (description)
    // 1 (category) + 1 (status)
    // 32 (collateral_mint) + 32 (yes_mint) + 32 (no_mint) + 32 (vault)
    // 8 (total_yes) + 8 (total_no) + 8 (total_collateral)
    // 1 (oracle_source) + 32 (oracle_feed) + 8 (oracle_threshold)
    // 8 (start) + 8 (lock) + 8 (end)
    // 1+1 (resolved_outcome option) + 1+8 (resolution_price option)
    // 8 (min_bet) + 8 (max_bet) + 2 (fee_bps)
    // 1+8 (resolved_at option)
    // 1 (is_recurring) + 1+8 (round_duration option) + 8 (current_round)
    // 1 (bump)
    pub const LEN: usize = 8 + 8 + 32 + (4 + 128) + (4 + 512) + 1 + 1 + 32 * 4 + 8 * 3 + 1 + 32 + 8 + 8 * 3 + 2 + 9 + 9 + 8 * 2 + 2 + 1 + 9 + 8 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace)]
pub enum MarketStatus {
    Pending,
    Active,
    Locked,
    Resolving,
    Resolved,
    Disputed,
    Cancelled,
    Paused,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace)]
pub enum MarketCategory {
    Crypto,
    Sports,
    Politics,
    Entertainment,
    Weather,
    Custom,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace, Debug)]
pub enum Outcome {
    Yes,
    No,
    Invalid,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace)]
pub enum OracleSource {
    Pyth,
    Switchboard,
    ManualAdmin,
}
