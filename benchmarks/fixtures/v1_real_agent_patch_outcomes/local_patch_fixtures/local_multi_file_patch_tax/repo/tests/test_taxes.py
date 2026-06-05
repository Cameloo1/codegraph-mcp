from src.invoice import invoice_total


def test_invoice_total_includes_tax():
    assert invoice_total(1000, 8) == 1080
