import { normalizeEmail } from "./email";
import type { User } from "./types";

export { normalizeEmail };

export function formatUser(user: User): string {
  return normalizeEmail(user.email);
}
