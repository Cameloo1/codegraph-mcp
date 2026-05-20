// sanitize(userInput) is only a comment and must not prove sanitization.
const label = "sanitize(userInput)";
function sanitize(value: string) { return value.trim(); }
export function run(userInput: string) { return sanitize(userInput); }
