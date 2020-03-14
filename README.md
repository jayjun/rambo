# Rambo

> One mission: Run your command. Send input. Get output.

Rambo is the easiest way to run external programs.

Chain commands or capture logs to any function. The shim only runs asynchronous
I/O on a single thread, so it’s very lightweight and efficient.

## Usage

```elixir
iex> Rambo.run("echo")
{:ok, %Rambo{status: 0, out: "\n", err: ""}}

# send standard input
iex> Rambo.run("cat", in: "rambo")

# pass arguments
iex> Rambo.run("ls", ["-l", "-a"])

# chain commands
iex> Rambo.run("ls") |> Rambo.run("sort") |> Rambo.run("head")

# set timeout
iex> Rambo.run("find", "peace", timeout: 1981)
```

### Logging

Logs to standard error are printed by default, so errors are visible before your
command finishes. Change this with the `:log` option.

```elixir
iex> Rambo.run("ls", log: :stderr) # default
iex> Rambo.run("ls", log: :stdout) # log stdout only
iex> Rambo.run("ls", log: true)    # log both stdout and stderr
iex> Rambo.run("ls", log: false)   # don’t log output

# or to any function
iex> Rambo.run("echo", log: &IO.inspect/1)
{:ok, %Rambo{status: 0, out: "\n", err: ""}}
```

### Kill

Kill your command from another process, Rambo returns with any gathered results
so far.

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
standard output, preventing results from coming back to your app. This gotcha
is marked [Won’t Fix](https://bugs.erlang.org/browse/ERL-128).

### Design

When Rambo is asked to run a command, it starts a shim that spawns your command
as a child. After writing to standard input, the file descriptor is closed while
output is streamed back to your app.

```
+-----------------+       stdin
|          +------+------+ --> +---------+
|  Erlang  | Port | Shim |     | Command |
|          +------+------+ <-- +---------+
+-----------------+       stdout
```

If your app exits prematurely, the child is automatically killed to prevent
orphans.

## Caveats

You cannot call `Rambo.run` from a GenServer because Rambo uses `receive`, which
interferes with GenServer’s `receive` loop. However, you can wrap the call in a
Task.

```elixir
task = Task.async(fn ->
  Rambo.run("mission")
end)

Task.await(task)
```

## Comparisons

Rambo does not start a pool of processes nor support bidirectional communication
with your commands. It is intentionally kept simple to run transient jobs with
minimal overhead, such as calling a Python or Node script to transform some
data. For more complicated use cases, see below.

### System.cmd

If you don’t need to pipe standard input to your external program, just use
[`System.cmd`](https://hexdocs.pm/elixir/System.html#cmd/3).

### Porcelain

[Porcelain](https://github.com/alco/porcelain) cannot send EOF to trigger output
by default. The [Goon](https://github.com/alco/goon) driver must be installed
separately to add this capability. Rambo ships with the required native
binaries.

Goon is written in Go, a multithreaded runtime with a garbage collector. To be
as lightweight as possible, Rambo’s shim is written in Rust. Single-threaded, no
garbage collection spikes, no runtime.

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
