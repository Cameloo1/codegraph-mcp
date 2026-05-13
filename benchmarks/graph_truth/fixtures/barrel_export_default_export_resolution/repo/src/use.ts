import { defaultFeature, namedFeature } from "./index";

export function runDefault() {
  return defaultFeature();
}

export function runNamed() {
  return namedFeature();
}
