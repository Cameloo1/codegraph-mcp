export function helper(value: string): string {
  return value;
}

export function demo(input: string): string {
  const a = input;
  let b = "";
  b = a;
  helper(b);
  return b;
}
