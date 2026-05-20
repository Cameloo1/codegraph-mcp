export function register(items: string[]) { for (var i = 0; i < items.length; i++) { setTimeout(() => console.log(items[i]), 1); } }
