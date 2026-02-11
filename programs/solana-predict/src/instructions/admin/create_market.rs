use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Mint, TokenAccount, Transfer};
use crate::state::{PlatformConfig, Market, MarketCategory, MarketStatus, OracleSource, Outcome};
use crate::events::MarketCreated;
use crate::errors::PredictError;

#[derive(Accounts)]
#[instruction(market_id: u64)] // market_id is passed as instruction arg to derive seeds
pub struct CreateMarket<'info> {
    #[account(
        init,
        seeds = [b"market", market_id.to_le_bytes().as_ref()],
        bump,
        payer = admin,
        space = Market::LEN
    )]
    pub market: Account<'info, Market>,

    #[account(
        init,
        seeds = [b"yes_mint", market.key().as_ref()],
        bump,
        payer = admin,
        mint::decimals = 9,
        mint::authority = market,
    )]
    pub yes_mint: Account<'info, Mint>,

    #[account(
        init,
        seeds = [b"no_mint", market.key().as_ref()],
        bump,
        payer = admin,
        mint::decimals = 9,
        mint::authority = market,
    )]
    pub no_mint: Account<'info, Mint>,

    #[account(
        init,
        seeds = [b"vault", market.key().as_ref()],
        bump,
        payer = admin,
        token::mint = collateral_mint,
        token::authority = market,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        has_one = collateral_mint,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,

    #[account(mut)]
    pub admin: Signer<'info>,

    /// Admin's collateral token account (wSOL ATA) for depositing initial liquidity
    #[account(
        mut,
        token::mint = collateral_mint,
        token::authority = admin,
    )]
    pub admin_ata: Account<'info, TokenAccount>,

    pub collateral_mint: Account<'info, Mint>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateMarketParams {
    pub title: String,
    pub description: String,
    pub category: MarketCategory,
    pub oracle_source: OracleSource,
    pub oracle_feed: Pubkey,
    pub oracle_threshold: i64,
    pub start_timestamp: i64,
    pub lock_timestamp: i64,
    pub end_timestamp: i64,
    pub min_bet: u64,
    pub max_bet: u64,
    pub is_recurring: bool,
    pub round_duration: Option<i64>,
    pub fee_bps: u16,
    pub initial_liquidity: u64,
}

pub fn process_create_market(
    ctx: Context<CreateMarket>,
    market_id: u64,
    params: CreateMarketParams,
) -> Result<()> {
    let platform = &mut ctx.accounts.platform_config;
    let market = &mut ctx.accounts.market;
    let clock = Clock::get()?;

    // Validation
    require!(!platform.paused, PredictError::PlatformPaused);
    require!(params.title.len() <= 128, PredictError::TitleTooLong);
    require!(params.description.len() <= 512, PredictError::DescriptionTooLong);
    require!(
        params.start_timestamp < params.lock_timestamp && params.lock_timestamp < params.end_timestamp,
        PredictError::InvalidTimestamps
    );
    require!(params.fee_bps <= 1000, PredictError::FeeExceedsMax);
    require!(params.initial_liquidity > 0, PredictError::InsufficientLiquidity);

    // Transfer initial liquidity from admin to vault (seeds CPMM pools)
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.admin_ata.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.admin.to_account_info(),
            },
        ),
        params.initial_liquidity,
    )?;

    // Initialize Market
    market.market_id = market_id;
    market.creator = ctx.accounts.admin.key(); // Admin is creator for now
    market.title = params.title.clone();
    market.description = params.description;
    market.category = params.category;
    market.status = if params.start_timestamp <= clock.unix_timestamp {
        MarketStatus::Active
    } else {
        MarketStatus::Pending
    };
    market.collateral_mint = ctx.accounts.collateral_mint.key();
    market.yes_mint = ctx.accounts.yes_mint.key();
    market.no_mint = ctx.accounts.no_mint.key();
    market.vault = ctx.accounts.vault.key();
    // CPMM: seed equal YES/NO pools so k = initial_liquidity^2
    market.total_yes_shares = params.initial_liquidity;
    market.total_no_shares = params.initial_liquidity;
    market.total_collateral = params.initial_liquidity;
    market.oracle_source = params.oracle_source;
    market.oracle_feed = params.oracle_feed;
    market.oracle_threshold = params.oracle_threshold;
    market.start_timestamp = params.start_timestamp;
    market.lock_timestamp = params.lock_timestamp;
    market.end_timestamp = params.end_timestamp;
    market.resolved_outcome = None;
    market.resolution_price = None;
    market.min_bet = params.min_bet;
    market.max_bet = params.max_bet;
    market.fee_bps = params.fee_bps;
    market.is_recurring = params.is_recurring;
    market.round_duration = params.round_duration;
    market.current_round = 0;
    market.bump = ctx.bumps.market;

    // Update Platform Config (increment total markets)
    // NOTE: In a real scenario, we might want to use the platform.total_markets as the market_id 
    // to ensure sequential IDs, but here we passed it as param for deterministic seed generation client-side.
    // Ideally, the client reads total_markets, calls this with that ID, and we verify it matches or we just use it.
    // If we want strict sequential on-chain logic, we should derive seeds from the counter inside the instruction, 
    // but that makes client-side PDA derivation harder (requires fetching count first).
    // For now, we update the counter to track usage.
    platform.total_markets = platform.total_markets.checked_add(1).ok_or(PredictError::MathOverflow)?;

    emit!(MarketCreated {
        market_id,
        creator: market.creator,
        title: market.title.clone(),
        oracle_source: market.oracle_source,
        end_timestamp: market.end_timestamp,
    });

    Ok(())
}
