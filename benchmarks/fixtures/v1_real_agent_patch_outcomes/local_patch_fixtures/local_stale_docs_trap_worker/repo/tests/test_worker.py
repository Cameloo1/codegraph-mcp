from src.worker import launch_worker


def test_worker_default_retries():
    assert launch_worker({})["retries"] == 3
