# Fixed CGC Fork Inspection

| Field | Value |
|---|---|
| `fork_path` | <CGC_FORK_ROOT> |
| `exists` | True |
| `listing` | `[{"name": ".cgcignore", "kind": "file", "bytes": 590}, {"name": ".codex-tools", "kind": "dir", "bytes": null}, {"name": ".cursor", "kind": "dir", "bytes": null}, {"name": ".dockerignore", "kind": "file", "bytes": 621}, {"name": ".env.example", "kind": "file", "bytes": 1170}, {"name": ".git", "kind": "dir", "bytes": null}, {"name": ".github", "kind": "dir", "bytes": null}, {"name": ".gitignore", "kind": "file", "bytes": 1654}, {"name": ".local-codex-logs", "kind": "dir", "bytes": null}, {"name": ` |
| `git_status_log` | reports/comparison/cgc_full_run/logs/phase1_fork_git_status.stdout.txt |
| `git_remote_log` | reports/comparison/cgc_full_run/logs/phase1_fork_git_remote.stdout.txt |
| `git_commit` | fcf03925f640541eed4a69e085af22a778bdaf30 |
| `pyproject` | `{"exists": true, "package_name": "codegraphcontext", "version": "0.4.8", "requires_python": ">=3.10", "console_scripts": {"cgc": "codegraphcontext.cli.main:app", "codegraphcontext": "codegraphcontext.cli.main:app"}, "dependencies": ["neo4j>=5.15.0", "watchdog>=3.0.0", "stdlibs>=2023.11.18", "typer>=0.9.0", "rich>=13.7.0", "inquirerpy>=0.3.4", "python-dotenv>=1.0.0", "tree-sitter>=0.21.0,<0.26.0; python_version != '3.13'", "tree-sitter-language-pack>=0.6.0,<1.0.0; python_version != '3.13'", "tree` |
| `cli_command_name` | cgc |
| `alternate_cli_command_name` | codegraphcontext |
| `install_method` | pip install -e <fixed fork path> |
| `install_mode_label` | fork_editable |
| `uncertainty` | `[]` |
