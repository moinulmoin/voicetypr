import { describe, it, expect } from 'vitest';

describe('Basic test', () => {
  it('should pass', () => {
    expect(true).toBe(true);
  });

  it('should do math', () => {
    expect(1 + 1).toBe(2);
  });
});