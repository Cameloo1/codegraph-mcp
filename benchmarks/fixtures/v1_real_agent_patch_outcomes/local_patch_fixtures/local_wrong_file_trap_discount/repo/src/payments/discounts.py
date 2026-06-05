def calculate_checkout_discount(subtotal_cents, coupon_percent):
    capped_percent = min(max(coupon_percent, 0), 40)
    return subtotal_cents * capped_percent // 100


def checkout_total(subtotal_cents, coupon_percent):
    return subtotal_cents - calculate_checkout_discount(subtotal_cents, coupon_percent)
