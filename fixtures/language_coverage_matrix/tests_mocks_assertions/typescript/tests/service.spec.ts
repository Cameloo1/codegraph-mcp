import { describe, it, expect, vi } from 'vitest';
function subject() { return 1; }
describe('subject', () => { beforeEach(() => vi.clearAllMocks()); it('works', () => { vi.mock('./net'); expect(subject()).toBe(1); }); });
