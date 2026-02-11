use anchor_lang::prelude::*;
use crate::state::market::Outcome;

#[account]
pub struct DisputeRecord {
    pub market: Pubkey,
    pub disputer: Pubkey,
    pub reason: String,           // max 256 chars
    pub bond_amount: u64,
    pub status: DisputeStatus,
    pub votes_for: u64,
    pub votes_against: u64,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub bump: u8,
}

impl DisputeRecord {
    pub const LEN: usize = 8 + 32 + 32 + (4 + 256) + 8 + 1 + 8 + 8 + 8 + 9 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace)]
pub enum DisputeStatus {
    Open,
    VotingActive,
    Upheld,
    Rejected,
}
