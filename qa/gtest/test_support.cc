#include "test_support.h"

#include <pwd.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <sstream>

#ifndef SHESHNAG_SCRIPT_PATH
#error "SHESHNAG_SCRIPT_PATH must be defined by the build"
#endif

namespace fs = std::filesystem;

namespace sheshnag::qa {
namespace {

std::string XmlEscape(const std::string& in) {
  std::string out;
  for (char c : in) {
    switch (c) {
      case '&': out += "&amp;"; break;
      case '<': out += "&lt;"; break;
      case '>': out += "&gt;"; break;
      default: out += c;
    }
  }
  return out;
}

}  // namespace

void SheshnagTest::SetUp() {
  const char* base = std::getenv("TEST_TMPDIR");  // Bazel convention, falls back to /tmp
  std::string pattern = std::string(base ? base : "/tmp") + "/sheshnag_qa_XXXXXX";
  std::vector<char> buf(pattern.begin(), pattern.end());
  buf.push_back('\0');
  ASSERT_NE(mkdtemp(buf.data()), nullptr) << "cannot create sandbox";
  root_ = fs::canonical(buf.data()).string();

  home_ = root_ + "/home";
  user_agents_ = home_ + "/Library/LaunchAgents";
  system_agents_ = root_ + "/system/LaunchAgents";
  system_daemons_ = root_ + "/system/LaunchDaemons";
  state_ = home_ + "/.sheshnag";
  stubs_ = root_ + "/bin";
  for (const auto& d : {user_agents_, system_agents_, system_daemons_, stubs_}) {
    fs::create_directories(d);
  }

  // Recording stubs. Each appends its argv to $STUB_CALLS so tests can assert on it.
  InstallStub("launchctl", "echo \"launchctl $*\" >> \"$STUB_CALLS\"\n");
  InstallStub("pkill", "echo \"pkill $*\" >> \"$STUB_CALLS\"\n");
  InstallStub("osascript",
              "case \"$*\" in\n"
              "  *\"display notification\"*) echo \"notify $*\" >> \"$STUB_CALLS\" ;;\n"
              "  *\"login item\"*) [ -f \"$STUB_LOGIN\" ] && cat \"$STUB_LOGIN\" ;;\n"
              "esac\nexit 0\n");
  InstallStub("crontab",
              "if [ \"$1\" = \"-l\" ]; then [ -f \"$STUB_CRON\" ] && cat \"$STUB_CRON\" || exit 1; fi\n");
  InstallStub("ps", "cat \"$STUB_PS\"\n");
  SetProcesses({});
}

void SheshnagTest::TearDown() {
  std::error_code ignored;
  if (!root_.empty()) fs::remove_all(root_, ignored);
}

void SheshnagTest::InstallStub(const std::string& name, const std::string& body) {
  const std::string path = stubs_ + "/" + name;
  WriteFile(path, "#!/bin/bash\n" + body);
  chmod(path.c_str(), 0755);
}

std::vector<std::string> SheshnagTest::Environment() const {
  return {
      "HOME=" + home_,
      "PATH=" + stubs_ + ":/usr/bin:/bin:/usr/sbin:/sbin",
      "SHESHNAG_AGENT_DIRS=" + user_agents_ + " " + system_agents_ + " " + system_daemons_,
      "STUB_CALLS=" + root_ + "/calls.log",
      "STUB_PS=" + root_ + "/ps.out",
      "STUB_CRON=" + root_ + "/cron.txt",
      "STUB_LOGIN=" + root_ + "/login.txt",
      "LC_ALL=C",
  };
}

RunResult SheshnagTest::Run(const std::vector<std::string>& args) const {
  int fds[2];
  EXPECT_EQ(pipe(fds), 0);
  const pid_t pid = fork();
  if (pid == 0) {
    dup2(fds[1], STDOUT_FILENO);
    dup2(fds[1], STDERR_FILENO);
    close(fds[0]);
    close(fds[1]);
    // SHESHNAG_BIN switches the engine under test from the bash script to the Rust CLI.
    const char* bin = std::getenv("SHESHNAG_BIN");
    std::vector<std::string> argv_s =
        bin ? std::vector<std::string>{bin} : std::vector<std::string>{"/bin/bash", SHESHNAG_SCRIPT_PATH};
    argv_s.insert(argv_s.end(), args.begin(), args.end());
    std::vector<char*> argv;
    for (auto& a : argv_s) argv.push_back(a.data());
    argv.push_back(nullptr);
    std::vector<std::string> env_s = Environment();
    std::vector<char*> envp;
    for (auto& e : env_s) envp.push_back(e.data());
    envp.push_back(nullptr);
    execve(argv[0], argv.data(), envp.data());
    _exit(127);
  }
  close(fds[1]);
  RunResult result;
  char chunk[4096];
  ssize_t n;
  while ((n = read(fds[0], chunk, sizeof chunk)) > 0) result.output.append(chunk, n);
  close(fds[0]);
  int status = 0;
  waitpid(pid, &status, 0);
  result.exit_code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return result;
}

std::string SheshnagTest::AddAgent(const std::string& dir, const std::string& label,
                                   const std::vector<std::string>& program_args) {
  std::ostringstream xml;
  xml << "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
      << "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" "
         "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n"
      << "<plist version=\"1.0\"><dict>\n"
      << "<key>Label</key><string>" << XmlEscape(label) << "</string>\n"
      << "<key>ProgramArguments</key><array>\n";
  for (const auto& a : program_args) xml << "<string>" << XmlEscape(a) << "</string>\n";
  xml << "</array></dict></plist>\n";
  const std::string path = dir + "/" + label + ".plist";
  WriteFile(path, xml.str());
  return path;
}

std::string SheshnagTest::AddUserAgent(const std::string& label,
                                       const std::vector<std::string>& a) {
  return AddAgent(user_agents_, label, a);
}

std::string SheshnagTest::AddSystemAgent(const std::string& label,
                                         const std::vector<std::string>& a) {
  return AddAgent(system_agents_, label, a);
}

void SheshnagTest::SetProcesses(const std::vector<std::string>& rows) {
  std::string text = "  PID  %CPU USER COMM ARGS\n";
  for (const auto& r : rows) text += r + "\n";
  WriteFile(root_ + "/ps.out", text);
}

void SheshnagTest::SetCrontab(const std::string& text) { WriteFile(root_ + "/cron.txt", text); }

void SheshnagTest::SetLoginItems(const std::string& items) {
  WriteFile(root_ + "/login.txt", items + "\n");
}

bool SheshnagTest::UsingRustEngine() { return std::getenv("SHESHNAG_BIN") != nullptr; }

std::string SheshnagTest::CurrentUser() const {
  const passwd* pw = getpwuid(getuid());
  return pw ? pw->pw_name : "unknown";
}

void SheshnagTest::WriteFile(const std::string& path, const std::string& contents) {
  fs::create_directories(fs::path(path).parent_path());
  std::ofstream out(path, std::ios::binary | std::ios::trunc);
  out << contents;
}

std::string SheshnagTest::ReadFile(const std::string& path) {
  std::ifstream in(path, std::ios::binary);
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

bool SheshnagTest::Exists(const std::string& path) { return fs::exists(path); }

int SheshnagTest::CountLines(const std::string& text) {
  int n = 0;
  for (char c : text) n += (c == '\n');
  return n;
}

std::string SheshnagTest::StubCalls() const { return ReadFile(root_ + "/calls.log"); }
std::string SheshnagTest::Findings() const { return ReadFile(state_ + "/findings.now"); }
std::string SheshnagTest::ActivityLog() const { return ReadFile(state_ + "/log"); }

}  // namespace sheshnag::qa
