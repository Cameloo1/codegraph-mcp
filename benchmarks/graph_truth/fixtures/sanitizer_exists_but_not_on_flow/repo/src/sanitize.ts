export function sanitizeHtml(value: string) {
  return value.replace(/</g, "&lt;");
}
