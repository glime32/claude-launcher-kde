#!/usr/bin/env python3
"""Reopening after a crash / shutdown, in a throwaway home with fake claude, KDE and Konsole.

    python3 tools/test-reprise.py [work dir]      (built binaries in target/release)
"""
import json, os, pty, select, shutil, signal, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target/release")
T = sys.argv[1] if len(sys.argv) > 1 else tempfile.mkdtemp(prefix="test-reprise-")
SID = "33333333-3333-4333-8333-333333333333"
HOME, RUN, STATE = f"{T}/home", f"{T}/run", f"{T}/state"
REC = f"{STATE}/claude-window/fenetres"
failures = 0


def report(name, ok, detail=""):
    global failures
    failures += not ok
    print(("OK    " if ok else "ÉCHEC ") + name + (f"  — {detail}" if detail else ""))


def write(path, text, mode=0o644):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)
    os.chmod(path, mode)


def setup():
    shutil.rmtree(T, ignore_errors=True)
    for b in ("claude-menu", "claude-hook", "claude-launcher", "claude-window"):
        os.makedirs(f"{HOME}/.local/bin", exist_ok=True)
        os.symlink(f"{BIN}/{b}", f"{HOME}/.local/bin/{b}")
    write(f"{HOME}/.config/claude-window/couleurs", "".join(f"essai{i} 34,197,94\n" for i in range(1, 7)))
    conv = [{"type": "user", "message": {"role": "user", "content": "Bonjour"}, "cwd": HOME, "sessionId": SID},
            {"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Salut"}]}, "cwd": HOME},
            {"type": "custom-title", "customTitle": "Test reprise"}]
    write(f"{HOME}/.claude/projects/-test/{SID}.jsonl", "".join(json.dumps(l) + "\n" for l in conv))
    fake = f"{T}/fake"
    write(f"{fake}/claude", f"""#!/bin/bash
env | grep -E '^CLAUDE_(REPRISE|FRAME)' | sort > {T}/claude-env.txt
trap 'echo hup >> {T}/claude-signal.txt; exit 129' HUP
case "$(cat {T}/mode)" in
  exit) sleep 0.5; exit 0 ;;
  wait) for i in $(seq 100); do sleep 0.1; done ;;
esac
""", 0o755)
    write(f"{fake}/qdbus6", f"""#!/bin/bash
case "$*" in *isShuttingDown*) cat {T}/arret ;; *GetActive*) echo false ;; esac
""", 0o755)
    write(f"{fake}/systemctl", "#!/bin/bash\necho running\n", 0o755)
    write(f"{fake}/loginctl", "#!/bin/bash\necho LockedHint=no\n", 0o755)
    write(f"{fake}/notify-send", f"#!/bin/bash\necho \"$*\" >> {T}/notif.txt\n", 0o755)
    write(f"{fake}/konsole", f"""#!/bin/bash
echo "$CLAUDE_FRAME_COLOR $CLAUDE_RESUME_SID ${{CLAUDE_REPRISE_ETAT:--}}" >> {T}/konsole.txt
""", 0o755)


def env(**extra):
    e = {"HOME": HOME, "XDG_RUNTIME_DIR": RUN, "XDG_STATE_HOME": STATE, "XDG_CACHE_HOME": f"{T}/cache",
         "PATH": f"{T}/fake:/usr/bin:/bin", "TERM": "xterm-256color", "CLAUDE_FRAME_COLOR": "essai1",
         "CLAUDE_FRAME_RGB": "34,197,94"}
    e.update(extra)
    return e


def record(color="essai1"):
    try:
        return open(f"{REC}/{color}").read().split("\n")[:2]
    except OSError:
        return None


def launcher(mode, shutting_down="false", close_after=None, kill_after=None, **extra):
    """Launcher resuming SID directly; optionally closes the window or kills the launcher."""
    write(f"{T}/mode", mode)
    write(f"{T}/arret", shutting_down + "\n")
    for f in ("claude-signal.txt", "claude-env.txt"):
        if os.path.exists(f"{T}/{f}"):
            os.unlink(f"{T}/{f}")
    pid, fd = pty.fork()
    if pid == 0:
        os.execve(f"{BIN}/claude-launcher", ["claude-launcher"], env(CLAUDE_RESUME_SID=SID, **extra))
    seen, start = None, time.time()
    while time.time() - start < 8:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                os.read(fd, 65536)
            except OSError:
                pass
        if seen is None and record():
            seen = record()
        if close_after and time.time() - start > close_after:
            os.close(fd)  # Konsole closes the window: the terminal hangs up
            close_after, fd = None, os.open("/dev/null", os.O_RDONLY)
        if kill_after and time.time() - start > kill_after:
            os.kill(pid, signal.SIGKILL)  # crash
            kill_after = None
        p, st = os.waitpid(pid, os.WNOHANG)
        if p:
            return os.waitstatus_to_exitcode(st), seen
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    return "bloqué", seen


def hook(state, payload, reprise=True):
    e = env(CLAUDE_KONSOLE_PID="")
    if reprise:
        e["CLAUDE_REPRISE"] = "1"
    subprocess.run([f"{BIN}/claude-hook", state], input=json.dumps(payload).encode(), env=e, stdout=subprocess.DEVNULL)


def leftovers():
    out = subprocess.run(["pgrep", "-af", T], capture_output=True, text=True).stdout.splitlines()
    return [l for l in out if "test-reprise.py" not in l]


setup()
os.makedirs(f"{RUN}/claude-couleurs", exist_ok=True)

# hook
write(f"{RUN}/claude-couleurs/essai1.session", SID)
hook("travail", {})
report("hook : travail enregistré", record() == [SID, "travail"], str(record()))
hook("fini", {"last_assistant_message": "C'est fait."})
report("hook : réponse finie", record() == [SID, "fini"], str(record()))
hook("attente", {"message": "Claude needs your permission to use Bash"})
report("hook : demande d'autorisation", record() == [SID, "attente"], str(record()))
os.unlink(f"{REC}/essai1")
hook("travail", {}, reprise=False)
hook("fini", {}, reprise=False)
report("hook : rien sans CLAUDE_REPRISE (anciennes fenêtres)", record() is None)
os.unlink(f"{RUN}/claude-couleurs/essai1.session")

# launcher
code, seen = launcher("exit")
report("Claude quitte normalement → oublié", seen == [SID, ""] and record() is None, f"code {code}, vu {seen}")
e = open(f"{T}/claude-env.txt").read()
report("Claude reçoit CLAUDE_REPRISE=1 sans CLAUDE_REPRISE_ETAT", "CLAUDE_REPRISE=1" in e and "ETAT" not in e, e.replace("\n", " "))

code, seen = launcher("wait", close_after=1.0)
sig = os.path.exists(f"{T}/claude-signal.txt")
report("fenêtre fermée à la main → oublié, Claude arrêté", record() is None and sig and code == 1, f"code {code}, SIGHUP reçu {sig}")

code, seen = launcher("wait", shutting_down="true", close_after=1.0)
report("fenêtre fermée par l'arrêt de KDE → gardé", record() == [SID, ""] and code == 1, f"code {code}, {record()}")
os.unlink(f"{REC}/essai1")

code, seen = launcher("wait", kill_after=1.0)
time.sleep(0.3)
report("plantage (launcher tué) → gardé", record() == [SID, ""], str(record()))
subprocess.run(["pkill", "-f", f"{T}/fake/claude"])

code, seen = launcher("exit", CLAUDE_REPRISE_ETAT="travail")
report("fenêtre rouverte garde l'état d'avant", seen == [SID, "travail"], str(seen))

# reopening at login
for color, state in [("essai1", "travail"), ("essai2", "fini"), ("essai3", ""), ("essai4", "attente")]:
    write(f"{REC}/{color}", f"{SID}\n{state}\n")
write(f"{REC}/essai5", f"{SID}\nattente\n")
dummy = subprocess.Popen(["bash", "-c", "sleep 5; true", "--desktopfile", "claude-code-essai5", "x"])
write(f"{REC}/essai6", "44444444-0000-4000-8000-000000000000\ntravail\n")
subprocess.run([f"{BIN}/claude-window", "--reprise"], env=env())
time.sleep(1.5)
opened = sorted(open(f"{T}/konsole.txt").read().split("\n")[:-1]) if os.path.exists(f"{T}/konsole.txt") else []
expected = sorted([f"essai1 {SID} travail", f"essai2 {SID} fini", f"essai3 {SID} -", f"essai4 {SID} attente"])
report("connexion : rouvre toutes les fenêtres, quel que soit leur état", opened == expected, str(opened))
report("connexion : ignore conversation supprimée et fenêtre encore ouverte",
       sorted(os.listdir(REC)) == ["essai5"], str(sorted(os.listdir(REC))))
dummy.kill(); dummy.wait()

time.sleep(0.3)
left = leftovers()
report("aucun processus restant", not left, str(left))
print("tout est bon" if failures == 0 else f"{failures} échec(s)")
sys.exit(failures != 0)
