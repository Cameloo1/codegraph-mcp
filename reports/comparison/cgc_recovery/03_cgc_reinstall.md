# 03 CGC Reinstall

Generated: 2026-05-13 08:51:00 -05:00

Result: stock package reinstalled in a diagnostic compatibility venv.

- Mode: stock_reinstall_compat_dependency
- Package: codegraphcontext==0.4.7
- Parser compatibility: tree-sitter-language-pack==1.6.2
- CGC algorithm/retrieval semantics patched: false
- Smoke after reinstall: True

This is not a semantic fork of CGC. The change is environment recovery: a disposable venv and compatible parser package so stock codegraphcontext 0.4.7 can start on this Windows machine.