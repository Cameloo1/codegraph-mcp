function handleOrder(order: unknown) { return order; }
export function wire(bus: any) { bus.on('order.created', handleOrder); bus.emit('order.created', {}); }
