defmodule Rambo.MixProject do
  use Mix.Project

  @version "0.3.4"
  @repo_url "https://github.com/jayjun/rambo"

  def project do
    [
      app: :rambo,
      version: @version,
      elixir: "~> 1.9",
      name: "Rambo",
      description: "Run your command. Send input. Get output.",
      compilers: Mix.compilers() ++ [:rambo],
      deps: deps(),
      package: [
        exclude_patterns: ["priv/target"],
        licenses: ["MIT"],
        links: %{"GitHub" => @repo_url}
      ],
      docs: [
        source_ref: @version,
        source_url: @repo_url,
        main: "Rambo",
        api_reference: false,
        extra_section: []
      ]
    ]
  end

  def application do
    [
      extra_applications: []
    ]
  end

  defp deps do
    [
      {:ex_doc, "~> 0.24", only: [:docs], runtime: false}
    ]
  end
end
