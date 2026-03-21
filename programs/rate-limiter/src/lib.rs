use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, System};
use spl_token::prelude::*;

declare_id!("Brg18kjrWYWA7rQvabRmGQDChf7d844T2NeVkaPSykwd");

// ─────────────────────────────────────────────────
// Account Structs
// ─────────────────────────────────────────────────

#[account]
pub struct GlobalConfig {
    pub admin: Pubkey,
    pub bump: u8,
}

#[account]
pub struct ApiKeyAccount {
    pub owner: Pubkey,
    pub quota: u64,          // max requests per window
    pub used: u64,           // requests used in current window
    pub window_slots: u64,   // window length in slots
    pub window_start: u64,   // slot when current window started
    pub is_active: bool,
    pub created_at: u64,
}

// ─────────────────────────────────────────────────
// PDAs
// ─────────────────────────────────────────────────

fn global_config_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"config"], &ID)
}

fn api_key_address(owner: &Pubkey, seed: &[u8]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"api_key", owner.as_ref(), seed], &ID)
}

// ─────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────

#[error_code]
pub enum RateLimitError {
    #[msg("Rate limit exceeded")]
    RateLimitExceeded,
    #[msg("Invalid window configuration")]
    InvalidWindow,
    #[msg("API key is revoked")]
    KeyRevoked,
    #[msg("Unauthorized: not the key owner or admin")]
    Unauthorized,
}

// ─────────────────────────────────────────────────
// Instructions
// ─────────────────────────────────────────────────

/// Initialize the global config (one-time, called by deployer)
pub fn initialize_config(ctx: Context<InitializeConfig>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.bump = ctx.bumps.config;
    Ok(())
}

/// Create a new API key account with rate limit config
/// Seeds: ["api_key", owner, key_seed]
pub fn create_api_key(
    ctx: Context<CreateApiKey>,
    key_seed: Vec<u8>,
    quota: u64,
    window_slots: u64,
) -> Result<()> {
    require!(quota > 0, RateLimitError::InvalidWindow);
    require!(window_slots > 0, RateLimitError::InvalidWindow);

    let clock = Clock::get()?;
    let current_slot = clock.slot;

    let api_key = &mut ctx.accounts.api_key;
    api_key.owner = ctx.accounts.owner.key();
    api_key.quota = quota;
    api_key.used = 0;
    api_key.window_slots = window_slots;
    api_key.window_start = current_slot;
    api_key.is_active = true;
    api_key.created_at = clock.unix_timestamp as u64;

    emit!(ApiKeyCreated {
        owner: api_key.owner,
        quota,
        window_slots,
    });

    Ok(())
}

/// Check if a request is within rate limit (view function equivalent)
/// Returns Err if rate limit exceeded, Ok if allowed
pub fn check_rate_limit(ctx: Context<CheckRateLimit>) -> Result<()> {
    let api_key = &ctx.accounts.api_key;
    
    require!(api_key.is_active, RateLimitError::KeyRevoked);

    let clock = Clock::get()?;
    let current_slot = clock.slot;

    // Sliding window: reset if window expired
    let window_elapsed = current_slot.saturating_sub(api_key.window_start);
    
    if window_elapsed >= api_key.window_slots {
        // Window expired, allow request (will be counted by increment_counter)
        return Ok(());
    }

    // Check quota
    require!(
        api_key.used < api_key.quota,
        RateLimitError::RateLimitExceeded
    );

    Ok(())
}

/// Increment the request counter (call this when a request is made)
pub fn increment_counter(ctx: Context<IncrementCounter>) -> Result<()> {
    let api_key = &mut ctx.accounts.api_key;
    
    require!(api_key.is_active, RateLimitError::KeyRevoked);

    let clock = Clock::get()?;
    let current_slot = clock.slot;

    // Sliding window reset
    let window_elapsed = current_slot.saturating_sub(api_key.window_start);
    
    if window_elapsed >= api_key.window_slots {
        // New window starts
        api_key.window_start = current_slot;
        api_key.used = 1;
    } else {
        require!(
            api_key.used < api_key.quota,
            RateLimitError::RateLimitExceeded
        );
        api_key.used += 1;
    }

    emit!(RequestCounted {
        owner: api_key.owner,
        used: api_key.used,
        quota: api_key.quota,
    });

    Ok(())
}

/// Admin: revoke an API key
pub fn revoke_api_key(ctx: Context<RevokeApiKey>) -> Result<()> {
    let api_key = &mut ctx.accounts.api_key;
    api_key.is_active = false;

    emit!(ApiKeyRevoked { owner: api_key.owner });
    Ok(())
}

/// Admin or owner: update the rate limit quota
pub fn update_quota(ctx: Context<UpdateQuota>, new_quota: u64) -> Result<()> {
    require!(new_quota > 0, RateLimitError::InvalidWindow);
    let api_key = &mut ctx.accounts.api_key;
    api_key.quota = new_quota;

    emit!(QuotaUpdated {
        owner: api_key.owner,
        new_quota,
    });
    Ok(())
}

/// Reset counter (admin or owner, typically after quota review)
pub fn reset_counter(ctx: Context<ResetCounter>) -> Result<()> {
    let clock = Clock::get()?;
    let api_key = &mut ctx.accounts.api_key;
    api_key.used = 0;
    api_key.window_start = clock.slot;
    Ok(())
}

// ─────────────────────────────────────────────────
// Events
// ─────────────────────────────────────────────────

#[event]
struct ApiKeyCreated {
    owner: Pubkey,
    quota: u64,
    window_slots: u64,
}

#[event]
struct RequestCounted {
    owner: Pubkey,
    used: u64,
    quota: u64,
}

#[event]
struct ApiKeyRevoked {
    owner: Pubkey,
}

#[event]
struct QuotaUpdated {
    owner: Pubkey,
    new_quota: u64,
}

// ─────────────────────────────────────────────────
// Account Validation
// ─────────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + GlobalConfig::INIT_SPACE,
        seeds = [b"config"],
        bump,
    )]
    pub config: Account<'info, GlobalConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateApiKey<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + ApiKeyAccount::INIT_SPACE,
        seeds = [b"api_key", owner.key().as_ref(), &key_seed],
        bump,
    )]
    pub api_key: Account<'info, ApiKeyAccount>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CheckRateLimit<'info> {
    pub api_key: Account<'info, ApiKeyAccount>,
}

#[derive(Accounts)]
pub struct IncrementCounter<'info> {
    #[account(mut)]
    pub api_key: Account<'info, ApiKeyAccount>,
}

#[derive(Accounts)]
pub struct RevokeApiKey<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub api_key: Account<'info, ApiKeyAccount>,
}

#[derive(Accounts)]
pub struct UpdateQuota<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub api_key: Account<'info, ApiKeyAccount>,
}

#[derive(Accounts)]
pub struct ResetCounter<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, GlobalConfig>,
    #[account(mut)]
    pub api_key: Account<'info, ApiKeyAccount>,
}

// ─────────────────────────────────────────────────
// Space constants
// ─────────────────────────────────────────────────

impl GlobalConfig {
    pub const INIT_SPACE: usize = 32 + 1; // admin + bump
}

impl ApiKeyAccount {
    pub const INIT_SPACE: usize = 32 + 8 + 8 + 8 + 8 + 1 + 8; // owner + quota + used + window_slots + window_start + is_active + created_at
}
