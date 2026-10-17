// Requirement coverage: FR-S-01 (baseline), FR-S-10 (state files).
#include <gmock/gmock.h>

#include "test_support.h"

namespace sheshnag::qa {
namespace {

using ::testing::HasSubstr;
using ::testing::Not;

class BaselineTest : public SheshnagTest {};

TEST_F(BaselineTest, RecordsEveryAgentAndReportsTheCount) {
  AddUserAgent("com.example.one");
  AddSystemAgent("com.example.two");

  const RunResult result = Run({"baseline"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, HasSubstr("baseline recorded: 2 agents"));
  EXPECT_EQ(CountLines(ReadFile(state_ + "/baseline/agents")), 2);
}

TEST_F(BaselineTest, RecordsHiddenDotPlistFiles) {
  AddUserAgent(".hidden.example");

  const RunResult result = Run({"baseline"});

  EXPECT_THAT(result.output, HasSubstr("1 agents"));
}

TEST_F(BaselineTest, WithNoAgentsStillSucceeds) {
  const RunResult result = Run({"baseline"});

  EXPECT_EQ(result.exit_code, 0);
  EXPECT_THAT(result.output, HasSubstr("0 agents"));
}

TEST_F(BaselineTest, IsIdempotentWhenNothingChanged) {
  AddUserAgent("com.example.one");
  Run({"baseline"});
  const std::string first = ReadFile(state_ + "/baseline/agents");

  Run({"baseline"});

  EXPECT_EQ(ReadFile(state_ + "/baseline/agents"), first);
}

TEST_F(BaselineTest, RecordsLoginItemsAndCrontab) {
  SetLoginItems("Dropbox, Slack");
  SetCrontab("0 * * * * /usr/bin/true\n");

  const RunResult result = Run({"baseline"});

  EXPECT_THAT(result.output, HasSubstr("2 login items"));
  EXPECT_THAT(ReadFile(state_ + "/baseline/cron"), HasSubstr("/usr/bin/true"));
}

TEST_F(BaselineTest, DoesNotTouchTheAgentsItRecords) {
  const std::string plist = AddUserAgent("com.example.one");
  const std::string before = ReadFile(plist);

  Run({"baseline"});

  EXPECT_EQ(ReadFile(plist), before);
  EXPECT_THAT(StubCalls(), Not(HasSubstr("launchctl")));
}

}  // namespace
}  // namespace sheshnag::qa
