import { paginationOptsValidator } from 'convex/server';
import { v } from 'convex/values';
import { query } from './_generated/server';
import { requireAdmin } from './lib/admin';

export const listSessions = query({
  args: {
    paginationOpts: paginationOptsValidator,
    userId: v.optional(v.string()),
    project: v.optional(v.string()),
    hasUpload: v.optional(v.boolean()),
  },
  handler: async (ctx, args) => {
    const identity = await requireAdmin(ctx);
    if (!identity) {
      // Return empty results during SSR
      return { page: [], isDone: true, continueCursor: '' };
    }

    // Build query with filters
    let sessionsQuery = ctx.db.query('sessions').order('desc');

    if (args.userId) {
      sessionsQuery = ctx.db
        .query('sessions')
        .withIndex('by_user_id', (q) => q.eq('userId', args.userId!))
        .order('desc');
    }

    const paginatedSessions = await sessionsQuery.paginate(args.paginationOpts);

    // Apply client-side filters for project and hasUpload
    let filteredPage = paginatedSessions.page;
    if (args.project) {
      filteredPage = filteredPage.filter((s) => s.project === args.project);
    }
    if (args.hasUpload !== undefined) {
      filteredPage = filteredPage.filter((s) =>
        args.hasUpload ? s.upload !== undefined : s.upload === undefined
      );
    }

    // Get user info for each session
    const userIds = [...new Set(filteredPage.map((s) => s.userId))];
    const users = await Promise.all(
      userIds.map((workosId) =>
        ctx.db
          .query('users')
          .withIndex('by_workos_id', (q) => q.eq('workosId', workosId))
          .first()
      )
    );
    const userMap = new Map(
      users.filter(Boolean).map((u) => [u!.workosId, u!])
    );

    const sessionsWithUsers = filteredPage.map((session) => ({
      ...session,
      user: userMap.get(session.userId) ?? null,
    }));

    return {
      ...paginatedSessions,
      page: sessionsWithUsers,
    };
  },
});

export const getSession = query({
  args: { sessionId: v.string() },
  handler: async (ctx, args) => {
    const identity = await requireAdmin(ctx);
    if (!identity) {
      // Return null during SSR
      return null;
    }

    const session = await ctx.db
      .query('sessions')
      .withIndex('by_session_id', (q) => q.eq('sessionId', args.sessionId))
      .first();

    if (!session) return null;

    // Get presigned URL if uploaded
    const contentUrl = session.upload
      ? await ctx.storage.getUrl(session.upload.storageId)
      : null;

    // Get user info
    const user = await ctx.db
      .query('users')
      .withIndex('by_workos_id', (q) => q.eq('workosId', session.userId))
      .first();

    // Get user session count
    const userSessions = user
      ? await ctx.db
          .query('sessions')
          .withIndex('by_user_id', (q) => q.eq('userId', session.userId))
          .collect()
      : [];

    const userWithStats = user
      ? {
          ...user,
          sessionCount: userSessions.length,
          uploadCount: userSessions.filter((s) => s.upload).length,
        }
      : null;

    // Get parent session info if this is an agent session
    const parentSession = session.parentSessionId
      ? await ctx.db
          .query('sessions')
          .withIndex('by_session_id', (q) =>
            q.eq('sessionId', session.parentSessionId!)
          )
          .first()
      : null;

    // Get child agent sessions
    const childSessions = await ctx.db
      .query('sessions')
      .withIndex('by_parent_session_id', (q) =>
        q.eq('parentSessionId', args.sessionId)
      )
      .collect();

    return {
      session,
      contentUrl,
      user: userWithStats,
      parentSession,
      childSessions,
    };
  },
});

export const listUsers = query({
  args: { paginationOpts: paginationOptsValidator },
  handler: async (ctx, args) => {
    const identity = await requireAdmin(ctx);
    if (!identity) {
      // Return empty results during SSR
      return { page: [], isDone: true, continueCursor: '' };
    }

    const paginatedUsers = await ctx.db
      .query('users')
      .order('desc')
      .paginate(args.paginationOpts);

    // Get stats for each user
    const usersWithStats = await Promise.all(
      paginatedUsers.page.map(async (user) => {
        const sessions = await ctx.db
          .query('sessions')
          .withIndex('by_user_id', (q) => q.eq('userId', user.workosId))
          .collect();

        const projects = [...new Set(sessions.map((s) => s.project))];
        const lastSession = sessions.reduce(
          (latest, s) =>
            !latest || s.lastHeartbeat > latest.lastHeartbeat ? s : latest,
          null as (typeof sessions)[0] | null
        );

        return {
          ...user,
          sessionCount: sessions.length,
          uploadCount: sessions.filter((s) => s.upload).length,
          projects,
          lastActive: lastSession?.lastHeartbeat ?? null,
        };
      })
    );

    return {
      ...paginatedUsers,
      page: usersWithStats,
    };
  },
});

export const getUserSessions = query({
  args: {
    paginationOpts: paginationOptsValidator,
    userId: v.string(),
  },
  handler: async (ctx, args) => {
    const identity = await requireAdmin(ctx);
    if (!identity) {
      // Return empty results during SSR
      return { page: [], isDone: true, continueCursor: '' };
    }

    return await ctx.db
      .query('sessions')
      .withIndex('by_user_id', (q) => q.eq('userId', args.userId))
      .order('desc')
      .paginate(args.paginationOpts);
  },
});
