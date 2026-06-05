def resolve_tenant_mode(account):
    return "enterprise" if account.get("seats", 0) >= 25 else "standard"
