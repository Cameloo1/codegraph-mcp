def apply_credit(invoice, amount):
    invoice["credit"] = invoice.get("credit", 0) + amount
    return invoice
