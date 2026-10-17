#!/bin/bash
# Sheshnag (शेषनाग): a thousand-headed watcher for a Mac.
# Witnesses, never guesses, never acts on its own.
#
#   bash sheshnag.sh baseline     record what is on the Mac now as trusted (do this once, after a clean check)
#   bash sheshnag.sh dekho        look once: print anything new or suspicious
#   bash sheshnag.sh install      look every 10 minutes and raise a macOS notification when something is off
#   bash sheshnag.sh uninstall
#   bash sheshnag.sh jaago <label>   quarantine a launch agent by label, on your word only
#   bash sheshnag.sh theek <path>    "this one is fine": stop flagging that file until it changes
#
# What it watches (the ways the 11 Oct loader got in and stayed):
#   1  new or changed launch agents and daemons (user and system)
#   2  launch agents whose command hides something: base64, osascript, curl|bash, /tmp, python -c
#   3  labels that imitate Apple (com.Apple.*, or any com.apple.* in your own LaunchAgents)
#   4  miner processes by name, and any user process eating more than 3 cores
#   5  changes to ~/.zshrc ~/.zprofile ~/.bash_profile and crontab, and pasted curl|bash lines in them
#   6  changes to Login Items
# Everything it finds goes to ~/.sheshnag/log and, when installed, to a notification.
# It needs no sudo. Version 0.2, 15 Oct 2026.
# A notification is raised only for findings that were not there on the previous look.

set -u
HOME_DIR=${HOME}
NEST=$HOME_DIR/.sheshnag
BASE=$NEST/baseline
LOG=$NEST/log
LABEL=in.palanhar.sheshnag
# SHESHNAG_AGENT_DIRS is a test seam: the gtest suite points it at a sandbox so no real system folder is read.
AGENT_DIRS=${SHESHNAG_AGENT_DIRS:-"$HOME_DIR/Library/LaunchAgents /Library/LaunchAgents /Library/LaunchDaemons"}
RC_FILES="$HOME_DIR/.zshrc $HOME_DIR/.zprofile $HOME_DIR/.zshenv $HOME_DIR/.bash_profile $HOME_DIR/.bashrc $HOME_DIR/.profile"
MINERS='xmrig|minerd|kdevtmpfsi|cpuminer|nicehash|xmr-stak|lolminer|t-rex|nbminer|rigupdater|--donate-level|stratum\+tcp'
HIDERS='base64|osascript|curl|wget|/tmp/|/private/tmp|nohup|python[0-9.]* -c|perl -e|eval '
CPU_MAX=300   # percent; 3 cores

mkdir -p "$BASE"
FINDINGS=0

note() { printf '%s  %s\n' "$(date '+%F %T')" "$*" >> "$LOG"; printf '%s\n' "$*"; }
flag() { FINDINGS=$((FINDINGS + 1)); note "!! $*"; }

hash_of() { shasum -a 256 "$1" 2>/dev/null | cut -c1-16; }

# What a launch agent actually runs: its Program and ProgramArguments, nothing else
# (log paths such as StandardOutPath are not commands).
agent_command() {
  if command -v plutil >/dev/null 2>&1; then
    { plutil -extract ProgramArguments json -o - "$1" 2>/dev/null; plutil -extract Program raw -o - "$1" 2>/dev/null; } | tr -d '\n'
  else
    cat "$1"
  fi
}

# A file the person has marked fine, at the hash it had then.
accepted() { [ -f "$BASE/accepted" ] && grep -q "^$1	$(hash_of "$1")$" "$BASE/accepted"; }

snapshot_agents() {   # path<TAB>hash for every plist in the agent dirs
  for d in $AGENT_DIRS; do
    [ -d "$d" ] || continue
    for f in "$d"/*.plist "$d"/.*.plist; do
      [ -f "$f" ] || continue
      printf '%s\t%s\n' "$f" "$(hash_of "$f")"
    done
  done | sort
}

snapshot_rc() {
  for f in $RC_FILES; do [ -f "$f" ] && printf '%s\t%s\n' "$f" "$(hash_of "$f")"; done | sort
}

snapshot_cron() { crontab -l 2>/dev/null | sort; }

snapshot_login_items() {
  osascript -e 'tell application "System Events" to get the name of every login item' 2>/dev/null | tr ',' '\n' | sed 's/^ *//' | sort
}

baseline() {
  snapshot_agents > "$BASE/agents"
  snapshot_rc > "$BASE/rc"
  snapshot_cron > "$BASE/cron"
  snapshot_login_items > "$BASE/login"
  date > "$BASE/when"
  note "baseline recorded: $(wc -l < "$BASE/agents" | tr -d ' ') agents, $(wc -l < "$BASE/login" | tr -d ' ') login items"
}

check_agents() {
  [ -f "$BASE/agents" ] || { note "no baseline yet: run  bash sheshnag.sh baseline"; return; }
  snapshot_agents > "$NEST/agents.now"
  # new or changed
  comm -13 "$BASE/agents" "$NEST/agents.now" | while IFS=$'\t' read -r f h; do
    if grep -q "^$f	" "$BASE/agents"; then flag "launch agent CHANGED: $f"; else flag "launch agent NEW: $f"; fi
  done
  comm -23 "$BASE/agents" "$NEST/agents.now" | while IFS=$'\t' read -r f h; do
    note "launch agent gone: $f"
  done
  # content and name checks on every agent, known or not
  for d in $AGENT_DIRS; do
    [ -d "$d" ] || continue
    for f in "$d"/*.plist "$d"/.*.plist; do
      [ -f "$f" ] || continue
      case "$f" in *"$LABEL"*) continue ;; esac
      b=$(basename "$f")
      case "$b" in
        com.Apple.*) flag "fake Apple label (capital A): $f" ;;
        com.apple.*) case "$d" in "$HOME_DIR"/*) flag "Apple-named agent in your own LaunchAgents, Apple never puts one there: $f" ;; esac ;;
      esac
      if accepted "$f"; then continue; fi
      cmd=$(agent_command "$f")
      if printf '%s' "$cmd" | grep -Eq "$HIDERS"; then
        flag "launch agent runs hidden commands ($(printf '%s' "$cmd" | grep -Eo "$HIDERS" | sort -u | tr '\n' ' ')): $f"
      fi
    done
  done
}

check_processes() {
  ps -Ao pid,pcpu,user,comm,args 2>/dev/null | tail -n +2 | while read -r pid cpu user comm rest; do
    if printf '%s %s' "$comm" "$rest" | grep -Eiq "$MINERS"; then
      flag "miner-like process $pid ($comm) at ${cpu}% CPU: $rest"
      continue
    fi
    c=${cpu%.*}
    if [ "${c:-0}" -gt "$CPU_MAX" ] && [ "$user" = "$(id -un)" ]; then
      case "$comm" in
        */Xcode*|*/clang*|*/rustc*|*/cargo*|*/ffmpeg*|*/Final\ Cut*|*/Compressor*|*/ollama*) ;;
        *) flag "process $pid ($comm) is using ${cpu}% CPU for user $user" ;;
      esac
    fi
  done
}

check_rc_and_cron() {
  [ -f "$BASE/rc" ] || return
  snapshot_rc > "$NEST/rc.now"
  comm -3 "$BASE/rc" "$NEST/rc.now" | cut -f1 | sort -u | while read -r f; do
    [ -n "$f" ] && note "shell start-up file changed: $f"
  done
  for f in $RC_FILES; do
    [ -f "$f" ] || continue
    if grep -Eq 'curl[^|]*\|[ ]*(ba)?sh|base64 (-d|--decode)|osascript' "$f"; then
      flag "shell start-up file runs a pasted-style command: $f  ($(grep -En 'curl[^|]*\|[ ]*(ba)?sh|base64 (-d|--decode)|osascript' "$f" | head -1))"
    fi
  done
  snapshot_cron > "$NEST/cron.now"
  if ! cmp -s "$BASE/cron" "$NEST/cron.now"; then flag "crontab changed: $(diff "$BASE/cron" "$NEST/cron.now" | grep '^[<>]' | head -3 | tr '\n' ' ')"; fi
}

check_login_items() {
  [ -f "$BASE/login" ] || return
  snapshot_login_items > "$NEST/login.now"
  comm -13 "$BASE/login" "$NEST/login.now" | while read -r item; do [ -n "$item" ] && flag "new Login Item: $item"; done
}

dekho() {
  FINDINGS=0
  [ -f "$NEST/findings.now" ] && cp "$NEST/findings.now" "$NEST/findings.prev" || : > "$NEST/findings.prev"
  { check_agents; check_processes; check_rc_and_cron; check_login_items; } | grep '^!!' > "$NEST/findings.now"
  n=$(grep -c '^!!' "$NEST/findings.now")
  if [ "$n" -gt 0 ]; then
    cat "$NEST/findings.now"
    new=$(grep -vxF -f "$NEST/findings.prev" "$NEST/findings.now" | grep -c '^!!')
    note "Sheshnag: $n thing(s) need your eyes ($new new since the last look). Nothing was changed."
    if [ "${1:-}" = quiet ] && [ "$new" -gt 0 ]; then
      osascript -e "display notification \"$new new finding(s). In Terminal: grep '!!' ~/.sheshnag/log\" with title \"Sheshnag\" subtitle \"Something new is on this Mac\"" 2>/dev/null
    fi
    return 1
  fi
  if [ "${1:-}" != quiet ]; then
    if [ -f "$BASE/agents" ]; then note "Sheshnag: all quiet. $(wc -l < "$BASE/agents" | tr -d ' ') agents as recorded, no miner, no new login item."
    else note "Sheshnag: nothing suspicious seen. Record the baseline next:  bash sheshnag.sh baseline"; fi
  fi
  return 0
}

install() {
  cp "$0" "$NEST/sheshnag.sh" 2>/dev/null || true
  cat > "$HOME_DIR/Library/LaunchAgents/$LABEL.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key><array><string>/bin/bash</string><string>$NEST/sheshnag.sh</string><string>dekho</string><string>quiet</string></array>
  <key>StartInterval</key><integer>600</integer>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>$NEST/last-run.txt</string>
  <key>StandardErrorPath</key><string>$NEST/last-run.txt</string>
</dict></plist>
EOF
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null
  launchctl bootstrap "gui/$(id -u)" "$HOME_DIR/Library/LaunchAgents/$LABEL.plist" && note "installed: looks every 10 minutes; log at $LOG"
  # the agent itself is now a launch agent: record it in the baseline so it is not reported as new
  baseline
}

uninstall() {
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null
  rm -f "$HOME_DIR/Library/LaunchAgents/$LABEL.plist"
  note "uninstalled"
}

theek() {   # accept a flagged file as fine until it changes
  f=${1:?path}
  [ -f "$f" ] || { note "no such file: $f"; return 1; }
  printf '%s\t%s\n' "$f" "$(hash_of "$f")" >> "$BASE/accepted"
  note "accepted as fine until it changes: $f"
}

jaago() {   # quarantine by label: unload and move the plist, keep it as evidence
  lbl=${1:?label}
  mkdir -p "$NEST/quarantine"
  launchctl bootout "gui/$(id -u)/$lbl" 2>/dev/null
  for d in $AGENT_DIRS; do
    [ -f "$d/$lbl.plist" ] && mv "$d/$lbl.plist" "$NEST/quarantine/" && note "quarantined $d/$lbl.plist -> $NEST/quarantine/"
  done
  pkill -9 -f "$lbl" 2>/dev/null
  note "jaago done for $lbl; if it was a system daemon, run again with sudo"
}

case "${1:-}" in
  baseline) baseline ;;
  dekho) dekho "${2:-}" ;;
  install) install ;;
  uninstall) uninstall ;;
  jaago) jaago "${2:-}" ;;
  theek) theek "${2:-}" ;;
  *) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
