type Handler = (payload: any) => void;
const listeners: Record<string, Handler[]> = {};

export function publishUserCreated(payload: any) {
  emitEvent("user.created", payload);
}

export function emitEvent(name: string, payload: any) {
  for (const handler of listeners[name] || []) handler(payload);
}

export function onEvent(name: string, handler: Handler) {
  listeners[name] = [...(listeners[name] || []), handler];
}