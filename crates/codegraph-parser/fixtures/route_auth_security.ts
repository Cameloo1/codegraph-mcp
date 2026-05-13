import express from "express";
import cors from "cors";
import DOMPurify from "dompurify";

const app = express();

function authorize(policy: string) {
  return policy;
}

function requireRole(role: string) {
  return role;
}

function checkPermission(permission: string) {
  return permission;
}

function validateEmail(value: string) {
  return value.includes("@");
}

function sanitize(value: string) {
  return DOMPurify.sanitize(value);
}

app.use(cors());

app.get("/admin", authorize("admin-policy"), requireRole("admin"), (req, res) => {
  const email = sanitize(req.body.email);
  validateEmail(email);
  checkPermission("users:write");
  res.json({ email });
});
