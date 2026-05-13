const emitter = new EventEmitter();
const producer = { publish(topic: string, payload: unknown) {} };
const consumer = {
  consume(topic: string, handler: unknown) {},
  subscribe(topic: string, handler: unknown) {},
};

function handleUserCreated(event: unknown) {
  return event;
}

export async function run(worker: () => Promise<void>) {
  const job = new Promise((resolve) => resolve(undefined));
  emitter.emit("UserCreated", { id: 1 });
  emitter.on("UserCreated", handleUserCreated);
  producer.publish("users.created", { id: 1 });
  consumer.consume("users.created", handleUserCreated);
  consumer.subscribe("users.updated", handleUserCreated);
  setTimeout(() => worker(), 1);
  await worker();
  await Promise.all([worker()]);
  return job;
}
