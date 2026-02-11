use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;
use crate::state::{PlatformConfig, Market, MarketStatus, OracleSource, Outcome};
use crate::events::MarketResolved;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct ResolveMarket<'info> {
    #[account(
        mut,
        seeds = [b"market", market.market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,

    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,

    /// The Pyth price feed account (optional - only needed for Pyth oracle markets)
    /// CHECK: We validate this is the correct feed in the instruction logic
    pub pyth_price_feed: Option<Account<'info, PriceUpdateV2>>,
}

pub fn process_resolve_market(
    ctx: Context<ResolveMarket>,
    market_id: u64,
    outcome: Outcome,
) -> Result<()> {
    let market = &mut ctx.accounts.market;
    let clock = Clock::get()?;

    // Guards
    require!(market.status == MarketStatus::Active || market.status == MarketStatus::Locked, PredictError::AlreadyResolved);
    
    // Check timestamp unless ManualAdmin (Early Resolution allowed)
    if market.oracle_source != OracleSource::ManualAdmin {
        require!(clock.unix_timestamp >= market.end_timestamp, PredictError::RoundIncomplete);
    }

    // Final outcome to be set
    let final_outcome: Outcome;
    let resolution_price: Option<i64>;

    // Oracle Logic
    match market.oracle_source {
        OracleSource::ManualAdmin => {
            // Admin (signer) provides outcome directly
            // Verified admin via constraint on platform_config
            require!(outcome == Outcome::Yes || outcome == Outcome::No || outcome == Outcome::Invalid, PredictError::InvalidOutcome);
            final_outcome = outcome;
            resolution_price = None;
        },
        OracleSource::Pyth => {
            // Require Pyth price feed account
            let price_feed = ctx.accounts.pyth_price_feed.as_ref()
                .ok_or(PredictError::OracleMismatch)?;
            
            // SC-3 FIX: Validate that the Pyth feed account matches the market's stored oracle_feed
            require!(
                price_feed.key() == market.oracle_feed,
                PredictError::InvalidPythFeed
            );
            
            // Get the latest price from PriceUpdateV2
            let price_data = &price_feed.price_message;
            
            // H-1 FIX: Check oracle staleness (reject prices older than 60 seconds)
            let price_timestamp = price_data.publish_time;
            require!(
                clock.unix_timestamp - price_timestamp <= 60,
                PredictError::OracleStale
            );
            
            // Price is stored with an exponent (e.g., price * 10^expo)
            // Normalize to a comparable integer (we'll use the raw price)
            let current_price = price_data.price;
            
            // Compare against threshold
            // If current_price > oracle_threshold, resolve as YES
            // If current_price <= oracle_threshold, resolve as NO
            if current_price > market.oracle_threshold {
                final_outcome = Outcome::Yes;
            } else {
                final_outcome = Outcome::No;
            }
            
            resolution_price = Some(current_price);
            
            msg!("Pyth price: {}, threshold: {}, outcome: {:?}", 
                current_price, market.oracle_threshold, final_outcome);
        },
        OracleSource::Switchboard => {
            // TODO: Implement Switchboard if needed
            return err!(PredictError::OracleMismatch);
        },
    }

    // Update State
    market.resolved_outcome = Some(final_outcome.clone());
    market.resolution_price = resolution_price;
    market.resolved_at = Some(clock.unix_timestamp);
    market.status = MarketStatus::Resolved;
    
    emit!(MarketResolved {
        market_id,
        outcome: final_outcome,
        resolution_price: resolution_price.unwrap_or(0),
        total_collateral: market.total_collateral,
    });

    Ok(())
}
