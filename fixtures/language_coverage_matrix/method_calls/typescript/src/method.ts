class Service { save() { return 1; } }
export function run(s: Service) { return s.save(); }
