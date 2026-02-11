use anchor_lang::prelude::*;
use crate::state::PlatformConfig;
use crate::errors::PredictError;
use anchor_spl::token;

#[derive(Accounts)]
pub struct UpdateCollateralMint<'info> {
    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    pub admin: Signer<'info>,
    /// CHECK: Validated below as a valid SPL Mint account
    pub new_collateral_mint: AccountInfo<'info>,
    /// CHECK: Validated below as a token account for the new mint
    pub new_treasury: AccountInfo<'info>,
}

pub fn update_collateral_mint(ctx: Context<UpdateCollateralMint>) -> Result<()> {
    // Validate new_collateral_mint is a valid Mint account (owner = Token program, data length = Mint::LEN)
    require!(
        ctx.accounts.new_collateral_mint.owner == &anchor_spl::token::ID,
        PredictError::InvalidMint
    );
    // Validate new_treasury is owned by Token program (it's a token account)
    require!(
        ctx.accounts.new_treasury.owner == &anchor_spl::token::ID,
        PredictError::InvalidMint
    );
    // Validate treasury token account mint matches the new collateral mint
    let treasury_data = anchor_spl::token::TokenAccount::try_deserialize(
        &mut &ctx.accounts.new_treasury.data.borrow()[..]
    ).map_err(|_| PredictError::InvalidMint)?;
    require!(
        treasury_data.mint == ctx.accounts.new_collateral_mint.key(),
        PredictError::InvalidMint
    );
    ctx.accounts.platform_config.collateral_mint = ctx.accounts.new_collateral_mint.key();
    ctx.accounts.platform_config.treasury = ctx.accounts.new_treasury.key();
    msg!("Collateral mint updated to {}", ctx.accounts.new_collateral_mint.key());
    msg!("Treasury updated to {}", ctx.accounts.new_treasury.key());
    Ok(())
}
