# PythTIG Session Manager

The Phase 7 session manager consumes one typed `PythCommand` per invocation and
returns. A supervisor is responsible for relaunching the graph for subsequent
commands.

This program uses the typed command capability surface only:

- `command.kind(commands)` projects the command kind from `CommandRead`.
- `command.text(commands)` projects the bounded UTF-8 payload/result text.
- `command.result_emit(commands, status, text)` emits a typed command result.

Human command parsing remains outside PythCore. PythCore receives only typed
command fields and capability-gated graph operations.
