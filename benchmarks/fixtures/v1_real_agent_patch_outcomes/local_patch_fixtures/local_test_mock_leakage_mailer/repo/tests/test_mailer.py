from src.mailer import send_receipt_email


def test_receipt_subject():
    message = send_receipt_email({"email": "a@example.test"}, {"total": 5})
    assert message["subject"] == "Your receipt"
