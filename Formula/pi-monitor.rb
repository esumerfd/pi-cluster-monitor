# This file is updated automatically by the release workflow.
class PiMonitor < Formula
  desc "TUI for monitoring a Raspberry Pi cluster"
  homepage "https://github.com/esumerfd/pi-cluster-monitor"
  version "0.0.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.0.0/pi-monitor-v0.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.0.0/pi-monitor-v0.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.0.0/pi-monitor-v0.0.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  end

  def install
    bin.install "pi-monitor"
  end

  test do
    system "#{bin}/pi-monitor", "--version"
  end
end
