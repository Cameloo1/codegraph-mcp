function source() { return 42; }
export function caller() { const value = source(); return value; }
