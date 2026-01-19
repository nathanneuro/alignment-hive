import { Link, Outlet, createFileRoute, redirect } from '@tanstack/react-router';
import { getSignInUrl } from '@workos/authkit-tanstack-react-start';
import { Authenticated, AuthLoading } from 'convex/react';

export const Route = createFileRoute('/admin')({
  beforeLoad: async ({ context, location }) => {
    // Access auth state from root beforeLoad
    const { userId, isAdmin } = context;
    if (!userId) {
      const path = location.pathname;
      const href = await getSignInUrl({ data: { returnPathname: path } });
      throw redirect({ href });
    }
    if (!isAdmin) {
      throw redirect({ to: '/' });
    }
  },
  component: AdminLayout,
});

function AdminLayout() {
  return (
    <div className="min-h-screen bg-background">
      <nav className="border-b border-border bg-card">
        <div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
          <div className="flex h-14 items-center justify-between">
            <div className="flex items-center gap-6">
              <Link to="/" className="font-semibold text-foreground">
                alignment-hive
              </Link>
              <span className="text-muted-foreground text-sm">Admin</span>
            </div>
            <div className="flex items-center gap-4">
              <Link
                to="/admin/sessions"
                className="text-sm text-muted-foreground hover:text-foreground [&.active]:text-foreground [&.active]:font-medium"
              >
                Sessions
              </Link>
              <Link
                to="/admin/users"
                className="text-sm text-muted-foreground hover:text-foreground [&.active]:text-foreground [&.active]:font-medium"
              >
                Users
              </Link>
              <Link
                to="/auth/sign-out"
                className="text-sm text-muted-foreground hover:text-foreground"
              >
                Sign out
              </Link>
            </div>
          </div>
        </div>
      </nav>
      <main className="mx-auto max-w-7xl px-4 py-6 sm:px-6 lg:px-8">
        <AuthLoading>
          <div className="text-muted-foreground">Loading...</div>
        </AuthLoading>
        <Authenticated>
          <Outlet />
        </Authenticated>
      </main>
    </div>
  );
}
