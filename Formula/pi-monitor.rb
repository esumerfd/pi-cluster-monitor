# This file is updated automatically by the release workflow.
class PiMonitor < Formula
  desc "TUI for monitoring a Raspberry Pi cluster"
  homepage "https://github.com/esumerfd/pi-cluster-monitor"
  version "0.4.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.4.0/pi-monitor-v0.4.0-aarch64-apple-darwin.tar.gz"
      sha256 "2a1acc98fa9ece1e67c0fd029c3b94c35e963d2907425fb47dd35a6b6b0a448b"
    end
    on_intel do
      url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.4.0/pi-monitor-v0.4.0-x86_64-apple-darwin.tar.gz"
      sha256 "5da92966bb2819e41a89680220437d5598418e92cdae18d9a3825feae72cf0f2"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/pi-cluster-monitor/releases/download/v0.4.0/pi-monitor-v0.4.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "9bec624f436c839d683f28a14df8495fd8338b465e398858c2fb7e0254206d46"
  end

  def install
    bin.install "pi-monitor"
  end

  test do
    system "#{bin}/pi-monitor", "--version"
  end
end
