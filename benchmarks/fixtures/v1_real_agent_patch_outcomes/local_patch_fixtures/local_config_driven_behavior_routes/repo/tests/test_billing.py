from src.billing import apply_credit


def test_credit_accumulates():
    assert apply_credit({"credit": 1}, 2)["credit"] == 3
