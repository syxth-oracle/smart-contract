use anchor_lang::prelude::*;

pub mod state;
pub mod instructions;
pub mod errors;
pub mod events;
pub mod utils;

use instructions::*;
use state::market::Outcome;

declare_id!("F4JxF7aePgrKKwmVM9tXHUadeTKNLXwFMZFQoiBowLcr");

#[program]
pub mod solana_predict {
    use super::*;

    pub fn init_platform(ctx: Context<InitPlatform>, fee_bps: u16, dispute_bond: u64) -> Result<()> {
        instructions::admin::init_platform::process_init_platform(ctx, fee_bps, dispute_bond)
    }

    pub fn create_market(ctx: Context<CreateMarket>, market_id: u64, params: CreateMarketParams) -> Result<()> {
        instructions::admin::create_market::process_create_market(ctx, market_id, params)
    }

    pub fn place_bet(ctx: Context<PlaceBet>, market_id: u64, outcome: Outcome, amount: u64, min_shares: u64) -> Result<()> {
        instructions::betting::place_bet::process_place_bet(ctx, market_id, outcome, amount, min_shares)
    }

    pub fn cancel_bet(ctx: Context<CancelBet>, market_id: u64, shares_to_burn: u64) -> Result<()> {
        instructions::betting::cancel_bet::process_cancel_bet(ctx, market_id, shares_to_burn)
    }

    pub fn claim_payout(ctx: Context<ClaimPayout>, market_id: u64) -> Result<()> {
        instructions::betting::claim_payout::process_claim_payout(ctx, market_id)
    }

    pub fn resolve_market(ctx: Context<ResolveMarket>, market_id: u64, outcome: Outcome) -> Result<()> {
        instructions::oracle::resolve_market::process_resolve_market(ctx, market_id, outcome)
    }

    pub fn open_dispute(ctx: Context<OpenDispute>, market_id: u64, reason: String) -> Result<()> {
        instructions::dispute::open_dispute::process_open_dispute(ctx, market_id, reason)
    }

    pub fn settle_dispute(ctx: Context<SettleDispute>, market_id: u64, result_outcome: Option<Outcome>) -> Result<()> {
        instructions::dispute::settle_dispute::process_settle_dispute(ctx, market_id, result_outcome)
    }

    pub fn pause_platform(ctx: Context<PlatformAdmin>) -> Result<()> {
        instructions::admin::pause::pause_platform(ctx)
    }

    pub fn unpause_platform(ctx: Context<PlatformAdmin>) -> Result<()> {
        instructions::admin::pause::unpause_platform(ctx)
    }

    pub fn pause_market(ctx: Context<ToggleMarketCtx>, market_id: u64) -> Result<()> {
        instructions::admin::pause::pause_market(ctx, market_id)
    }

    pub fn unpause_market(ctx: Context<ToggleMarketCtx>, market_id: u64) -> Result<()> {
        instructions::admin::pause::unpause_market(ctx, market_id)
    }

    pub fn update_fees(ctx: Context<UpdateFees>, new_fee_bps: u16) -> Result<()> {
        instructions::admin::update_fees::update_fees(ctx, new_fee_bps)
    }

    pub fn close_market(ctx: Context<CloseMarket>, market_id: u64) -> Result<()> {
        instructions::admin::close_market::process_close_market(ctx, market_id)
    }

    pub fn update_collateral_mint(ctx: Context<UpdateCollateralMint>) -> Result<()> {
        instructions::admin::update_collateral_mint::update_collateral_mint(ctx)
    }
}
