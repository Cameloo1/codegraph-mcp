import { saveInvoice } from "./repo";

export function createInvoice(payload: any) {
  const record = { id: payload.id, amount: payload.amount };
  return saveInvoice(record);
}