# Rambo

Rambo is the simplest, lightest way to run external programs.

## Usage

```elixir
iex> Rambo.run("echo")
{:ok, %Rambo{status: 0, out: "\n", err: ""}}
```

If the command fails,

```elixir
iex> Rambo.run("printf")
{:error, %Rambo{status: 1, out: "", err: "usage: printf format [arguments ...]\n"}}
```

Send standard input to your command.

```elixir
iex> Rambo.run("cat", in: "rambo")
{:ok, %Rambo{status: 0, out: "rambo", err: ""}}
```

Pass arguments as a string or list of iodata.

```elixir
iex> Rambo.run("ls", "-la")
iex> Rambo.run("ls", ["-l", "-a"])
```

Chain commands together. If one of them fails, the rest won’t be executed and
the failing result is returned.

```elixir
iex> Rambo.run("ls") |> Rambo.run("sort") |> Rambo.run("head")
```

### Logging

By default, Rambo streams standard error to the console as your command runs so
you can spot errors before the command finishes.

Change this behaviour with the `:log` option.

```elixir
iex> Rambo.run("ls", log: :stderr) # default
iex> Rambo.run("ls", log: :stdout) # stream stdout only
iex> Rambo.run("ls", log: true)    # stream both
iex> Rambo.run("ls", log: false)   # don’t log output
```

You can stream logs to any function. It receives `{:stdout, binary}` and
`{:stderr, binary}` tuples whenever output is produced.

```elixir
iex> Rambo.run("echo", log: &IO.inspect/1)
{:stdout, "\n"}
{:ok, %Rambo{status: 0, out: "\n", err: ""}}
```

### Kill

If your command is stuck, kill your command from another process and Rambo
returns with any gathered results so far.

```elixir
iex> task = Task.async(fn ->
...>   Rambo.run("cat")
...> end)

iex> Rambo.kill(task.pid)

iex> Task.await(task)
{:killed, %Rambo{status: nil, out: "", err: ""}}
```

## Why?

Erlang ports do not work with programs that expect EOF to produce output. The
only way to close standard input is to close the port, which also closes
standard output preventing results from coming back to your app. This gotcha
is marked [Won’t Fix](https://bugs.erlang.org/browse/ERL-128).

## Design

When Rambo is asked to run a command, it creates a port to a shim. Then the shim
runs your command, closes standard input and waits for output. After your
command exits, its output is returned to your app before the port is closed and
the shim exits.

```
+-----------------+       stdin
|          +------+------+ --> +---------+
|  Erlang  | Port | Shim |     | Command |
|          +------+------+ <-- +---------+
+-----------------+       stdout
```

If the Erlang node stops during the command, your command is killed and the shim
exits to avoid creating orphans (process leak).

Rambo does not start a pool of processes nor support bidirectional communication
with your commands. It is intentionally kept simple and lightweight to run
transient jobs with minimal overhead, such as calling a Python or Node script to
transform some data. For more complicated use cases, see
[other libraries](#comparisons) below.

## Caveats

You cannot call `Rambo.run` from a GenServer because Rambo uses `receive`, which
interferes with GenServer’s `receive` loop. However, you can wrap the call in a
Task.

```elixir
task = Task.async(fn ->
  Rambo.run("thingamabob")
end)

Task.await(task)
```

## Comparisons

While small and focused, Rambo has some niceties not available elsewhere. You
can [chain commands](#usage) and [easily stream](#logging) your command’s output
to any function.

### System.cmd

If you don’t need to pipe standard input to your external program, just use
[`System.cmd`](https://hexdocs.pm/elixir/System.html#cmd/3).

### Porcelain

[Porcelain](https://github.com/alco/porcelain) cannot send EOF to trigger output
by default. The [Goon](https://github.com/alco/goon) driver must be installed
separately to add this capability. Rambo ships with the required native
binaries.

Goon is written in Go, a multithreaded runtime with a garbage collector. To be
as lightweight as possible, Rambo’s shim is written in Rust. No
garbage collector, no runtime overhead.

Most importantly, Porcelain currently [leaks](https://github.com/alco/porcelain/issues/13)
processes. Writing a new driver to replace Goon should fix it, but Porcelain
appears to be [abandoned](https://github.com/alco/porcelain/issues/50) so effort
went into creating Rambo.

### MuonTrap

[MuonTrap](https://github.com/fhunleth/muontrap) is designed to run long-running
external programs. You can attach the OS process to your supervision tree, and
restart it if it crashes. Likewise if your Elixir process crashes, the OS
process is terminated too.

You can also limit CPU and memory usage on Linux through cgroups.

### erlexec

[erlexec](https://github.com/saleyn/erlexec) is great if you want fine grain
control over external programs.

Each external OS process is mirrored as an Erlang process, so you get
asynchronous and bidirectional communication. You can kill your OS processes
with any signal or monitor them for termination, among many powerful features.

Choose erlexec if you want a kitchen sink solution.

## Installation

Add `rambo` to your list of dependencies in `mix.exs`:

```elixir
def deps do
  [
    {:rambo, "~> 0.2"}
  ]
end
```

This package bundles macOS, Linux and Windows binaries (x86-64 architecture
only). For other environments, install the Rust compiler or Rambo won’t compile.

To remove unused binaries, set `:purge` to `true` in your configuration.

```elixir
config :rambo,
  purge: true
```

## Links

- [Documentation](https://hexdocs.pm/rambo/Rambo.html)
- [Hex](https://hex.pm/packages/rambo)

## License

Rambo is released under [MIT](https://github.com/jayjun/rambo/blob/master/LICENSE.md)
license.
