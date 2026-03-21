# Solana On-chain Rate Limiter

**Web2 Concept:** API Rate Limiting with Redis + API Keys
**On-chain Equivalent:** Token-gated request counters with per-key quotas on Solana

## Problem: Web2 → Web3 Translation

### Web2 Rate Limiter (Redis-based)
```
API Key → Lookup key:ratelimit:{key} 
        → INCR + EXPIRE
        → Reject if > 1000 req/hour
```
- **State:** Redis (mutable, centralized)
- **Logic:** Server-side, opaque to users
- **Trust:** Requires trusted operator

### On-chain Equivalent (Solana)
```
API Key NFT → Lookup account by key Pubkey
            → Increment counter (CPI)
            → Reject if counter > max_quota
```
- **State:** On-chain accounts (immutable, decentralized)
- **Logic:** Transparent, verifiable by anyone
- **Trust:** Cryptographic, permissionless

## Core Features

- [x] `create_api_key` — Mint API key NFT, set rate limit config
- [x] `increment_counter` — Called by service to record request
- [x] `check_rate_limit` — Verify if key is within quota
- [x] `revoke_api_key` — Admin can revoke a key
- [x] `update_quota` — Update rate limit config

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  RateLimiterProgram (on-chain)                              │
│                                                             │
│  ┌─────────────────┐  ┌──────────────────┐                 │
│  │ GlobalConfig    │  │ ApiKeyAccount    │                 │
│  │ - admin         │  │ - owner          │                 │
│  │ - default_limit │  │ - quota          │                 │
│  │ - window_secs   │  │ - used           │                 │
│  └─────────────────┘  │ - last_reset     │                 │
│                      │ - is_active      │                 │
│                      └──────────────────┘                 │
└─────────────────────────────────────────────────────────────┘
```

## Usage (CLI)

```bash
# Create a new API key with 10,000 req/hour limit
solana-test-validator &
anchor test
anchor deploy --provider.cluster devnet

# Or with the JS client
node tests/rate-limiter.js
```

## Build & Test

```bash
anchor build
anchor test
solana-test-validator
```

## Security Notes

- Admin PDA has sole authority to revoke/update keys
- Rate limit counters are non-reentrant via CPI
- Uses `invoke_signed` for PDA key creation
