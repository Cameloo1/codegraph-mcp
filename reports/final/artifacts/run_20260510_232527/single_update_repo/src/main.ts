import { chooseUser } from "./a";
import { audit } from "./audit";

export function handler(id: string) {
  audit(id);
  return chooseUser(id);
}

export const finalGateTouch = 1;
