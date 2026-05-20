export interface Principal {
  role: "admin" | "user" | "guest";
}

export function check_admin(principal: Principal): boolean {
  return principal.role === "admin";
}

export function check_user(principal: Principal): boolean {
  return principal.role === "user";
}

export function not_authorized(principal: Principal): boolean {
  return !check_admin(principal) && !check_user(principal);
}

export function no_admin_required(principal: Principal): boolean {
  return check_user(principal) || principal.role === "guest";
}

