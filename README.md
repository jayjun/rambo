# Rambo

Ever stumped trying to get output from a port?

Rambo has one mission. Start your program, pipe standard input,
**send EOF** and return with output.

```elixir
Rambo.run("cat", in: "hello")
{:ok, %Rambo{out: "hello"}}

Rambo.run("echo", ["-n", "world"])
{:ok, %Rambo{out: "world"}}

Rambo.run("ls") |> Rambo.run("sort") |> Rambo.run("head")
{:ok, %Rambo{out: "bar\nbaz\nfoo\n"}}
```

Kill your command if it’s stuck and Rambo returns any gathered results.

```elixir
task = Task(fn ->
  Rambo.run("cat")
end)

Rambo.kill(task.pid)

Task.await(task)
{:killed, %Rambo{out: "", status: nil}}
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
only). For other environments, install the Rust compiler or Rambo will not
compile.

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
