---
parts:
  - list
  - add
  - change
  - remove
---


<!-- Generated from doc comments in Rust. Do not edit. -->

# `members`: Manage org members

## Usage

```sh
stencila orgs members [options] <subcommand>
```



## Subcommands

| Name | Description |
| --- | --- |
| [`list`](list.md) | List members of a org |
| [`add`](add.md) | Add a user as a member of a organization |
| [`change`](change.md) | Change the role of a user within an organization |
| [`remove`](remove.md) | Remove a user as a member of an organization |
| `help` | Print help information |



## Global options

| Name | Description |
| --- | --- |
| `--help` | Print help information. |
| `--version` | Print version information. |
| `--as <format>` | Format to display output values (if possible). |
| `--json` | Display output values as JSON (alias for `--as json`). |
| `--yaml` | Display output values as YAML (alias for `--as yaml`). |
| `--md` | Display output values as Markdown if possible (alias for `--as md`). |
| `--interact -i` | Enter interactive mode (with any command and options as the prefix). |
| `--debug` | Print debug level log events and additional diagnostics. Equivalent to setting `--log-level=debug` and `--log-format=detail` and overrides the both. |
| `--log-level <log-level>` | The minimum log level to print. One of: `trace`, `debug`, `info`, `warn`, `error`, `never` |
| `--log-format <log-format>` | The format to print log events. One of: `simple`, `detail`, `json` |