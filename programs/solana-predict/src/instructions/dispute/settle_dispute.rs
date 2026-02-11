use anchor_lang::prelude::*;
use crate::state::{PlatformConfig, Market, MarketStatus, DisputeRecord, DisputeStatus, Outcome};
use crate::events::DisputeSettled;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct SettleDispute<'info> {
    #[account(
        mut,
        seeds = [b"market", market.market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        seeds = [b"dispute", market.key().as_ref()],
        bump = dispute_record.bump,
        has_one = market,
    )]
    pub dispute_record: Account<'info, DisputeRecord>,

    /// Platform config — used to verify admin identity
    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,

    #[account(mut)]
    pub admin: Signer<'info>,
}

pub fn process_settle_dispute(
    ctx: Context<SettleDispute>,
    market_id: u64,
    result_outcome: Option<Outcome>, // None = Rejected (keep original), Some = Upheld (change to this)
) -> Result<()> {
    let market = &mut ctx.accounts.market;
    let dispute = &mut ctx.accounts.dispute_record;
    let clock = Clock::get()?;

    // Guards
    require!(market.status == MarketStatus::Disputed, PredictError::MarketNotActive);
    require!(dispute.status == DisputeStatus::Open || dispute.status == DisputeStatus::VotingActive, PredictError::AlreadyResolved);

    // Apply Result
    if let Some(new_outcome) = result_outcome {
        // Upheld
        market.resolved_outcome = Some(new_outcome.clone());
        market.status = MarketStatus::Resolved;
        dispute.status = DisputeStatus::Upheld;
        // Refund bond logic would go here if we held it in PDA or could move from treasury
    } else {
        // Rejected
        market.status = MarketStatus::Resolved; // Revert to resolved
        dispute.status = DisputeStatus::Rejected;
    }
    
    dispute.resolved_at = Some(clock.unix_timestamp);

    let upheld = dispute.status == DisputeStatus::Upheld;

    emit!(DisputeSettled {
        market_id,
        upheld,
        new_outcome: market.resolved_outcome.clone(),
    });

    Ok(())
}
