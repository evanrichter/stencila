<!-- Generated from doc comments in Rust. Do not edit. -->

# `usage`: View an organization's resource usage against quotas

## Usage

```sh
stencila orgs usage [options]
```

Use this command to quickly see the usage of various resources (e.g. compute time) against the quota for an organization.



## Options

| Name | Description |
| --- | --- |
| `--org -o <org>` | The id of the org. If this option is not supplied, Stencila will use your default organization. |
| `--web -w` | Open the corresponding web page on Stencila in your browser. Use this option when you want to quickly jump to the web page on Stencila that offers the same, or similar, functionality to this command. |

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