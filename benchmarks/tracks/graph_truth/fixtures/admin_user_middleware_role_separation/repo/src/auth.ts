export function requireAdmin(user: any) {
  return checkRole(user, "admin");
}

export function requireUser(user: any) {
  return checkRole(user, "user");
}

export function checkRole(user: any, role: "admin" | "user") {
  return user.roles.includes(role);
}
