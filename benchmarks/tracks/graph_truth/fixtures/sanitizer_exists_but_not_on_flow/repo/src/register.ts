import { sanitizeHtml } from "./sanitize";

export function saveComment(req: any) {
  const raw = req.body.comment;
  const normalized = raw.trim();
  return writeComment(normalized);
}

export function writeComment(comment: string) {
  return comment;
}
