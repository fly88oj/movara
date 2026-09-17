# Homebrew formula template for movara.
#
# To serve this via a tap, create a repo named `homebrew-tap` under your
# GitHub account, copy this file there as Formula/movara.rb, and replace
# the VERSION/SHA placeholders (see packaging/README note in the main
# README "Install" section). Users then install with:
#
#   brew tap <user>/tap
#   brew install movara
#
# CI alternative: replace the url with the GitHub Release tarball of a
# tagged version and fill `sha256` from `shasum -a 256 <tarball>`.
class Movara < Formula
  desc "Move a project directory and rewrite every AI coding agent's local session/config references to it, with backup and undo"
  homepage "https://github.com/fly88oj/movara"
  url "https://github.com/fly88oj/movara/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT OR Apache-2.0"

  livecheck do
    url :stable
    strategy :github_latest
  end

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/movara --version")
  end
end
