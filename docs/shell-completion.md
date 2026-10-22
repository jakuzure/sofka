# Shell completion

Run `sofka completion <shell>` to print a completion script. Supported shells
are `bash`, `zsh`, `fish`, `elvish`, and `powershell`.

The scripts complete CLI options, subcommands, and fixed option values from the
CLI definitions. They also complete these values when you press Tab:

- `--context`: context names from kubeconfig. This uses `KUBECONFIG`, or
  `~/.kube/config` when the variable is not set. An earlier `--kubeconfig` option
  selects a different file.
- `-n`, `--namespace`: namespaces from the cluster selected by an earlier
  `--context` option, or the current kubeconfig context.
- The resource argument and `--resource`: built-in aliases, configured aliases,
  and resource types from API discovery, including custom resources, short names,
  and names with an API group suffix. `ctx` and `contexts` are also available.
- `--kubeconfig` and `--validate-plugin-report`: local paths.
- `--validate-plugin`: local directories.
- `plugin describe` and `plugin install`: plugin IDs and `ID@VERSION` values from
  the cached catalog. Install suggestions exclude withdrawn releases and releases
  that do not support the current sofka version or platform. Describe suggestions
  include all cached versions. Run `sofka plugin search` to populate or refresh
  that cache.
- `plugin update` and `plugin remove`: installed, managed plugin IDs.

Plugin completion does not download or install packages. Free text, such as a
plugin search query, has no value suggestions. The CLI has no object-name argument,
so resource completion suggests resource types only.

Completion uses arguments before the cursor. Both `--context NAME` and
`--context=NAME` are supported. Cluster queries use the selected kubeconfig and
TLS options. They can run credential helpers configured in kubeconfig, but do
not allow interactive input. Each completion request has a two-second limit;
unavailable data produces no error output. Cluster access must permit namespace
listing or API discovery to complete those values.

Script generation does not load the sofka configuration or connect to a cluster.
The generated scripts call `sofka` when you press Tab. Generate them again after
an update, as shown below.

## Nix package

The Nix package (`nix build`, the flake's overlay, or the Home Manager module)
installs the Bash, Fish and Zsh scripts, so shells that read completions from
installed packages need no setup. Fish reads them from `vendor_completions.d`;
Bash needs bash-completion; Zsh needs the package's `share/zsh/site-functions`
on `fpath` before `compinit` (Home Manager and NixOS set this up). The scripts
call the installed `sofka` when you press Tab, so they stay in sync after an
update. Don't also add the lines below, or the completion is registered twice.
A cross-compiled package has no scripts: set it up as below.

## Other installations

Use the instructions for your shell below. Make sure `sofka` is on `PATH` when
the shell starts. These commands generate the script at shell startup, so
completion stays in sync after a sofka update.

## Bash

Add this line to `~/.bashrc`:

```bash
source <(sofka completion bash)
```

## Zsh

Add these lines to `~/.zshrc`. If your shell setup already runs `compinit`, add
only the `source` line after that setup.

```zsh
autoload -Uz compinit
compinit
source <(sofka completion zsh)
```

## Fish

Add this line to `~/.config/fish/config.fish`:

```fish
sofka completion fish | source
```

## Elvish

Add this line to your Elvish `rc.elv` file:

```elvish
eval (sofka completion elvish | slurp)
```

## PowerShell

Add this line to your PowerShell profile (`$PROFILE`):

```powershell
sofka completion powershell | Out-String | Invoke-Expression
```

Restart your shell after you change its configuration. You can also run the
commands directly to enable completion in the current session.

`completion` is a CLI subcommand. To open a Kubernetes resource with that name,
use `sofka --resource completion`.
