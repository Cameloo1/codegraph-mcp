from src.auth.session import refresh_session_token


def test_refresh_bumps_version():
    assert refresh_session_token({"version": 1}, 42)["version"] == 2
