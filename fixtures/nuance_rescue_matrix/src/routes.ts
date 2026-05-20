export const USER_ROUTE_LITERAL = "/api/users/:id";
export const ADMIN_ROUTE_LITERAL = "/api/admin/:id";

export function routeTitleFor(path: string): string {
  if (path === ADMIN_ROUTE_LITERAL) {
    return "Admin profile route";
  }
  if (path === USER_ROUTE_LITERAL) {
    return "User profile route";
  }
  return "Unknown route";
}

