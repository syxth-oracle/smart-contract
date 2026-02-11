use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Mint, TokenAccount, Burn, Transfer};
use crate::state::{PlatformConfig, Market, MarketStatus, UserPosition, Outcome};
use crate::events::BetCancelled;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct CancelBet<'info> {
    #[account(
        mut,
        seeds = [b"market", market.market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Box<Account<'info, Market>>,

    #[account(
        mut,
        seeds = [b"yes_mint", market.key().as_ref()],
        bump,
        constraint = yes_mint.key() == market.yes_mint @ PredictError::InvalidMint
    )]
    pub yes_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"no_mint", market.key().as_ref()],
        bump,
        constraint = no_mint.key() == market.no_mint @ PredictError::InvalidMint
    )]
    pub no_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"vault", market.key().as_ref()],
        bump,
        token::mint = collateral_mint
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"position", market.key().as_ref(), user.key().as_ref()],
        bump = user_position.bump,
    )]
    pub user_position: Account<'info, UserPosition>,

    #[account(
        mut,
        associated_token::mint = market.collateral_mint,
        associated_token::authority = user,
    )]
    pub user_ata: Account<'info, TokenAccount>,

    /// CHECK: Validated to match outcome mint
    #[account(mut)]
    pub user_share_account: AccountInfo<'info>,
    
    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
    )]
    pub platform_config: Account<'info, PlatformConfig>,

    #[account(
        mut,
        constraint = treasury.key() == platform_config.treasury,
        constraint = treasury.mint == collateral_mint.key() @ PredictError::InvalidMint,
    )]
    pub treasury: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub collateral_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn process_cancel_bet(
    ctx: Context<CancelBet>,
    market_id: u64,
    shares_to_burn: u64,
) -> Result<()> {
    let market = &mut ctx.accounts.market;
    let clock = Clock::get()?;

    // Guards
    require!(market.status == MarketStatus::Active, PredictError::MarketNotActive);
    require!(clock.unix_timestamp < market.lock_timestamp, PredictError::BettingClosed);

    // Identify which outcome user holds (simplification: assume user signals intent via share account or we check balance)
    // Actually, checking user_position is better
    // But `cancel_bet` usually requires specifying WHICH side if user holds both (hedging).
    // The instruction args in design only say `shares_to_burn`.
    // We infer side from `user_share_account` mint?
    
    // Let's verify `user_share_account` mint matches `yes_mint` or `no_mint`.
    // We can load the account data to check mint.
    let user_share_acc = TokenAccount::try_deserialize(&mut &ctx.accounts.user_share_account.data.borrow()[..])?;
    let outcome = if user_share_acc.mint == market.yes_mint {
        Outcome::Yes
    } else if user_share_acc.mint == market.no_mint {
        Outcome::No
    } else {
        return err!(PredictError::InvalidOutcome);
    };

    // CPMM sell: reverse of buy
    // Selling YES: add shares back to yes_pool, remove collateral from no_pool
    // Selling NO:  add shares back to no_pool, remove collateral from yes_pool
    let yes_pool = market.total_yes_shares as u128;
    let no_pool = market.total_no_shares as u128;
    let k = yes_pool.checked_mul(no_pool).ok_or(PredictError::MathOverflow)?;
    let burn_amount = shares_to_burn as u128;

    let (raw_refund, new_yes, new_no) = if outcome == Outcome::Yes {
        let new_yes_pool = yes_pool.checked_add(burn_amount).ok_or(PredictError::MathOverflow)?;
        let new_no_pool = k.checked_div(new_yes_pool).ok_or(PredictError::MathOverflow)?;
        let refund = (no_pool.checked_sub(new_no_pool).ok_or(PredictError::MathOverflow)?) as u64;
        (refund, new_yes_pool as u64, new_no_pool as u64)
    } else {
        let new_no_pool = no_pool.checked_add(burn_amount).ok_or(PredictError::MathOverflow)?;
        let new_yes_pool = k.checked_div(new_no_pool).ok_or(PredictError::MathOverflow)?;
        let refund = (yes_pool.checked_sub(new_yes_pool).ok_or(PredictError::MathOverflow)?) as u64;
        (refund, new_yes_pool as u64, new_no_pool as u64)
    };

    require!(raw_refund > 0, PredictError::MathOverflow);

    // Exit fee: use market.fee_bps (round up to prevent micro-transaction fee bypass)
    let fee = ((raw_refund as u128 * market.fee_bps as u128 + 9999) / 10000) as u64;
    let refund = raw_refund.checked_sub(fee).ok_or(PredictError::MathOverflow)?;

    // Burn Shares
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: if outcome == Outcome::Yes { ctx.accounts.yes_mint.to_account_info() } else { ctx.accounts.no_mint.to_account_info() },
                from: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares_to_burn,
    )?;

    // Transfer Refund
    let market_id_bytes = market.market_id.to_le_bytes();
    let seeds = &[
        b"market",
        market_id_bytes.as_ref(),
        &[market.bump],
    ];
    let signer = &[&seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.user_ata.to_account_info(),
                authority: market.to_account_info(),
            },
            signer,
        ),
        refund,
    )?;

    // Fee logic?
    // If fee > 0, does the Vault keep it or we send to treasury?
    // Design says "Transfer USDC from vault -> user". It implies vault keeps fee (collateral surplus).
    // Or we send fee to treasury.
    if fee > 0 {
         token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                    authority: market.to_account_info(),
                },
                signer,
            ),
            fee,
        )?;
    }

    // Update State (CPMM pool reserves)
    market.total_collateral = market.total_collateral.checked_sub(raw_refund).ok_or(PredictError::InsufficientVault)?;
    market.total_yes_shares = new_yes;
    market.total_no_shares = new_no;

    if outcome == Outcome::Yes {
        ctx.accounts.user_position.yes_shares = ctx.accounts.user_position.yes_shares.checked_sub(shares_to_burn).ok_or(PredictError::InsufficientShares)?;
    } else {
        ctx.accounts.user_position.no_shares = ctx.accounts.user_position.no_shares.checked_sub(shares_to_burn).ok_or(PredictError::InsufficientShares)?;
    }
    
    // We should also decrement `total_deposited` in position if we track net?
    // Or maybe not. Let's leave it as cumulative deposited?
    // Actually `total_deposited` usually means net principal exposed.
    // Let's decrement it by `refund` (principal returned).
    ctx.accounts.user_position.total_deposited = ctx.accounts.user_position.total_deposited.saturating_sub(refund); 

    emit!(BetCancelled {
        market_id,
        user: ctx.accounts.user.key(),
        shares_burned: shares_to_burn,
        refund_amount: refund,
    });

    Ok(())
}
