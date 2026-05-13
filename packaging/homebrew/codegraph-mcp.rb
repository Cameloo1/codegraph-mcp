class CodegraphMcp < Formula
  desc "Local proof-oriented code graph memory layer for Codex-style agents"
  homepage "https://github.com/example/codegraph-mcp"
  version "0.0.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/example/codegraph-mcp/releases/download/v#{version}/codegraph-mcp-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    else
      url "https://github.com/example/codegraph-mcp/releases/download/v#{version}/codegraph-mcp-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  on_linux do
    url "https://github.com/example/codegraph-mcp/releases/download/v#{version}/codegraph-mcp-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "REPLACE_WITH_SHA256"
  end

  def install
    bin.install "codegraph-mcp"
  end

  test do
    assert_match "codegraph-mcp", shell_output("#{bin}/codegraph-mcp --version")
  end
end

