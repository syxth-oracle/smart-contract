use anchor_lang::prelude::*;
use anchor_spl::token::{Token, Mint, TokenAccount, CloseAccount, close_account};
use crate::state::{PlatformConfig, Market, MarketStatus};
use crate::errors::PredictError;

#[derive(Accounts)]
#[instruction(market_id: u64)]
pub struct CloseMarket<'info> {
    #[account(
        mut,
        seeds = [b"market", market_id.to_le_bytes().as_ref()],
        bump = market.bump,
        close = admin,
    )]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        seeds = [b"yes_mint", market.key().as_ref()],
        bump,
    )]
    pub yes_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"no_mint", market.key().as_ref()],
        bump,
    )]
    pub no_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"vault", market.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn process_close_market(ctx: Context<CloseMarket>, _market_id: u64) -> Result<()> {
    let market = &ctx.accounts.market;

    // Safety check: market must be Resolved or Cancelled
    require!(
        market.status == MarketStatus::Resolved || market.status == MarketStatus::Cancelled,
        PredictError::MarketNotCloseable
    );

    // Safety check: vault must be empty (all payouts claimed)
    require!(
        ctx.accounts.vault.amount == 0,
        PredictError::OutstandingPositions
    );

    // Safety check: all share tokens must be burned (no outstanding positions)
    require!(
        ctx.accounts.yes_mint.supply == 0 && ctx.accounts.no_mint.supply == 0,
        PredictError::OutstandingPositions
    );

    let market_key = market.key();
    let market_id_bytes = market.market_id.to_le_bytes();

    // Market PDA is the authority for vault and mints
    let seeds = &[
        b"market" as &[u8],
        market_id_bytes.as_ref(),
        &[market.bump],
    ];
    let signer_seeds = &[&seeds[..]];

    // Close vault (token account)
    close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.admin.to_account_info(),
            authority: ctx.accounts.market.to_account_info(),
        },
        signer_seeds,
    ))?;

    // Close YES mint account
    close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.yes_mint.to_account_info(),
            destination: ctx.accounts.admin.to_account_info(),
            authority: ctx.accounts.market.to_account_info(),
        },
        signer_seeds,
    ))?;

    // Close NO mint account
    close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.no_mint.to_account_info(),
            destination: ctx.accounts.admin.to_account_info(),
            authority: ctx.accounts.market.to_account_info(),
        },
        signer_seeds,
    ))?;

    // Market account is closed by Anchor's `close = admin` constraint

    msg!("Market {} closed, rent reclaimed", market_key);
    Ok(())
}
