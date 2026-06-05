from src.payments.discounts import checkout_total


def test_checkout_discount_cap():
    assert checkout_total(10000, 70) == 6000
