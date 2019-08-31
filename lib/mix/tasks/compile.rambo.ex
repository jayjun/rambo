defmodule Mix.Tasks.Compile.Rambo do
  @moduledoc false
  use Mix.Task.Compiler

  defmacro platform_specific(do: do_block, else: else_block) do
    %{mac: mac, linux: linux, windows: windows} = Map.new(do_block)

    quote do
      case List.to_string(:erlang.system_info(:system_architecture)) do
        "x86_64-apple-darwin" <> _ ->
          unquote(mac)

        "x86_64-" <> system ->
          if String.contains?(system, "linux") do
            unquote(linux)
          else
            unquote(else_block)
          end

        "win32" ->
          unquote(windows)

        _ ->
          unquote(else_block)
      end
    end
  end

  # rust targets
  @mac "x86_64-apple-darwin"
  @linux "x86_64-unknown-linux-musl"
  @windows "x86_64-pc-windows-gnu"

  def executable(@mac), do: "rambo-mac"
  def executable(@linux), do: "rambo-linux"
  def executable(@windows), do: "rambo.exe"
  def executable(_target), do: "rambo"

  def executable do
    platform_specific do
      [mac: executable(@mac), linux: executable(@linux), windows: executable(@windows)]
    else
      executable(:self_compiled)
    end
  end

  def run(["--unix"]) do
    for target <- [@mac, @linux], reduce: :ok do
      :ok -> compile(target)
      error -> error
    end
  end

  def run(["--windows"]) do
    compile(@windows)
  end

  def run([]) do
    compile()
  end

  def compile(target \\ nil) do
    priv_dir = List.to_string(:code.priv_dir(:rambo))

    args =
      if target do
        ["build", "--release", "--target", target]
      else
        ["build", "--release"]
      end

    case System.cmd("cargo", args, cd: priv_dir) do
      {_output, 0} ->
        source =
          if target do
            Path.join(priv_dir, "target/#{target}/release/rambo")
          else
            Path.join(priv_dir, "target/release/rambo")
          end

        destination = Path.join(priv_dir, executable(target))
        File.rename!(source, destination)
        :ok

      {output, _exit_status} ->
        {:error, [output]}
    end
  end
end
