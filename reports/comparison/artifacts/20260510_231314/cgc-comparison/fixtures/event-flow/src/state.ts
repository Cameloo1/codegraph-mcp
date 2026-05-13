const state: Record<string, boolean> = {};

export function markUserActive(id: string) {
  state[id] = true;
}