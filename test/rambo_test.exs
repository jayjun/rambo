defmodule RamboTest do
  use ExUnit.Case
  doctest Rambo

  test "standard out" do
    assert {:ok, %{status: 0, out: "\n", err: ""}} = Rambo.run("echo")
  end

  test "standard input" do
    assert {:ok, %{status: 0, out: "rambo"}} = Rambo.run("cat", in: "rambo")
  end

  test "standard error" do
    assert {:error, %{status: 1, out: ""}} = Rambo.run("printf")
  end

  test "arguments" do
    assert {:ok, %{out: "rambo\n"}} = Rambo.run("echo", "rambo")
    assert {:ok, %{out: "rambo"}} = Rambo.run("echo", ["-n", "rambo"])
  end

  test "environment variables" do
    env = %{"FOO" => "foo"}
    assert {:ok, %{out: "foo\n"}} = Rambo.run("/bin/sh", ["-c", "echo $FOO"], env: env)
  end

  test "change directory" do
    assert {:ok, %{out: "rambo_test.exs\ntest_helper.exs\n"}} = Rambo.run("ls", cd: "test")
  end

  test "piping runs" do
    assert {:ok, %Rambo{}} = result = Rambo.run("echo", "rambo")
    assert {:ok, %{out: "rambo\n"}} = result |> Rambo.run("cat") |> Rambo.run("cat")
  end

  defmodule Bag do
    use Agent

    def start_link do
      Agent.start_link(fn -> [] end, name: __MODULE__)
    end

    def put(log) do
      Agent.update(__MODULE__, &[log | &1])
    end

    def look do
      Agent.get(__MODULE__, &Enum.reverse/1)
    end
  end

  test "redirecting logs" do
    Bag.start_link()
    Rambo.run("echo", ["-n", "rambo"], log: &Bag.put/1)
    Rambo.run("printf", log: &Bag.put/1)
    assert [{:stdout, "rambo"}, {:stderr, _} | _] = Bag.look()
  end
end
