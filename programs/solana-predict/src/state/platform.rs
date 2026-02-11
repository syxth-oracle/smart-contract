use anchor_lang::prelude::*;

#[account]
pub struct PlatformConfig {
    pub admin: Pubkey,              // 32
    pub fee_bps: u16,               // 2
    pub treasury: Pubkey,           // 32
    pub paused: bool,               // 1
    pub total_markets: u64,         // 8
    pub collateral_mint: Pubkey,    // 32 (wSOL or other SPL mint)
    pub dispute_bond_lamports: u64, // 8
    pub bump: u8,                   // 1
}

impl PlatformConfig {
    pub const LEN: usize = 8 + 32 + 2 + 32 + 1 + 8 + 32 + 8 + 1;
}
