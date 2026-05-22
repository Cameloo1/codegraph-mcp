import { describe, expect, it, vi } from "vitest";
import { checkout } from "../src/checkout";
import { chargeCard } from "../src/service";

vi.mock("../src/service", () => ({
  chargeCard: vi.fn(() => "mocked")
}));

describe("checkout", () => {
  it("uses a test double for chargeCard", () => {
    expect(checkout(5)).toBe("mocked");
  });
});
