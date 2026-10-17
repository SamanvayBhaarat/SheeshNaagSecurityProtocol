// Shared fixture for the Sheshnag acceptance tests.
//
// Every test runs the real Src/sheshnag.sh under /bin/bash in a private sandbox:
//   * HOME points at a fresh temp directory.
//   * SHESHNAG_AGENT_DIRS points at sandbox folders, so no real LaunchAgent is read.
//   * launchctl, osascript, crontab, ps and pkill are replaced by recording stubs
//     on PATH, so nothing touches the machine running the tests.
// Set SHESHNAG_BIN to the Rust CLI to run the same tests against it.
// This keeps the tests hermetic, fast and deterministic (no sleeps, no network).
#ifndef SHESHNAG_QA_TEST_SUPPORT_H_
#define SHESHNAG_QA_TEST_SUPPORT_H_

#include <string>
#include <vector>

#include <gtest/gtest.h>

namespace sheshnag::qa {

struct RunResult {
  int exit_code = -1;
  std::string output;  // stdout and stderr, interleaved
};

class SheshnagTest : public ::testing::Test {
 protected:
  void SetUp() override;
  void TearDown() override;

  // Runs `bash sheshnag.sh <args>` inside the sandbox.
  RunResult Run(const std::vector<std::string>& args) const;

  // ---- sandbox fixtures -------------------------------------------------
  // Writes <dir>/<label>.plist whose ProgramArguments are `program_args`.
  std::string AddAgent(const std::string& dir, const std::string& label,
                       const std::vector<std::string>& program_args = {"/usr/bin/true"});
  std::string AddUserAgent(const std::string& label,
                           const std::vector<std::string>& program_args = {"/usr/bin/true"});
  std::string AddSystemAgent(const std::string& label,
                             const std::vector<std::string>& program_args = {"/usr/bin/true"});
  void SetProcesses(const std::vector<std::string>& rows);  // "pid pcpu user comm args"
  void SetCrontab(const std::string& text);
  void SetLoginItems(const std::string& comma_separated);
  std::string CurrentUser() const;

  // ---- file helpers -----------------------------------------------------
  // True when SHESHNAG_BIN selects the Rust CLI instead of the bash script.
  static bool UsingRustEngine();

  static void WriteFile(const std::string& path, const std::string& contents);
  static std::string ReadFile(const std::string& path);
  static bool Exists(const std::string& path);
  static int CountLines(const std::string& text);

  // ---- observations -----------------------------------------------------
  std::string StubCalls() const;      // everything the stubs recorded
  std::string Findings() const;       // ~/.sheshnag/findings.now
  std::string ActivityLog() const;    // ~/.sheshnag/log

  std::string home_;
  std::string user_agents_;
  std::string system_agents_;
  std::string system_daemons_;
  std::string state_;    // ~/.sheshnag
  std::string stubs_;    // PATH stub directory

 private:
  std::string root_;
  std::vector<std::string> Environment() const;
  void InstallStub(const std::string& name, const std::string& body);
};

}  // namespace sheshnag::qa

#endif  // SHESHNAG_QA_TEST_SUPPORT_H_
