import { usePaginatedQuery } from 'convex/react';
import { Link, createFileRoute } from '@tanstack/react-router';
import { api } from '../../../../convex/_generated/api';

export const Route = createFileRoute('/admin/sessions/')({
  component: SessionsList,
});

function SessionsList() {
  const { results, status, loadMore } = usePaginatedQuery(
    api.admin.listSessions,
    {},
    { initialNumItems: 50 }
  );

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold text-foreground">Sessions</h1>

      <div className="rounded-lg border border-border bg-card">
        <table className="w-full">
          <thead>
            <tr className="border-b border-border text-left text-sm text-muted-foreground">
              <th className="px-4 py-3 font-medium">Session</th>
              <th className="px-4 py-3 font-medium">User</th>
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
                <td className="px-4 py-3 text-sm">
                  {session.user ? (
                    <Link
                      to="/admin/users/$userId"
                      params={{ userId: session.userId }}
                      className="text-primary hover:underline"
                    >
                      {session.user.firstName ?? session.user.email}
                    </Link>
                  ) : (
                    <span className="text-muted-foreground">Unknown</span>
                  )}
                </td>
                <td className="px-4 py-3 text-sm text-muted-foreground">
                  {session.project}
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
