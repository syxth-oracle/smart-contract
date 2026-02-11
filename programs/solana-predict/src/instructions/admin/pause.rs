use anchor_lang::prelude::*;
use crate::state::{PlatformConfig, Market, MarketStatus};
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct PlatformAdmin<'info> {
    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct MarketAdmin<'info> {
    #[account(
        mut,
        has_one = creator,
        seeds = [b"market", market.market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    
    // Check creator or platform admin?
    // Design says "Admin-only". Usually platform admin.
    // But market has `creator`.
    // Let's allow Platform Admin OR Creator?
    // System Design: `pause_market(ctx: Context<AdminAction>)`.
    // Let's assume Platform Admin for consistency.
    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    
    pub admin: Signer<'info>,
    /// CHECK: Validated via equality to admin
    #[account(constraint = creator.key() == market.creator)]
    pub creator: AccountInfo<'info>, 
}

// Actually, let's simplify to PlatformAdmin for both as per Design "Admin Instructions".

pub fn pause_platform(ctx: Context<PlatformAdmin>) -> Result<()> {
    ctx.accounts.platform_config.paused = true;
    Ok(())
}

pub fn unpause_platform(ctx: Context<PlatformAdmin>) -> Result<()> {
    ctx.accounts.platform_config.paused = false;
    Ok(())
}

#[derive(Accounts)]
#[instruction(market_id: u64)]
pub struct ToggleMarketCtx<'info> {
    #[account(
        mut,
        seeds = [b"market", market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    pub admin: Signer<'info>,
}

pub fn pause_market(ctx: Context<ToggleMarketCtx>, _market_id: u64) -> Result<()> {
    // Save previous status? Market struct doesn't have "prev_status".
    // We just set to Paused.
    // If we Unpause, we need to know what to revert to.
    // Logic: calculated based on timestamps?
    // "Revert to previous status if timestamps still valid".
    // We'll calculate current expected status in `unpause`.
    ctx.accounts.market.status = MarketStatus::Paused;
    Ok(())
}

pub fn unpause_market(ctx: Context<ToggleMarketCtx>, _market_id: u64) -> Result<()> {
    // Re-evaluate status
    let market = &mut ctx.accounts.market;
    let clock = Clock::get()?;
    
    if market.resolved_outcome.is_some() {
        market.status = MarketStatus::Resolved;
    } else if clock.unix_timestamp >= market.end_timestamp {
        market.status = MarketStatus::Locked; // or Resolving?
    } else if clock.unix_timestamp >= market.lock_timestamp {
        market.status = MarketStatus::Locked;
    } else if clock.unix_timestamp >= market.start_timestamp {
        market.status = MarketStatus::Active;
    } else {
        market.status = MarketStatus::Pending;
    }
    
    Ok(())
}
