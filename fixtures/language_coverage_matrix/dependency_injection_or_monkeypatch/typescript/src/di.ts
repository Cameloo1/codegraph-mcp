export function run(container: any) { const service = container.resolve('service'); return service.handle(); }
