import { chargeCard } from "./service";

export function checkout(total: number) {
  return chargeCard(total);
}
