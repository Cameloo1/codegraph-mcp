import { saveOrder, ordersTable } from "./store";

export function submitOrder(order: any) {
  return saveOrder(order);
}
