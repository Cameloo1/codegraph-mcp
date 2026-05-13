import { writeDatabase } from "./db";

export function saveInvoice(record: any) {
  return writeDatabase("invoices", record);
}