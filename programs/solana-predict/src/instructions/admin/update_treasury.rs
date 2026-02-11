use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use crate::state::PlatformConfig;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct UpdateTreasury<'info> {
    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    pub admin: Signer<'info>,
    #[account(
        constraint = new_treasury.mint == platform_config.collateral_mint @ PredictError::InvalidMint,
    )]
    pub new_treasury: Account<'info, TokenAccount>,
}

pub fn update_treasury(ctx: Context<UpdateTreasury>) -> Result<()> {
    ctx.accounts.platform_config.treasury = ctx.accounts.new_treasury.key();
    Ok(())
}
