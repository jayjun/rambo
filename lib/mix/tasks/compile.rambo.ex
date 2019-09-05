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

  def run(["bundled"]) do
    with :ok <- compile(@mac),
         :ok <- compile_in_docker(@linux),
         :ok <- compile_in_docker(@windows) do
      :ok
    else
      error -> error
    end
  end

  def run(_args) do
    compile_locally()
  end

  defp priv_dir do
    List.to_string(:code.priv_dir(:rambo))
  end

  defp compile(target \\ nil) do
    priv_dir = priv_dir()

    args =
      case target do
        nil -> ["build", "--release"]
        target -> ["build", "--release", "--target", target]
      end

    case System.cmd("cargo", args, cd: priv_dir) do
      {_output, 0} ->
        move_executable(priv_dir, target)
        :ok

      {output, _exit_status} ->
        {:error, [output]}
    end
  end

  defp compile_in_docker(target) do
    priv_dir = priv_dir()
    tag = "rambo/" <> target
    build = ["build", "--tag", tag, "--file", "Dockerfile." <> target, "."]

    with {_output, 0} <- System.cmd("docker", build, cd: priv_dir),
         {_output, 0} <- System.cmd("docker", ["run", "--volume", priv_dir <> ":/app", tag]) do
      move_executable(priv_dir, target)
      :ok
    else
      {output, _exit_status} -> {:error, [output]}
    end
  end

  defp move_executable(priv_dir, target) do
    target_executable =
      case target do
        nil -> "target/release/rambo"
        @windows -> "target/#{@windows}/release/rambo.exe"
        target -> "target/#{target}/release/rambo"
      end

    source = Path.join(priv_dir, target_executable)
    destination = Path.join(priv_dir, executable(target))
    File.rename!(source, destination)
  end
end
