import { usePaginatedQuery } from 'convex/react';
import { Link, createFileRoute } from '@tanstack/react-router';
import { api } from '../../../../convex/_generated/api';

export const Route = createFileRoute('/admin/users/$userId')({
  component: UserDetail,
});

function UserDetail() {
  const { userId } = Route.useParams();

  // Get user's sessions
  const { results, status, loadMore } = usePaginatedQuery(
    api.admin.getUserSessions,
    { userId },
    { initialNumItems: 50 }
  );

  // Get user info from listUsers query
  const usersData = usePaginatedQuery(
    api.admin.listUsers,
    {},
    { initialNumItems: 100 }
  );

  const user = usersData.results.find((u) => u.workosId === userId);

  if (!user && usersData.status === 'Exhausted') {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-destructive">User not found</div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Link to="/admin/users" className="hover:text-foreground">
          Users
        </Link>
        <span>/</span>
        <span>{user?.firstName ?? userId.slice(0, 8)}</span>
      </div>

      {user && (
        <div className="rounded-lg border border-border bg-card p-4">
          <h1 className="text-xl font-semibold text-foreground">
            {user.firstName} {user.lastName}
          </h1>
          <p className="text-sm text-muted-foreground">{user.email}</p>
          <div className="mt-4 flex gap-6 text-sm">
            <div>
              <span className="text-muted-foreground">Sessions: </span>
              <span className="font-medium">{user.sessionCount}</span>
            </div>
            <div>
              <span className="text-muted-foreground">Uploads: </span>
              <span className="font-medium">{user.uploadCount}</span>
            </div>
            <div>
              <span className="text-muted-foreground">Last active: </span>
              <span className="font-medium">
                {user.lastActive
                  ? formatRelativeTime(user.lastActive)
                  : '—'}
              </span>
            </div>
          </div>
          {user.projects.length > 0 && (
            <div className="mt-4">
              <span className="text-sm text-muted-foreground">Projects: </span>
              <span className="text-sm">
                {user.projects.map(formatProject).join(', ')}
              </span>
            </div>
          )}
        </div>
      )}

      <div className="space-y-4">
        <h2 className="text-lg font-semibold text-foreground">Sessions</h2>

        <div className="rounded-lg border border-border bg-card">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border text-left text-sm text-muted-foreground">
                <th className="px-4 py-3 font-medium">Session</th>
                <th className="px-4 py-3 font-medium">Project</th>
                <th className="px-4 py-3 font-medium">Lines</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Last Activity</th>
                <th className="px-4 py-3 font-medium"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {results.map((session) => (
                <tr key={session._id} className="hover:bg-muted/50">
                  <td className="px-4 py-3 font-mono text-sm">
                    {session.sessionId.slice(0, 8)}
                  </td>
                  <td className="px-4 py-3 text-sm text-muted-foreground truncate max-w-[200px]" title={session.project}>
                    {formatProject(session.project)}
                  </td>
                  <td className="px-4 py-3 text-sm tabular-nums">
                    {session.lineCount}
                  </td>
                  <td className="px-4 py-3">
                    {session.upload ? (
                      <span className="inline-flex items-center rounded-full bg-green-500/10 px-2 py-0.5 text-xs font-medium text-green-600 dark:text-green-400">
                        Uploaded
                      </span>
                    ) : (
                      <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                        Not uploaded
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm text-muted-foreground">
                    {formatRelativeTime(session.lastHeartbeat)}
                  </td>
                  <td className="px-4 py-3 text-right">
                    {session.upload && (
                      <Link
                        to="/admin/sessions/$sessionId"
                        params={{ sessionId: session.sessionId }}
                        className="text-sm text-primary hover:underline"
                      >
                        View
                      </Link>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {status === 'CanLoadMore' && (
          <button
            onClick={() => loadMore(50)}
            className="w-full rounded-lg border border-border bg-card py-2 text-sm text-muted-foreground hover:bg-muted"
          >
            Load more
          </button>
        )}
      </div>
    </div>
  );
}

function formatRelativeTime(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp;

  const minutes = Math.floor(diff / 60000);
  if (minutes < 60) return `${minutes}m ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;

  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;

  return new Date(timestamp).toLocaleDateString();
}

function formatProject(project: string): string {
  // Extract repo name from github.com/owner/repo format
  const match = project.match(/github\.com\/([^/]+\/[^/]+)/);
  if (match) {
    return match[1];
  }
  // For local paths, show just the last directory
  const parts = project.split('/');
  return parts[parts.length - 1] || project;
}
