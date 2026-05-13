export interface User {
  id: string;
  email: string;
}

export function normalizeEmail(user: User): string {
  return user.email.trim().toLowerCase();
}

export class AuthService {
  login(user: User): string {
    return normalizeEmail(user);
  }
}
