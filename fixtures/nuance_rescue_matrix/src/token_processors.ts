export function processUserToken(token: string): string {
  return `user:${token.trim()}`;
}

export function processUserTokens(tokens: string[]): string[] {
  return tokens.map(processUserToken);
}

export function processAdminToken(token: string): string {
  return `admin:${token.trim()}`;
}

