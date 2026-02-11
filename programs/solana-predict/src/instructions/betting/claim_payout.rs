use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Mint, TokenAccount, Burn, Transfer};
use crate::state::{Market, MarketStatus, UserPosition, Outcome};
use crate::events::PayoutClaimed;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct ClaimPayout<'info> {
    #[account(
        mut,
        seeds = [b"market", market.market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,

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

    #[account(mut)]
    pub user: Signer<'info>,

    pub collateral_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn process_claim_payout(
    ctx: Context<ClaimPayout>,
    market_id: u64,
) -> Result<()> {
    let market = &mut ctx.accounts.market;
    
    // Guards
    require!(market.status == MarketStatus::Resolved, PredictError::MarketNotResolved);
    let outcome = market.resolved_outcome.clone().ok_or(PredictError::MarketNotResolved)?;
    require!(ctx.accounts.user_position.total_claimed == 0, PredictError::AlreadyClaimed);

    // Read user balance
    let user_share_acc = TokenAccount::try_deserialize(&mut &ctx.accounts.user_share_account.data.borrow()[..])?;
    
    // For Invalid outcome, user can claim with either YES or NO shares (pro-rata across total supply)
    // For Yes/No outcomes, user must hold the winning mint
    if outcome == Outcome::Invalid {
        // Accept either YES or NO mint for Invalid outcome
        require!(
            user_share_acc.mint == market.yes_mint || user_share_acc.mint == market.no_mint,
            PredictError::InvalidOutcome
        );
    } else {
        let winning_mint = match outcome {
            Outcome::Yes => market.yes_mint,
            Outcome::No => market.no_mint,
            _ => unreachable!(),
        };
        require!(user_share_acc.mint == winning_mint, PredictError::InvalidOutcome);
    }
    
    let shares = user_share_acc.amount;
    require!(shares > 0, PredictError::NoPosition);

    // Calculate Payout using mint supply (total outstanding winning tokens)
    // In CPMM, market.total_yes/no_shares are pool reserves, NOT total supply.
    // We use the mint's supply to get the actual total outstanding tokens.
    let payout = if outcome == Outcome::Invalid {
        let total_supply = ctx.accounts.yes_mint.supply + ctx.accounts.no_mint.supply;
        if total_supply == 0 { 0 } else {
            (shares as u128 * market.total_collateral as u128 / total_supply as u128) as u64
        }
    } else {
        let winning_supply = match outcome {
            Outcome::Yes => ctx.accounts.yes_mint.supply,
            Outcome::No => ctx.accounts.no_mint.supply,
            _ => 0,
        };
        if winning_supply == 0 { 0 } else {
            (shares as u128 * market.total_collateral as u128 / winning_supply as u128) as u64
        }
    };

    // Cap payout to vault balance to prevent last-claimer underflow from rounding
    let payout = payout.min(ctx.accounts.vault.amount);
    require!(payout > 0, PredictError::NoPosition);

    // Burn Winning Shares
    // Wait, if I burn shares, I manipulate `total_winning_shares` for the NEXT claimer?
    // NO. `market.total_winning_shares` MUST remain constant during payout phase, 
    // OR we use the snapshot at resolution.
    // `market` struct has `total_yes_shares`. If we decrement it here, early claimers get correct amount,
    // but late claimers get (Shares / ReducedTotal) * ReducedCollateral?
    // Math:
    // User A: 10 shares. Total: 100. Collateral: 1000.
    // Claim: (10/100)*1000 = 100. Remainder: 900. Total Shares: 90.
    // User B: 10 shares. Total: 90. Collateral: 900.
    // Claim: (10/90)*900 = 100. 
    // It works out proportionally IF we burn and transfer.
    
    // Burn shares from the correct mint — for Invalid outcome, determine mint from user's share account
    let burn_mint = if user_share_acc.mint == market.yes_mint {
        ctx.accounts.yes_mint.to_account_info()
    } else {
        ctx.accounts.no_mint.to_account_info()
    };

    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: burn_mint,
                from: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares,
    )?;

    // Transfer Payout
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
        payout,
    )?;

    // Update State
    market.total_collateral = market.total_collateral.checked_sub(payout).ok_or(PredictError::InsufficientVault)?;
    // Note: pool reserves (total_yes/no_shares) are NOT decremented during payout.
    // In CPMM, these track AMM pool reserves, not token supply.
    // The burn above reduces mint supply, which is used as the payout denominator.
    
    ctx.accounts.user_position.total_claimed = ctx.accounts.user_position.total_claimed
        .checked_add(payout)
        .ok_or(PredictError::MathOverflow)?;

    emit!(PayoutClaimed {
        market_id,
        user: ctx.accounts.user.key(),
        amount: payout,
        shares_burned: shares,
    });

    Ok(())
}
