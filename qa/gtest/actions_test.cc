// Requirement coverage: FR-S-11 (install / uninstall), FR-S-12 (quarantine),
// FR-S-09 (trust a file), NFR-SAFE-01 (Sakshi never changes the machine on its own).
#include <gmock/gmock.h>

#include "test_support.h"

namespace sheshnag::qa {
namespace {

using ::testing::HasSubstr;
using ::testing::Not;

class InstallTest : public SheshnagTest {};

TEST_F(InstallTest, WritesALaunchAgentThatRunsEveryTenMinutes) {
  const RunResult result = Run({"install"});

  EXPECT_EQ(result.exit_code, 0);
  const std::string plist = ReadFile(user_agents_ + "/in.palanhar.sheshnag.plist");
  EXPECT_THAT(plist, HasSubstr("<integer>600</integer>"));
  EXPECT_THAT(plist, HasSubstr("<string>dekho</string>"));
  EXPECT_THAT(plist, HasSubstr("<string>quiet</string>"));
  EXPECT_THAT(StubCalls(), HasSubstr("launchctl bootstrap"));
}

TEST_F(InstallTest, RecordsItselfInTheBaselineSoItIsNotReportedAsNew) {
  Run({"install"});

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, Not(HasSubstr("NEW")));
}

TEST_F(InstallTest, UninstallRemovesTheAgentAndUnloadsIt) {
  Run({"install"});

  const RunResult result = Run({"uninstall"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_FALSE(Exists(user_agents_ + "/in.palanhar.sheshnag.plist"));
  EXPECT_THAT(StubCalls(), HasSubstr("launchctl bootout"));
}

TEST_F(InstallTest, UninstallWhenNotInstalledStillSucceeds) {
  EXPECT_EQ(Run({"uninstall"}).exit_code, 0);
}

class QuarantineTest : public SheshnagTest {};

TEST_F(QuarantineTest, MovesThePlistToQuarantineAndUnloadsIt) {
  AddUserAgent("com.example.bad");

  const RunResult result = Run({"jaago", "com.example.bad"});

  EXPECT_THAT(result.output, HasSubstr("quarantined"));
  EXPECT_FALSE(Exists(user_agents_ + "/com.example.bad.plist"));
  EXPECT_TRUE(Exists(state_ + "/quarantine/com.example.bad.plist"));
  EXPECT_THAT(StubCalls(), HasSubstr("launchctl bootout"));
}

TEST_F(QuarantineTest, KeepsTheEvidenceFileContentIntact) {
  const std::string path = AddUserAgent("com.example.bad", {"/tmp/payload"});
  const std::string before = ReadFile(path);

  Run({"jaago", "com.example.bad"});

  EXPECT_EQ(ReadFile(state_ + "/quarantine/com.example.bad.plist"), before);
}

TEST_F(QuarantineTest, LeavesOtherAgentsAlone) {
  AddUserAgent("com.example.bad");
  AddUserAgent("com.example.good");

  Run({"jaago", "com.example.bad"});

  EXPECT_TRUE(Exists(user_agents_ + "/com.example.good.plist"));
}

TEST_F(QuarantineTest, RequiresALabel) {
  const RunResult result = Run({"jaago"});

  EXPECT_NE(result.exit_code, 0);
  EXPECT_THAT(result.output, HasSubstr("label"));
}

// Documents the bash engine's behaviour: the label is used as an unanchored pattern for
// `pkill -9 -f`, which can match unrelated processes (SHE-R-003).
TEST_F(QuarantineTest, BashEngineKillsByUnanchoredLabelPattern) {
  if (UsingRustEngine()) GTEST_SKIP() << "behaviour replaced in the Rust engine";
  AddUserAgent("com.example.bad");

  Run({"jaago", "com.example.bad"});

  EXPECT_THAT(StubCalls(), HasSubstr("pkill -9 -f com.example.bad"));
}

// FR-S-15: the Rust engine stops the job through the service manager and never kills by name pattern.
TEST_F(QuarantineTest, NeverKillsProcessesByNamePattern) {
  if (!UsingRustEngine()) GTEST_SKIP() << "bash engine uses pkill";
  AddUserAgent("com.example.bad");

  Run({"jaago", "com.example.bad"});

  EXPECT_THAT(StubCalls(), Not(HasSubstr("pkill")));
  EXPECT_THAT(StubCalls(), HasSubstr("launchctl bootout"));
}

class TrustFileTest : public SheshnagTest {};

TEST_F(TrustFileTest, RecordsThePathAndItsHash) {
  const std::string path = AddUserAgent("com.example.chill");

  const RunResult result = Run({"theek", path});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(ReadFile(state_ + "/baseline/accepted"), HasSubstr(path + "\t"));
}

TEST_F(TrustFileTest, RejectsAFileThatDoesNotExist) {
  const RunResult result = Run({"theek", home_ + "/missing.plist"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("no such file"));
}

TEST_F(TrustFileTest, RequiresAPath) {
  EXPECT_NE(Run({"theek"}).exit_code, 0);
}

// Sakshi (Layer 1) must only observe. A scan with findings changes nothing on disk
// outside its own state folder.
class WatchOnlyTest : public SheshnagTest {};

TEST_F(WatchOnlyTest, ScanWithFindingsDoesNotMoveDeleteOrKill) {
  AddUserAgent("com.example.known");
  Run({"baseline"});
  const std::string suspect = AddUserAgent("com.example.suspect", {"/tmp/payload"});
  SetProcesses({"4242 5.0 " + CurrentUser() + " /tmp/xmrig xmrig"});

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_TRUE(Exists(suspect));
  const std::string calls = StubCalls();
  EXPECT_THAT(calls, Not(HasSubstr("pkill")));
  EXPECT_THAT(calls, Not(HasSubstr("launchctl")));
  EXPECT_THAT(result.output, HasSubstr("Nothing was changed"));
}

}  // namespace
}  // namespace sheshnag::qa
