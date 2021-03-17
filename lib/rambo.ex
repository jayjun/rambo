defmodule Rambo do
  @moduledoc File.read!("#{__DIR__}/../README.md")
             |> String.split("\n")
             |> Enum.drop(2)
             |> Enum.join("\n")

  defstruct status: nil, out: "", err: ""

  @type t :: %__MODULE__{
          status: integer(),
          out: String.t(),
          err: String.t()
        }
  @type args :: String.t() | [iodata()] | nil
  @type result :: {:ok, t()} | {:error, t() | String.t()}

  alias __MODULE__

  @doc """
  Stop by killing your command.

  Pass the `pid` of the process that called `run/1`. That process will return
  with `{:killed, %Rambo{}}` with results accumulated thus far.

  ## Example

      iex> task = Task.async(fn ->
      ...>   Rambo.run("cat")
      ...> end)
      iex> Rambo.kill(task.pid)
      iex> Task.await(task)
      {:killed, %Rambo{status: nil}}

  """
  @spec kill(pid()) :: {:killed, t()}
  def kill(pid) do
    send(pid, :kill)
  end

  @doc ~S"""
  Runs `command`.

  Executes the `command` and returns `{:ok, %Rambo{}}` or `{:error, reason}`.
  `reason` is a string if the child process failed to start, or a `%Rambo{}`
  struct if the child process started successfully but exited with a non-zero
  status.

  Multiple calls can be chained together with the `|>` pipe operator to
  simulate Unix pipes.

      Rambo.run("ls") |> Rambo.run("sort") |> Rambo.run("head")

  If any command did not exit with `0`, the rest will not be executed and the
  last executed result is returned in an `:error` tuple.

  See `run/2` or `run/3` to pass arguments or options.

  ## Examples

      iex> Rambo.run("echo")
      {:ok, %Rambo{out: "\n", status: 0, err: ""}}

  """
  @spec run(command :: String.t() | result()) :: result()
  def run(command) do
    run(command, nil, [])
  end

  @doc ~S"""
  Runs `command` with arguments or options.

  Arguments can be a string or list of strings. See `run/3` for options.

  ## Examples

      iex> Rambo.run("echo", "john")
      {:ok, %Rambo{out: "john\n", status: 0}}

      iex> Rambo.run("echo", ["-n", "john"])
      {:ok, %Rambo{out: "john", status: 0}}

      iex> Rambo.run("cat", in: "john")
      {:ok, %Rambo{out: "john", status: 0}}

  """
  @spec run(command :: String.t() | result(), args_or_opts :: args() | Keyword.t()) :: result()
  def run(command, args_or_opts) do
    case command do
      {:ok, %{status: 0, out: out}} ->
        command = args_or_opts
        run(command, in: out)

      {:error, reason} ->
        {:error, reason}

      command ->
        if Keyword.keyword?(args_or_opts) do
          run(command, nil, args_or_opts)
        else
          run(command, args_or_opts, [])
        end
    end
  end

  @doc ~S"""
  Runs `command` with arguments and options.

  ## Options

    * `:in` - pipe iodata as standard input
    * `:cd` - the directory to run the command in
    * `:env` - map or list of tuples containing environment key-value as strings
    * `:log` - stream standard output or standard error to console or a
    function. May be `:stdout`, `:stderr`, `true` for both, `false` for
    neither, or a function with one arity. If a function is given, it will be
    passed `{:stdout, output}` or `{:stderr, error}` tuples. Defaults to
    `:stderr`.
    * `:timeout` - kills command after timeout in milliseconds. Defaults to no
    timeout.

  ## Examples

      iex> Rambo.run("/bin/sh", ["-c", "echo $JOHN"], env: %{"JOHN" => "rambo"})
      {:ok, %Rambo{out: "rambo\n", status: 0}}

      iex> Rambo.run("echo", "rambo", log: &IO.inspect/1)
      {:ok, %Rambo{out: "rambo\n", status: 0}}

  """
  @spec run(command :: String.t() | result(), args :: args(), opts :: Keyword.t()) :: result()
  def run(command, args, opts) do
    case command do
      {:ok, %{out: out}} ->
        command = args
        args_or_opts = opts

        if Keyword.keyword?(args_or_opts) do
          run(command, nil, [in: out] ++ args_or_opts)
        else
          run(command, args_or_opts, in: out)
        end

      {:error, reason} ->
        {:error, reason}

      command when byte_size(command) > 0 ->
        {stdin, opts} = Keyword.pop(opts, :in)
        {envs, opts} = Keyword.pop(opts, :env)
        {current_dir, opts} = Keyword.pop(opts, :cd)
        {log, opts} = Keyword.pop(opts, :log, :stderr)
        {timeout, _opts} = Keyword.pop(opts, :timeout)

        log =
          case log do
            log when is_function(log) -> log
            true -> [:stdout, :stderr]
            log -> [log]
          end

        rambo = Mix.Tasks.Compile.Rambo.find_rambo()
        port = Port.open({:spawn, rambo}, [:binary, :exit_status, {:packet, 4}])
        send_command(port, command)

        if args, do: send_arguments(port, args)
        if stdin, do: send_stdin(port, stdin)
        if envs, do: send_envs(port, envs)
        if current_dir, do: send_current_dir(port, current_dir)

        if is_integer(timeout) do
          Process.send_after(self(), :kill, timeout)
        end

        run_command(port)

        port
        |> receive_result(%Rambo{}, log)
        |> output_to_binary()

      command ->
        raise ArgumentError, message: "invalid command '#{inspect(command)}'"
    end
  end

  @doc false
  @spec run(result :: result(), command :: String.t(), args :: args(), opts :: Keyword.t()) ::
          result()
  def run(result, command, args, opts) do
    case result do
      {:ok, %{out: out}} -> run(command, args, [in: out] ++ opts)
      {:error, reason} -> {:error, reason}
    end
  end

  @messages [
    :command,
    :arg,
    :stdin,
    :env,
    :current_dir,
    :eot,
    :error,
    :stdout,
    :stderr,
    :exit_status
  ]

  for {message, index} <- Enum.with_index(@messages) do
    Module.put_attribute(__MODULE__, message, <<index>>)
  end

  defp send_command(port, command) do
    Port.command(port, [@command, command])
  end

  defp send_arguments(port, args) when is_list(args) do
    for arg <- args do
      send_arguments(port, arg)
    end
  end

  defp send_arguments(port, arg) when is_binary(arg) do
    Port.command(port, [@arg, arg])
  end

  defp send_stdin(port, stdin) do
    Port.command(port, [@stdin, stdin])
  end

  defp send_envs(port, envs) do
    for {name, value} <- envs do
      Port.command(port, [@env, <<byte_size(name)::32>>, name, value])
    end
  end

  defp send_current_dir(port, current_dir) do
    Port.command(port, [@current_dir, current_dir])
  end

  defp run_command(port) do
    Port.command(port, @eot)
  end

  defp receive_result(port, result, log) do
    receive do
      {^port, {:data, @error <> message}} ->
        Port.close(port)
        {:error, message}

      {^port, {:data, @stdout <> stdout}} ->
        maybe_log(:stdout, stdout, log)
        result = Map.update(result, :out, [], &[&1 | stdout])
        receive_result(port, result, log)

      {^port, {:data, @stderr <> stderr}} ->
        maybe_log(:stderr, stderr, log)
        result = Map.update(result, :err, [], &[&1 | stderr])
        receive_result(port, result, log)

      {^port, {:data, @exit_status <> <<exit_status::32>>}} ->
        result = Map.put(result, :status, exit_status)
        receive_result(port, result, log)

      {^port, {:data, @eot}} ->
        Port.close(port)

        if result.status == 0 do
          {:ok, result}
        else
          {:error, result}
        end

      {^port, {:exit_status, exit_status}} ->
        {:error, "rambo exited with #{exit_status}"}

      :kill ->
        Port.close(port)
        {:killed, result}
    end
  end

  defp maybe_log(to, output, log) when is_function(log) do
    log.({to, output})
  end

  defp maybe_log(to, output, log) do
    if to in log do
      device =
        case to do
          :stdout -> :stdio
          :stderr -> :stderr
        end

      IO.binwrite(device, output)
    end
  end

  defp output_to_binary({reason, %Rambo{out: out, err: err} = result}) do
    {reason, %{result | out: to_binary(out), err: to_binary(err)}}
  end

  defp output_to_binary(result) do
    result
  end

  defp to_binary(iodata) when is_list(iodata) do
    IO.iodata_to_binary(iodata)
  end

  defp to_binary(output) do
    output
  end
end
