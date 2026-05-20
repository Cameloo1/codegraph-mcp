import { check_admin, not_authorized, type Principal } from "../src/auth_guards";
import { ADMIN_ROUTE_LITERAL } from "../src/routes";

export function failing_test_admin_route_rejects_user_token_when_not_authorized(): boolean {
  const user: Principal = { role: "user" };
  return ADMIN_ROUTE_LITERAL === "/api/admin/:id" && !check_admin(user) && !not_authorized(user);
}

