export function write(obj: Record<string, string>, key: string, value: string) { obj[key] = value; return obj[key]; }
