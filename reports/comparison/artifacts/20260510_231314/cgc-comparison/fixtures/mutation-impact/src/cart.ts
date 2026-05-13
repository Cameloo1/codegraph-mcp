import { persistCart } from "./repo";

export const cartState: { total: number } = { total: 0 };

export function addToCart(price: number) {
  cartState.total = cartState.total + price;
  return persistCart(cartState);
}

export function checkout(price: number) {
  return addToCart(price);
}