use anchor_lang::prelude::*;
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
    /// CHECK: New treasury address (should be a wSOL ATA)
    pub new_treasury: AccountInfo<'info>,
}

pub fn update_treasury(ctx: Context<UpdateTreasury>) -> Result<()> {
    ctx.accounts.platform_config.treasury = ctx.accounts.new_treasury.key();
    Ok(())
}
