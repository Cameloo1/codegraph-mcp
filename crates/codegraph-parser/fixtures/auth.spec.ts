import { describe, it, expect, vi, beforeEach } from "vitest";
import { login } from "./auth";

vi.mock("./client");

beforeEach(() => setupUserFixture());

describe("login", () => {
  it("returns token", () => {
    const client = vi.fn();
    vi.spyOn(client, "post");
    vi.stubEnv("TOKEN_SECRET", "test");
    const result = login("admin@example.com");
    expect(result.token).toBeDefined();
  });
});
