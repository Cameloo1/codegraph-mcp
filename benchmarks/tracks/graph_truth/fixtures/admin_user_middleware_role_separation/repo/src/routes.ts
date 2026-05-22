import { requireAdmin, requireUser } from "./auth";

export const adminRoute = route("GET", "/admin", requireAdmin, adminPanel);
export const userRoute = route("GET", "/account", requireUser, userHome);

function route(method: string, path: string, guard: Function, handler: Function) {
  return { method, path, guard, handler };
}
function adminPanel() { return "admin"; }
function userHome() { return "user"; }
