from src.taxes import calculate_tax


def invoice_total(subtotal_cents, rate_percent):
    return subtotal_cents + calculate_tax(subtotal_cents, rate_percent)
