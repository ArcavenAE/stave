# Homebrew formula for stave
# Single channel, kos pattern: updated by CI on every push to main
# (alpha-... versions) and on v* tags (stable versions). The same
# formula tracks the latest published artifact, alpha or stable.
# macOS only (arm64). Linux support is a future option.

class Stave < Formula
  desc "Spec-driven CLI for the iru (Kandji) Endpoint Management API"
  homepage "https://github.com/ArcavenAE/stave"
  version "VERSION_PLACEHOLDER"
  license "MIT"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/ArcavenAE/stave/releases/download/TAG_PLACEHOLDER/stave-darwin-arm64"
    sha256 "SHA256_DARWIN_ARM64_PLACEHOLDER"
  else
    odie "stave currently only supports macOS arm64. Build from source for other platforms."
  end

  def install
    bin.install "stave-darwin-arm64" => "stave"
  end

  def caveats
    <<~EOS
      stave updates on every push to main (alpha versions) until the
      first stable tag is cut.

      Bootstrap credentials (recommended — token goes to macOS Keychain):

        echo "$IRU_API_TOKEN" | stave auth login --stdin --subdomain <tenant>
        stave auth status

      Discover and invoke operations:

        stave ops list
        stave api <operationId> --param key=value
        stave list device --limit 5

      stave is read-only against your tenant by default; mutating
      operations require --allow-write.

      Every API call writes a structured JSONL audit line under
      ~/.stave/audit/ (macOS) or ~/.local/state/stave/audit/ (Linux).
    EOS
  end

  test do
    assert_match "stave", shell_output("#{bin}/stave --version 2>&1")
  end
end
