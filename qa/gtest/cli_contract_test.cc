// Requirement coverage: FR-S-13 (command-line contract the dashboard depends on).
#include <gmock/gmock.h>

#include "test_support.h"

namespace sheshnag::qa {
namespace {

using ::testing::HasSubstr;

class CliContractTest : public SheshnagTest {};

TEST_F(CliContractTest, NoArgumentsPrintsUsageAndExitsWithTwo) {
  const RunResult result = Run({});

  EXPECT_EQ(result.exit_code, 2);
  EXPECT_THAT(result.output, HasSubstr("baseline"));
  EXPECT_THAT(result.output, HasSubstr("dekho"));
}

TEST_F(CliContractTest, UnknownCommandPrintsUsageAndExitsWithTwo) {
  EXPECT_EQ(Run({"frobnicate"}).exit_code, 2);
}

TEST_F(CliContractTest, ScanExitCodesMatchWhatTheDashboardExpects) {
  // The dashboard treats 0 as "no findings" and 1 as "findings need review".
  AddUserAgent("com.example.known");
  Run({"baseline"});
  EXPECT_EQ(Run({"dekho"}).exit_code, 0);

  AddUserAgent("com.example.intruder");
  EXPECT_EQ(Run({"dekho"}).exit_code, 1);
}

TEST_F(CliContractTest, CreatesItsStateFolderOnFirstRun) {
  Run({"dekho"});

  EXPECT_TRUE(Exists(state_ + "/baseline"));
  EXPECT_TRUE(Exists(state_ + "/log"));
}

TEST_F(CliContractTest, ActivityLogLinesCarryATimestampPrefix) {
  Run({"baseline"});

  // "YYYY-MM-DD HH:MM:SS  message", which the dashboard's log view parses.
  const std::string log = ActivityLog();
  ASSERT_GE(log.size(), 21u);
  EXPECT_EQ(log[4], '-');
  EXPECT_EQ(log[10], ' ');
  EXPECT_EQ(log[13], ':');
}

}  // namespace
}  // namespace sheshnag::qa
