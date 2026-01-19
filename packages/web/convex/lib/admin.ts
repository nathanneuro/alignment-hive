import type { QueryCtx, MutationCtx } from '../_generated/server';

function getAdminEmails(): string[] {
  const envValue = process.env.ADMIN_EMAILS ?? '';
  return envValue
    .split('\n')
    .map((e) => e.trim())
    .filter(Boolean);
}

/**
 * Check if the current user is an admin.
 * Returns null if not authenticated (e.g., during SSR/loading).
 * Throws if authenticated but not an admin.
 */
export async function requireAdmin(ctx: QueryCtx | MutationCtx) {
  const identity = await ctx.auth.getUserIdentity();
  if (!identity) {
    // Not authenticated - return null for SSR/loading requests
    return null;
  }
  const adminEmails = getAdminEmails();
  if (!identity.email || !adminEmails.includes(identity.email)) {
    throw new Error('Not authorized');
  }
  return identity;
}
