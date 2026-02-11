use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Mint, TokenAccount, MintTo, Transfer};
use crate::state::{PlatformConfig, Market, MarketStatus, UserPosition, Outcome};
use crate::events::BetPlaced;
use crate::errors::PredictError;

#[derive(Accounts)]
#[instruction(market_id: u64)]
pub struct PlaceBet<'info> {
    #[account(
        mut,
        seeds = [b"market", market_id.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Box<Account<'info, Market>>,

    #[account(
        mut,
        seeds = [b"yes_mint", market.key().as_ref()],
        bump
    )]
    pub yes_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"no_mint", market.key().as_ref()],
        bump
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
        init_if_needed,
        seeds = [b"position", market.key().as_ref(), user.key().as_ref()],
        bump,
        payer = user,
        space = UserPosition::LEN
    )]
    pub user_position: Account<'info, UserPosition>,

    #[account(
        mut,
        associated_token::mint = market.collateral_mint,
        associated_token::authority = user,
    )]
    pub user_ata: Account<'info, TokenAccount>,

    // Note: We need user's share ATAs to mint shares to.
    // Assuming client creates them or we init_if_needed.
    // For simplicity, we assume they exist or let anchor/client handle.
    // To allow `init_if_needed`, we need associated_token program constraints.
    // For now, let's assume passed in accounts use `init_if_needed` or are just standard token accounts.
    // But `MintTo` requires writable destination.
    // We'll trust the user passes their own correct ATAs.
    // Ideally we use `associated_token::mint = ...` but we need dynamic mint selection (YES vs NO).
    // So we pass one generic `user_share_account` and verify it matches the outcome mint?
    // Or we pass both and select one?
    // Let's expect the client to pass the correct account for the outcome.
    
    /// CHECK: Validated in handler to match outcome mint
    #[account(mut)]
    pub user_share_account: AccountInfo<'info>,

    #[account(
        mut,
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
    pub rent: Sysvar<'info, Rent>,
}

pub fn process_place_bet(
    ctx: Context<PlaceBet>,
    market_id: u64,
    outcome: Outcome,
    amount: u64,
    min_shares_out: u64,
) -> Result<()> {
    let market = &mut ctx.accounts.market;
    let platform = &ctx.accounts.platform_config;
    let clock = Clock::get()?;

    // 1. Guard Checks
    require!(!platform.paused, PredictError::PlatformPaused);
    require!(market.status == MarketStatus::Active, PredictError::MarketNotActive);
    require!(clock.unix_timestamp < market.lock_timestamp, PredictError::BettingClosed);
    require!(amount >= market.min_bet, PredictError::BelowMinBet);
    if market.max_bet > 0 {
        require!(amount <= market.max_bet, PredictError::AboveMaxBet);
    }
    require!(outcome == Outcome::Yes || outcome == Outcome::No, PredictError::InvalidOutcome);

    // Validate user share account before any transfers
    let user_share_data = TokenAccount::try_deserialize(&mut &ctx.accounts.user_share_account.data.borrow()[..])?;
    let target_mint = if outcome == Outcome::Yes { market.yes_mint } else { market.no_mint };
    require!(user_share_data.mint == target_mint, PredictError::InvalidMint);
    require!(user_share_data.owner == ctx.accounts.user.key(), PredictError::Unauthorized);

    // 2. Fee Calculation (round up to prevent micro-bet fee bypass)
    let fee = ((amount as u128 * market.fee_bps as u128 + 9999) / 10000) as u64;
    let net_amount = amount.checked_sub(fee).ok_or(PredictError::MathOverflow)?;
    require!(net_amount > 0, PredictError::BelowMinBet);

    // 3. Transfer USDC
    // User -> Vault (net)
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_ata.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        net_amount,
    )?;

    // User -> Treasury (fee)
    if fee > 0 {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_ata.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            fee,
        )?;
    }

    // 4. Calculate Shares via CPMM
    let yes_pool = market.total_yes_shares as u128;
    let no_pool = market.total_no_shares as u128;
    let k = yes_pool.checked_mul(no_pool).ok_or(PredictError::MathOverflow)?;
    let net = net_amount as u128;

    let shares = if outcome == Outcome::Yes {
        let new_no_pool = no_pool.checked_add(net).ok_or(PredictError::MathOverflow)?;
        let new_yes_pool = k.checked_div(new_no_pool).ok_or(PredictError::MathOverflow)?;
        (yes_pool.checked_sub(new_yes_pool).ok_or(PredictError::MathOverflow)?) as u64
    } else {
        let new_yes_pool = yes_pool.checked_add(net).ok_or(PredictError::MathOverflow)?;
        let new_no_pool = k.checked_div(new_yes_pool).ok_or(PredictError::MathOverflow)?;
        (no_pool.checked_sub(new_no_pool).ok_or(PredictError::MathOverflow)?) as u64
    };

    require!(shares > 0, PredictError::MathOverflow);
    
    // Slippage Check
    require!(shares >= min_shares_out, PredictError::SlippageExceeded);

    // Determine Mint and Mint To
    let (mint_pubkey, bump) = if outcome == Outcome::Yes {
        (market.yes_mint, market.bump) // We don't need bump here for CPI to mint
        // Actually we need market seeds for signing if market was mint authority, 
        // BUT mint authority IS market PDA.
    } else {
        (market.no_mint, market.bump)
    };

    let market_id_bytes = market.market_id.to_le_bytes();
    let seeds = &[
        b"market",
        market_id_bytes.as_ref(),
        &[market.bump],
    ];
    let signer = &[&seeds[..]];

    let mint_account = if outcome == Outcome::Yes {
        ctx.accounts.yes_mint.to_account_info()
    } else {
        ctx.accounts.no_mint.to_account_info()
    };

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: mint_account,
                to: ctx.accounts.user_share_account.to_account_info(),
                authority: market.to_account_info(),
            },
            signer,
        ),
        shares,
    )?;

    // 5. Update State (CPMM pool reserves)
    market.total_collateral = market.total_collateral
        .checked_add(net_amount)
        .ok_or(PredictError::MathOverflow)?;
    if outcome == Outcome::Yes {
        // User takes YES shares from pool, collateral adds to NO side
        market.total_yes_shares = market.total_yes_shares.checked_sub(shares).ok_or(PredictError::MathOverflow)?;
        market.total_no_shares = market.total_no_shares.checked_add(net_amount).ok_or(PredictError::MathOverflow)?;
    } else {
        // User takes NO shares from pool, collateral adds to YES side
        market.total_no_shares = market.total_no_shares.checked_sub(shares).ok_or(PredictError::MathOverflow)?;
        market.total_yes_shares = market.total_yes_shares.checked_add(net_amount).ok_or(PredictError::MathOverflow)?;
    }

    // Update User Position
    let position = &mut ctx.accounts.user_position;
    position.user = ctx.accounts.user.key();
    position.market = market.key();
    if outcome == Outcome::Yes {
        position.yes_shares = position.yes_shares
            .checked_add(shares)
            .ok_or(PredictError::MathOverflow)?;
    } else {
        position.no_shares = position.no_shares
            .checked_add(shares)
            .ok_or(PredictError::MathOverflow)?;
    }
    position.total_deposited = position.total_deposited
        .checked_add(net_amount)
        .ok_or(PredictError::MathOverflow)?;
    position.last_bet_timestamp = clock.unix_timestamp;
    position.bump = ctx.bumps.user_position;

    emit!(BetPlaced {
        market_id,
        user: ctx.accounts.user.key(),
        outcome,
        amount,
        shares,
        new_yes_total: market.total_yes_shares,
        new_no_total: market.total_no_shares,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
