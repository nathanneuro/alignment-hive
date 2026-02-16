import { useState } from 'react';
import { usePaginatedQuery } from 'convex/react';
import { createFileRoute } from '@tanstack/react-router';
import { api } from '../../../../convex/_generated/api';
import { SessionsTable } from '~/components/sessions-table';

export const Route = createFileRoute('/admin/sessions/')({
  component: SessionsList,
});

type UploadFilter = 'all' | 'uploaded' | 'not-uploaded';

function SessionsList() {
  const [uploadFilter, setUploadFilter] = useState<UploadFilter>('all');
  const [userFilter, setUserFilter] = useState<string>('all');

  // Get users for filter dropdown
  const usersData = usePaginatedQuery(
    api.admin.listUsers,
    {},
    { initialNumItems: 100 }
  );

  const queryArgs = {
    ...(uploadFilter === 'uploaded' && { hasUpload: true }),
    ...(uploadFilter === 'not-uploaded' && { hasUpload: false }),
    ...(userFilter !== 'all' && { userId: userFilter }),
  };

  const { results, status, loadMore } = usePaginatedQuery(
    api.admin.listSessions,
    queryArgs,
    { initialNumItems: 50 }
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold text-foreground">Sessions</h1>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">User:</span>
            <select
              value={userFilter}
              onChange={(e) => setUserFilter(e.target.value)}
              className="rounded-md border border-border bg-card px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary"
            >
              <option value="all">All users</option>
              {usersData.results.map((user) => (
                <option key={user.workosId} value={user.workosId}>
                  {user.firstName && user.lastName
                    ? `${user.firstName} ${user.lastName}`
                    : user.email}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Status:</span>
            <select
              value={uploadFilter}
              onChange={(e) => setUploadFilter(e.target.value as UploadFilter)}
              className="rounded-md border border-border bg-card px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary"
            >
              <option value="all">All</option>
              <option value="uploaded">Uploaded</option>
              <option value="not-uploaded">Not uploaded</option>
            </select>
          </div>
        </div>
      </div>

      <SessionsTable sessions={results} loading={status === 'LoadingFirstPage'} />

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
