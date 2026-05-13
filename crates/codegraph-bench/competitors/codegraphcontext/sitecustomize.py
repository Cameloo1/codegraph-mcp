"""Benchmark-only Python startup hook for the external CGC CLI.

This file is only used when the CGC comparison runner adds this directory to
PYTHONPATH. It keeps CodeGraphContext's tree-sitter-language-pack cache inside a
declared local cache path so benchmark execution can stay offline.
"""

from __future__ import annotations

import os


def _configure_tree_sitter_cache() -> None:
    cache_dir = os.environ.get("TREE_SITTER_LANGUAGE_PACK_CACHE_DIR")
    if not cache_dir:
        return
    try:
        import tree_sitter_language_pack

        tree_sitter_language_pack.configure(cache_dir=cache_dir)
    except Exception:
        # CGC owns parser error reporting. This hook only supplies the cache
        # location when the optional dependency is importable.
        return


_configure_tree_sitter_cache()
