export function isRootUid(uid: number): boolean {
  return uid === 0;
}

export function deny(): boolean {
  return false;
}

export function allow(): boolean {
  return true;
}

