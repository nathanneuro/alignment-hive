import { useEffect, useRef, useState } from 'react';
import { usePaginatedQuery } from 'convex/react';
import { createFileRoute } from '@tanstack/react-router';
import { api } from '../../../../convex/_generated/api';
import { SessionsTable } from '~/components/sessions-table';

export const Route = createFileRoute('/admin/sessions/')({
  component: SessionsList,
});

type UploadFilter = 'all' | 'uploaded' | 'not-uploaded';

const UNKNOWN_USERS_KEY = '__unknown__';

function SessionsList() {
  const [uploadFilter, setUploadFilter] = useState<UploadFilter>('all');
  const [excludedUserIds, setExcludedUserIds] = useState<Set<string>>(
    new Set()
  );
  const [isUserFilterOpen, setIsUserFilterOpen] = useState(false);
  const filterRef = useRef<HTMLDivElement>(null);

  // Get users for filter dropdown
  const usersData = usePaginatedQuery(
    api.admin.listUsers,
    {},
    { initialNumItems: 100 }
  );

  const excludeUnknownUsers = excludedUserIds.has(UNKNOWN_USERS_KEY);
  const excludeUserIds = [...excludedUserIds].filter(
    (id) => id !== UNKNOWN_USERS_KEY
  );

  const queryArgs = {
    ...(uploadFilter === 'uploaded' && { hasUpload: true }),
    ...(uploadFilter === 'not-uploaded' && { hasUpload: false }),
    ...(excludeUserIds.length > 0 && { excludeUserIds }),
    ...(excludeUnknownUsers && { excludeUnknownUsers: true }),
  };

  const { results, status, loadMore } = usePaginatedQuery(
    api.admin.listSessions,
    queryArgs,
    { initialNumItems: 50 }
  );

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (filterRef.current && !filterRef.current.contains(e.target as Node)) {
        setIsUserFilterOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const toggleUser = (id: string) => {
    setExcludedUserIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const allOptionIds = [
    UNKNOWN_USERS_KEY,
    ...usersData.results.map((u) => u.workosId),
  ];
  const allSelected = excludedUserIds.size === 0;
  const noneSelected = excludedUserIds.size === allOptionIds.length;

  const selectAll = () => setExcludedUserIds(new Set());
  const deselectAll = () => setExcludedUserIds(new Set(allOptionIds));

  const userFilterLabel = allSelected
    ? 'All users'
    : noneSelected
      ? 'No users'
      : `${allOptionIds.length - excludedUserIds.size}/${allOptionIds.length} users`;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold text-foreground">Sessions</h1>
        <div className="flex items-center gap-4">
          <div className="relative" ref={filterRef}>
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">User:</span>
              <button
                onClick={() => setIsUserFilterOpen((v) => !v)}
                className="rounded-md border border-border bg-card px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary"
              >
                {userFilterLabel}
              </button>
            </div>
            {isUserFilterOpen && (
              <div className="absolute right-0 top-full z-10 mt-1 max-h-80 w-64 overflow-y-auto rounded-md border border-border bg-card shadow-lg">
                <div className="flex gap-2 border-b border-border px-3 py-2">
                  <button
                    onClick={selectAll}
                    className="text-xs text-primary hover:underline"
                  >
                    Select all
                  </button>
                  <button
                    onClick={deselectAll}
                    className="text-xs text-primary hover:underline"
                  >
                    Deselect all
                  </button>
                </div>
                <label className="flex cursor-pointer items-center gap-2 border-b border-border px-3 py-2 hover:bg-muted">
                  <input
                    type="checkbox"
                    checked={!excludedUserIds.has(UNKNOWN_USERS_KEY)}
                    onChange={() => toggleUser(UNKNOWN_USERS_KEY)}
                  />
                  <span className="text-sm italic text-muted-foreground">
                    Unknown users
                  </span>
                </label>
                {usersData.results.map((user) => (
                  <label
                    key={user.workosId}
                    className="flex cursor-pointer items-center gap-2 px-3 py-2 hover:bg-muted"
                  >
                    <input
                      type="checkbox"
                      checked={!excludedUserIds.has(user.workosId)}
                      onChange={() => toggleUser(user.workosId)}
                    />
                    <span className="text-sm">
                      {user.firstName && user.lastName
                        ? `${user.firstName} ${user.lastName}`
                        : user.email}
                    </span>
                  </label>
                ))}
              </div>
            )}
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
