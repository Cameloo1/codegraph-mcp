export function authorizeRequest(req: any) {
  if (!req.user) throw new Error("unauthorized");
}

export function checkRole(user: any, role: string) {
  if (user.role !== role) throw new Error("forbidden");
}