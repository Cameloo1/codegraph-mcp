import { createInvoice } from "./service";

export function postInvoice(req: any) {
  return createInvoice(req.body);
}