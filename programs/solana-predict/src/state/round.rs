use anchor_lang::prelude::*;

#[account]
pub struct RoundState {
    pub market: Pubkey,
    pub round_id: u64,
    pub status: RoundStatus,
    pub lock_price: Option<i64>,
    pub close_price: Option<i64>,
    pub total_yes: u64,
    pub total_no: u64,
    pub start_ts: i64,
    pub lock_ts: i64,
    pub end_ts: i64,
    pub oracle_round_id: Option<u64>,
    pub bump: u8,
}

impl RoundState {
    pub const LEN: usize = 8 + 32 + 8 + 1 + 9 + 9 + 8 * 3 + 8 * 3 + 9 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, InitSpace)]
pub enum RoundStatus {
    Open,
    Locked,
    Resolved,
    Cancelled,
}
