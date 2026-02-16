import { v } from 'convex/values';
import { mutation, query, internalMutation } from './_generated/server';

export const heartbeatSession = mutation({
  args: {
    sessionId: v.string(),
    checkoutId: v.string(),
    project: v.string(),
    lineCount: v.number(),
    parentSessionId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error('Not authenticated');
    }

    const userId = identity.subject;
    const now = Date.now();

    // Access name claims (customJwt passes through snake_case keys from JWT)
    const givenName = (identity as Record<string, unknown>)['given_name'] as
      | string
      | undefined;
    const familyName = (identity as Record<string, unknown>)['family_name'] as
      | string
      | undefined;

    // Upsert user record with latest identity info
    const existingUser = await ctx.db
      .query('users')
      .withIndex('by_workos_id', (q) => q.eq('workosId', userId))
      .first();

    if (existingUser) {
      // Update user if name fields are missing or email changed
      if (
        existingUser.firstName !== givenName ||
        existingUser.lastName !== familyName ||
        existingUser.email !== identity.email
      ) {
        await ctx.db.patch(existingUser._id, {
          email: identity.email ?? existingUser.email,
          firstName: givenName ?? existingUser.firstName,
          lastName: familyName ?? existingUser.lastName,
        });
      }
    } else if (identity.email) {
      await ctx.db.insert('users', {
        workosId: userId,
        email: identity.email,
        firstName: givenName,
        lastName: familyName,
      });
    }

    const existing = await ctx.db
      .query('sessions')
      .withIndex('by_session_id', (q) => q.eq('sessionId', args.sessionId))
      .first();

    if (existing) {
      if (existing.userId !== userId) {
        throw new Error('Session belongs to different user');
      }
      await ctx.db.patch(existing._id, {
        lineCount: args.lineCount,
        lastHeartbeat: now,
      });
    } else {
      await ctx.db.insert('sessions', {
        sessionId: args.sessionId,
        userId,
        checkoutId: args.checkoutId,
        project: args.project,
        lineCount: args.lineCount,
        lastHeartbeat: now,
        parentSessionId: args.parentSessionId,
      });
    }
  },
});

export const generateUploadUrl = mutation({
  args: { sessionId: v.string() },
  handler: async (ctx, { sessionId }) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error('Not authenticated');
    }

    const session = await ctx.db
      .query('sessions')
      .withIndex('by_session_id', (q) => q.eq('sessionId', sessionId))
      .first();

    if (!session) {
      throw new Error('Session not found - heartbeat first');
    }
    if (session.userId !== identity.subject) {
      throw new Error('Session belongs to different user');
    }

    return await ctx.storage.generateUploadUrl();
  },
});

export const saveUpload = mutation({
  args: {
    sessionId: v.string(),
    storageId: v.id('_storage'),
    summary: v.optional(v.string()),
  },
  handler: async (ctx, { sessionId, storageId, summary }) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      throw new Error('Not authenticated');
    }

    const session = await ctx.db
      .query('sessions')
      .withIndex('by_session_id', (q) => q.eq('sessionId', sessionId))
      .first();

    if (!session) {
      throw new Error('Session not found');
    }
    if (session.userId !== identity.subject) {
      throw new Error('Session belongs to different user');
    }

    await ctx.db.patch(session._id, {
      ...(summary && { summary }),
      upload: {
        storageId,
        uploadedAt: Date.now(),
      },
    });
  },
});

export const listUserSessions = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      return [];
    }

    return await ctx.db
      .query('sessions')
      .withIndex('by_user_id', (q) => q.eq('userId', identity.subject))
      .collect();
  },
});

export const upsertCheckout = mutation({
  args: { checkoutId: v.string() },
  handler: async (ctx, { checkoutId }) => {
    const now = Date.now();

    const existing = await ctx.db
      .query('checkouts')
      .withIndex('by_checkout_id', (q) => q.eq('checkoutId', checkoutId))
      .first();

    if (existing) {
      await ctx.db.patch(existing._id, { lastSeenAt: now });
    } else {
      await ctx.db.insert('checkouts', {
        checkoutId,
        firstSeenAt: now,
        lastSeenAt: now,
      });
    }
  },
});

