# Buy-Site Consolidation into Main Frontend

**Date**: 2025-12-29
**PR**: #122
**Branch**: `consolidate-buy-site`

## Summary

Consolidated the separate `buy-site/` Next.js application into the main `frontend/` application. This eliminates cross-domain issues and simplifies deployment.

## Problem

The original architecture had two separate frontends:
- `gen.nomadkaraoke.com` - Main karaoke generation app
- `buy.nomadkaraoke.com` - Credit purchase and beta enrollment

This caused a critical bug: **localStorage is domain-isolated**. Auth tokens stored on `buy.nomadkaraoke.com` were invisible to `gen.nomadkaraoke.com`, so users who purchased credits appeared logged out when redirected to the main app.

## Solution

Merged all buy-site functionality into the main frontend:
- Landing page with pricing, beta enrollment at `/` (root)
- Main app (job submission, job list) at `/app`
- Payment success page at `/payment/success`

## Key Changes

### Route Structure
| Before | After |
|--------|-------|
| `gen.nomadkaraoke.com/` | Main app (auth required) |
| `buy.nomadkaraoke.com/` | Landing/pricing |

| After | Route |
|-------|-------|
| Landing/pricing | `/` (root) |
| Main app | `/app` |
| Auth verify | `/auth/verify` |
| Payment success | `/payment/success` |

### Auth Flow
- Unauthenticated users see landing page at root
- Authenticated users are redirected to `/app`
- Magic link verification redirects to `/app`
- Payment success redirects to `/app`

### Files Changed
- Created `frontend/app/app/page.tsx` (main app moved from root)
- Updated `frontend/app/page.tsx` (landing page from buy-site)
- Deleted `frontend/app/welcome/` (no longer needed)
- Updated redirect paths in auth/payment pages
- Updated E2E tests for new URL structure

### Deleted
- Entire `buy-site/` directory (merged into frontend)
- `buy.nomadkaraoke.com` DNS CNAME

## Lessons Learned

1. **Cross-domain localStorage isolation** is a common gotcha when splitting apps across subdomains. Unless you need true multi-tenant isolation, keep auth on a single domain.

2. **Token storage key consistency** matters - we also fixed a mismatch where buy-site stored tokens as `access_token` but frontend expected `auth_token`.

3. **E2E tests caught the real user journey bug** that unit tests missed - the redirect after payment led to an "auth required" state.

## Testing

- Updated `frontend/e2e/production-user-journey.spec.ts` for new URLs
- Added MailSlurp integration for email verification testing
- All E2E tests pass with new route structure
