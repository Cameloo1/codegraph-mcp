def calculate_discount_report(rows):
    return [{"label": row["name"], "discount": row["discount"]} for row in rows]
