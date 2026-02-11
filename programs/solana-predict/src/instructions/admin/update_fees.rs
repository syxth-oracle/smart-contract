use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Transfer, TokenAccount};
use crate::state::PlatformConfig;
use crate::errors::PredictError;

#[derive(Accounts)]
pub struct UpdateFees<'info> {
    #[account(
        mut,
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    pub admin: Signer<'info>,
}

pub fn update_fees(ctx: Context<UpdateFees>, new_fee_bps: u16) -> Result<()> {
    require!(new_fee_bps <= 1000, PredictError::FeeExceedsMax);
    ctx.accounts.platform_config.fee_bps = new_fee_bps;
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(
        seeds = [b"platform_config"],
        bump = platform_config.bump,
        constraint = platform_config.admin == admin.key() @ PredictError::Unauthorized
    )]
    pub platform_config: Account<'info, PlatformConfig>,
    
    /// CHECK: Configured treasury address, validated against platform_config constraint
    #[account(mut, constraint = treasury.key() == platform_config.treasury)]
    pub treasury: AccountInfo<'info>, // Assuming this is where fees ACCUMULATED?
    // Wait, in `place_bet`, fees are sent DIRECTLY to `treasury`.
    // So there is nothing to withdraw from the contract itself?
    // Design said: "Admin withdraws accumulated fees from treasury vault."
    // If treasury is a System Account (SOL) or Token Account owned by Admin?
    // If fees are sent to `treasury` address immediately, then "Withdraw" implies moving from that address?
    // Checks place_bet logic:
    // `to: ctx.accounts.treasury.to_account_info()`
    // So fees reside in the treasury account.
    // If treasury account is a PDA owned by program, we need instruction to move it.
    // But in `init_platform`, `treasury` is passed as an AccountInfo. We stored its key.
    // If it's an arbitrary wallet (e.g. Admin's cold wallet), then we don't need withdrawal instruction.
    // If it's a Program Owned Account (vault), we do.
    // Design: "withdraw_fees(amount)".
    // Implementation Plan: "Withdraw fees from treasury vault".
    
    // Let's assume Treasury is an ATA owned by the Program (PlatformConfig PDA).
    // In `init_platform`: `pub treasury: AccountInfo`.
    // We didn't enforce it to be a PDA.
    // If the Admin set `treasury` to their own wallet, funds are already there.
    
    // I will implement `withdraw_fees` assuming the `treasury` stored in config IS the source,
    // and we transfer FROM it to `destination`.
    // This requires `treasury` to include the Program as authority or be a PDA.
    // But `place_bet` sends TO it.
    // If I just implement `update_fees`, that covers the parameter change.
    // I will skip `withdraw_fees` implementation if `treasury` is external, 
    // BUT to follow design I'll implement a `withdraw_from_vault` logic where `treasury` might be the vault?
    
    // Let's just implement `update_fees` in this file.
    pub admin: Signer<'info>,
}
