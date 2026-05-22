import unittest
from pathlib import Path

from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.paths import (
    config_path,
    repo_root,
    resolve_benchmark_path,
    workspace_path,
)


class BenchmarkPathResolverTests(unittest.TestCase):
    def test_legacy_config_alias_loads_canonical_config(self):
        legacy = load_config("benchmarks/configs/internal_gold_smoke.toml")
        canonical = load_config(config_path("internal_gold", "smoke.toml"))
        self.assertEqual(validate_config(legacy), [])
        self.assertEqual(legacy.dataset_path, canonical.dataset_path)
        self.assertEqual(legacy.workspace_dir, canonical.workspace_dir)

    def test_moved_dataset_path_resolves_to_canonical_location(self):
        resolved = resolve_benchmark_path("benchmarks/datasets/internal_gold")
        self.assertEqual(resolved, Path("benchmarks/tracks/internal_gold/datasets/internal_gold"))

    def test_track_workspace_helper_uses_canonical_track_root(self):
        self.assertEqual(
            workspace_path("repobench", "probe"),
            repo_root() / Path("benchmarks/tracks/repobench/workspaces/probe"),
        )


if __name__ == "__main__":
    unittest.main()
