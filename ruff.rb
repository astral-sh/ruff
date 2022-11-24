class Ruff < Formula
  desc "An extremely fast Python linter, written in Rust."
  homepage "https://github.com/charliermarsh/ruff"
  url "https://github.com/charliermarsh/ruff/archive/refs/tags/v0.0.137.tar.gz"
  sha256 "d5521f2ad9ee87ca8018bd23ea731fc05abd0fa4a01878880a8cc5119596f837"
  license "MIT"
  head "https://github.com/charliermarsh/ruff.git", branch: "main"

  # STOPSHIP(charlie): Requires Rust 1.65.0.
  # See: https://github.com/Homebrew/homebrew-core/pull/116480
  depends_on "rust" => :build

  def install
    system "cargo", "install", "--no-default-features", *std_cargo_args
    bin.install "target/release/ruff" => "ruff"
  end

  test do
    (testpath/"test.py").write <<~EOS
      import os
    EOS
    expected = <<~EOS
      test.py:1:1: F401 `os` imported but unused
    EOS
    assert_equal expected, shell_output("#{bin}/ruff -- --quiet #{testpath}/test.py")
  end
end
