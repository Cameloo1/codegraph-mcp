from src.service.tenant import resolve_tenant_mode


def test_enterprise_threshold():
    assert resolve_tenant_mode({"seats": 25}) == "enterprise"
