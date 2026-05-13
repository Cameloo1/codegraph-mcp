export function cgc_smoke_target(value: number): number {
  return value + 1;
}

export function cgc_smoke_caller(input: number): number {
  return cgc_smoke_target(input);
}