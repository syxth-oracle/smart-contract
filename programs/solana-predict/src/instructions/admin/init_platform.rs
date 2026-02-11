use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, TokenAccount};
use crate::state::PlatformConfig;
use crate::events::PlatformInitialized;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct InitPlatform<'info> {
    #[account(
        init,
        seeds = [b"platform_config"],
        bump,
        payer = admin,
        space = PlatformConfig::LEN
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    
    #[account(mut)]
    pub admin: Signer<'info>,
    
    pub system_program: Program<'info, System>,
    pub collateral_mint: Account<'info, Mint>,
    #[account(
        constraint = treasury.mint == collateral_mint.key() @ PredictError::InvalidMint,
    )]
    pub treasury: Account<'info, TokenAccount>,
}

pub fn process_init_platform(
    ctx: Context<InitPlatform>, 
    fee_bps: u16, 
    dispute_bond: u64
) -> Result<()> {
    require!(fee_bps <= 1000, PredictError::FeeExceedsMax); // Max 10%
    
    let platform = &mut ctx.accounts.platform_config;
    platform.admin = ctx.accounts.admin.key();
    platform.fee_bps = fee_bps;
    platform.treasury = ctx.accounts.treasury.key();
    platform.paused = false;
    platform.total_markets = 0;
    platform.collateral_mint = ctx.accounts.collateral_mint.key();
    platform.dispute_bond_lamports = dispute_bond;
    platform.bump = ctx.bumps.platform_config;

    emit!(PlatformInitialized {
        admin: platform.admin,
        fee_bps: platform.fee_bps,
    });

    Ok(())
}
