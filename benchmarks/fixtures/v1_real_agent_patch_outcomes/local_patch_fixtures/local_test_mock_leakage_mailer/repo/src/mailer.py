def send_receipt_email(customer, receipt):
    return {
        "to": customer["email"],
        "subject": "Your receipt",
        "body": f"Receipt total: {receipt['total']}",
    }
