use anchor_lang::prelude::*;

#[error_code]
pub enum PredictError {
    #[msg("Platform is paused")]
    PlatformPaused,
    #[msg("Market is not active")]
    MarketNotActive,
    #[msg("Market is not resolved")]
    MarketNotResolved,
    #[msg("Betting period has ended")]
    BettingClosed,
    #[msg("Bet below minimum")]
    BelowMinBet,
    #[msg("Bet above maximum")]
    AboveMaxBet,
    #[msg("Slippage exceeded")]
    SlippageExceeded,
    #[msg("Oracle price stale (>30s)")]
    StaleOracle,
    #[msg("Oracle feed mismatch")]
    OracleMismatch,
    #[msg("Invalid timestamps")]
    InvalidTimestamps,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Market already resolved")]
    AlreadyResolved,
    #[msg("No position to claim")]
    NoPosition,
    #[msg("Already claimed")]
    AlreadyClaimed,
    #[msg("Dispute window expired")]
    DisputeWindowExpired,
    #[msg("Dispute already exists")]
    DisputeExists,
    #[msg("Title too long (max 128)")]
    TitleTooLong,
    #[msg("Description too long (max 512)")]
    DescriptionTooLong,
    #[msg("Invalid outcome for bet")]
    InvalidOutcome,
    #[msg("Round not yet complete")]
    RoundIncomplete,
    #[msg("Market is not recurring")]
    NotRecurring,
    #[msg("Insufficient shares")]
    InsufficientShares,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Fee exceeds maximum (10%)")]
    FeeExceedsMax,
    #[msg("Vault balance insufficient")]
    InsufficientVault,
    #[msg("Initial liquidity must be greater than 0")]
    InsufficientLiquidity,
    #[msg("Market has outstanding positions and cannot be closed")]
    OutstandingPositions,
    #[msg("Market is not in a closeable state")]
    MarketNotCloseable,
    #[msg("Invalid mint account — does not match market PDA-derived mint")]
    InvalidMint,
    #[msg("Pyth feed account does not match market oracle_feed")]
    InvalidPythFeed,
    #[msg("Oracle price is stale")]
    OracleStale,
}
