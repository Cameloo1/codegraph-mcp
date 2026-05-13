import { calculateTotal } from "../src/price";

test("calculateTotal adds tax", () => {
  expect(calculateTotal(10, 2)).toBe(12);
});