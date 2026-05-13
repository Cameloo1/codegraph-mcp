export class Counter {
  constructor(private value: number) {}

  increment(step: number): number {
    const next = this.value + step;
    this.value = next;
    return this.value;
  }
}
