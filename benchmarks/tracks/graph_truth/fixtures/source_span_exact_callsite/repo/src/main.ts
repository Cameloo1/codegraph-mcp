import { first, second } from "./actions";

export function run(flag: boolean) {
  first();
  if (flag) {
    second();
  }
}
