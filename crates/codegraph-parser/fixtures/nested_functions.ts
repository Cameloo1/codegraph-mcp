export function outer(input: string): string {
  function inner(value: string): string {
    const trimmed = value.trim();
    return trimmed;
  }

  return inner(input);
}
