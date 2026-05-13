import { readFileSync } from "node:fs";

export function readConfig(path) {
  const raw = readFileSync(path, "utf8");
  return JSON.parse(raw);
}

class ConfigCache {
  constructor() {
    this.items = new Map();
  }
}
