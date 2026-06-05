from src.feature_flags import enable_new_checkout


def test_new_checkout_flag():
    assert enable_new_checkout({"new_checkout": True})
