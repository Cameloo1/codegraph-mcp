import { authorizeRequest, checkRole } from "./security";

export function adminRoute(req: any) {
  authorizeRequest(req);
  checkRole(req.user, "admin");
  return { ok: true };
}