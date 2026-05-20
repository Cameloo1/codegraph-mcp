export function duplicate(value: number) { return value + 1; }
export function caller() { return duplicate(1); }
export class Box {
  duplicate() { return 2; }
  call() { return this.duplicate(); }
}
