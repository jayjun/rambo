defmodule Mix.Tasks.Compile.Rambo do
  @moduledoc false
  use Mix.Task.Compiler

  # rust targets
  @mac "x86_64-apple-darwin"
  @linux "x86_64-unknown-linux-musl"
  @windows "x86_64-pc-windows-gnu"

  @filenames %{
    @mac => "rambo-mac",
    @linux => "rambo-linux",
    @windows => "rambo.exe",
    :custom => "rambo"
  }

  @environment List.to_string(:erlang.system_info(:system_architecture))

  filename =
    cond do
      String.starts_with?(@environment, "x86_64-apple-darwin") ->
        @filenames[@mac]

      String.starts_with?(@environment, "x86_64") and String.contains?(@environment, "linux") ->
        @filenames[@linux]

      @environment == "win32" ->
        @filenames[@windows]

      true ->
        @filenames.custom
    end

  @filename filename
  def find_rambo do
    Path.join(:code.priv_dir(:rambo), @filename)
  end

  def run(["all"]) do
    with :ok <- compile(@mac),
         :ok <- compile_in_docker(@linux),
         :ok <- compile_in_docker(@windows) do
      :ok
    else
      error -> error
    end
  end

  def run([]) do
    executable = find_rambo()
    unless File.exists?(executable), do: compile!()

    if Application.get_env(:rambo, :purge, false) do
      remove_unused_binaries(executable)
    end

    :ok
  end

  def run(platforms) do
    for platform <- platforms do
      case platform do
        "mac" -> compile(@mac)
        "linux" -> compile(@linux)
        "windows" -> compile(@windows)
        _ -> :ok
      end
    end

    :ok
  end

  @compile_priv_dir "#{:code.priv_dir(:rambo)}"

  defp remove_unused_binaries(executable) do
    filename = Path.basename(executable)

    @filenames
    |> Enum.map(fn {_target, filename} -> filename end)
    |> Enum.reject(&(&1 == filename))
    |> Enum.map(&Path.join(@compile_priv_dir, &1))
    |> Enum.each(&File.rm/1)
  end

  defp compile! do
    if System.find_executable("cargo") do
      compile(:custom)
    else
      raise """
      Rambo does not ship with binaries for your environment.

          #{@environment} detected

      Install the Rust compiler so a binary can be prepared for you.
      """
    end
  end

  defp compile(target) do
    args =
      case target do
        :custom -> ["build", "--release"]
        target -> ["build", "--release", "--target", target]
      end

    case System.cmd("cargo", args, cd: @compile_priv_dir) do
      {_output, 0} ->
        move_executable(target)
        :ok

      {output, _exit_status} ->
        {:error, [output]}
    end
  end

  defp compile_in_docker(target) do
    tag = "rambo/" <> target
    build = ["build", "--tag", tag, "--file", "Dockerfile." <> target, "."]

    with {_output, 0} <- System.cmd("docker", build, cd: @compile_priv_dir),
         {_output, 0} <-
           System.cmd("docker", ["run", "--volume", @compile_priv_dir <> ":/app", tag]) do
      move_executable(target)
      :ok
    else
      {output, _exit_status} -> {:error, [output]}
    end
  end

  defp move_executable(target) do
    target_executable =
      case target do
        :custom -> "target/release/rambo"
        @windows -> "target/#{@windows}/release/rambo.exe"
        target -> "target/#{target}/release/rambo"
      end

    source = Path.join(@compile_priv_dir, target_executable)
    destination = Path.join(@compile_priv_dir, @filenames[target])
    File.rename!(source, destination)
  end
end
