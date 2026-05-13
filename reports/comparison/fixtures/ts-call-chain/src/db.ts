export function writeDatabase(table: string, value: any) {
  return { table, value, written: true };
}