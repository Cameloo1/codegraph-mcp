import { onEvent } from "./events";
import { markUserActive } from "./state";

export function registerUserConsumer() {
  onEvent("user.created", handleUserCreated);
}

export function handleUserCreated(payload: any) {
  markUserActive(payload.id);
}