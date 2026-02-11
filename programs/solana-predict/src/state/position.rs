use anchor_lang::prelude::*;

#[account]
pub struct UserPosition {
    pub user: Pubkey,
    pub market: Pubkey,
    pub yes_shares: u64,
    pub no_shares: u64,
    pub total_deposited: u64,
    pub total_claimed: u64,
    pub last_bet_timestamp: i64,
    pub bump: u8,
}

impl UserPosition {
    pub const LEN: usize = 8 + 32 + 32 + 8 * 4 + 8 + 1;
}
