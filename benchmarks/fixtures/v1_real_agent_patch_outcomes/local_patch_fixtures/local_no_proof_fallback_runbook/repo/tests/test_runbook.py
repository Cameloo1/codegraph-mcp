from pathlib import Path


def test_runbook_exists():
    assert Path("docs/runbook.md").exists()
