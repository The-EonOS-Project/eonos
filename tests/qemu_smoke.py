#!/usr/bin/env python3
"""Build EonOS in a temporary directory and smoke-test it in headless QEMU."""

from __future__ import annotations

import os
import re
import resource
import selectors
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROMPT = b"superuser@eonos:/# "

def qemu_display_starts(qemu: str, model: str) -> bool:
    def disable_core_dumps() -> None:
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    try:
        subprocess.run(
            [
                qemu,
                "-M", "q35",
                "-nodefaults",
                "-display", "none",
                "-S",
                "-monitor", "none",
                "-serial", "none",
                "-device", model,
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=1.0,
            check=False,
            preexec_fn=disable_core_dumps,
        )
    except subprocess.TimeoutExpired:
        return True
    return False

class SmokeTest:
    def __init__(self, process: subprocess.Popen[bytes], monitor_path: Path, timeout: float = 30.0):
        self.process = process
        self.monitor_path = monitor_path
        self.timeout = timeout
        self.output = bytearray()
        self.cursor = 0
        self.selector = selectors.DefaultSelector()
        assert process.stdout is not None
        self.selector.register(process.stdout, selectors.EVENT_READ)

    def read_until(self, needle: bytes, start: int, description: str) -> bytes:
        deadline = time.monotonic() + self.timeout
        while True:
            found = self.output.find(needle, start)
            if found >= 0:
                return bytes(self.output[start : found + len(needle)])
            if self.process.poll() is not None:
                self.fail(f"QEMU exited while waiting for {description}")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                self.fail(f"Timed out waiting for {description}")
            events = self.selector.select(min(remaining, 0.25))
            for key, _ in events:
                chunk = os.read(key.fd, 4096)
                if chunk:
                    self.output.extend(chunk)

    def expect(self, needle: bytes, description: str, start: int = 0) -> None:
        self.read_until(needle, start, description)

    def command(self, line: str, expected: tuple[bytes, ...] = ()) -> bytes:
        assert self.process.stdin is not None
        start = len(self.output)
        self.process.stdin.write(line.encode("utf-8") + b"\n")
        self.process.stdin.flush()
        response = self.read_until(PROMPT, start, f"prompt after {line!r}")
        for marker in expected:
            if marker not in response:
                self.fail(f"{line!r} did not produce {marker!r}")
        print(f"PASS shell: {line or '<blank line>'}")
        return response

    def command_bytes(self, data: bytes, description: str, expected: tuple[bytes, ...] = ()) -> bytes:
        assert self.process.stdin is not None
        start = len(self.output)
        self.process.stdin.write(data)
        self.process.stdin.flush()
        response = self.read_until(PROMPT, start, f"prompt after {description}")
        for marker in expected:
            if marker not in response:
                self.fail(f"{description} did not produce {marker!r}")
        print(f"PASS shell input: {description}")
        return response

    def interactive_command(
        self, line: str, input_line: str, input_prompt: bytes, expected: tuple[bytes, ...] = ()
    ) -> bytes:
        assert self.process.stdin is not None
        start = len(self.output)
        self.process.stdin.write(line.encode("utf-8") + b"\n")
        self.process.stdin.flush()
        self.read_until(input_prompt, start, f"input prompt from {line!r}")
        self.process.stdin.write(input_line.encode("utf-8") + b"\n")
        self.process.stdin.flush()
        response = self.read_until(PROMPT, start, f"shell prompt after {line!r}")
        for marker in expected:
            if marker not in response:
                self.fail(f"{line!r} did not produce {marker!r}")
        print(f"PASS shell input: {line!r}")
        return response

    def start_program(self, line: str, prompt: bytes) -> bytes:
        assert self.process.stdin is not None
        start = len(self.output)
        self.process.stdin.write(line.encode("utf-8") + b"\n")
        self.process.stdin.flush()
        return self.read_until(prompt, start, f"interactive prompt from {line!r}")

    def monitor_command(self, command: str) -> bytes:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as monitor:
            monitor.settimeout(5)
            monitor.connect(str(self.monitor_path))

            def read_prompt() -> bytes:
                response = bytearray()
                while b"(qemu) " not in response:
                    chunk = monitor.recv(4096)
                    if not chunk:
                        raise RuntimeError("QEMU monitor closed before its prompt")
                    response.extend(chunk)
                return bytes(response)

            read_prompt()
            monitor.sendall(command.encode("ascii") + b"\n")
            return read_prompt()

    def mouse_click_to(self, x: int, y: int, current_x: int, current_y: int) -> None:
        while current_x != x or current_y != y:
            dx = max(-80, min(80, x - current_x))
            dy = max(-80, min(80, y - current_y))
            self.monitor_command(f"mouse_move {dx} {dy}")
            current_x += dx
            current_y += dy
            time.sleep(0.08)
        self.monitor_command("mouse_button 1")
        time.sleep(0.12)
        self.monitor_command("mouse_button 0")
        time.sleep(0.18)

    def interactive_line(
        self, line: str, prompt: bytes, expected: tuple[bytes, ...] = ()
    ) -> bytes:
        assert self.process.stdin is not None
        start = len(self.output)
        self.process.stdin.write(line.encode("utf-8") + b"\n")
        self.process.stdin.flush()
        response = self.read_until(prompt, start, f"prompt after interactive input {line!r}")
        for marker in expected:
            if marker not in response:
                self.fail(f"interactive input {line!r} did not produce {marker!r}")
        print(f"PASS rust shell: {line!r}")
        return response

    def fail(self, message: str) -> None:
        output = bytes(self.output).decode("utf-8", errors="replace")
        raise RuntimeError(f"{message}\n--- QEMU serial output ---\n{output[-12000:]}")

def run() -> None:
    qemu = shutil.which("qemu-system-x86_64")
    if qemu is None:
        raise RuntimeError("qemu-system-x86_64 is required")

    device_list = subprocess.run(
        [qemu, "-device", "help"], capture_output=True, text=True, check=False
    )
    available_devices = device_list.stdout + device_list.stderr
    video_model = "std"
    for candidate in ("virtio-gpu-pci", "virtio-vga"):
        if f'name "{candidate}"' in available_devices:
            if qemu_display_starts(qemu, candidate):
                video_model = candidate
                break
            print(f"QEMU {candidate} failed its host startup probe")

    with tempfile.TemporaryDirectory(prefix="eonos-qemu-smoke-") as tmp:
        build_dir = Path(tmp) / "build"
        env = os.environ.copy()
        env["EONOS_BUILD_DIR"] = str(build_dir)

        print("Building EonOS in a temporary directory...")
        subprocess.run([str(ROOT / "build.sh")], cwd=ROOT, env=env, check=True)
        iso = build_dir / "iso_output" / "eonos.iso"
        disk = build_dir / "eonos_disk.img"
        monitor_path = Path(tmp) / "qemu-monitor.sock"
        if not iso.is_file():
            raise RuntimeError(f"build did not produce {iso}")
        if not disk.is_file() or disk.stat().st_size != 4 * 1024**3:
            raise RuntimeError("build did not produce the expected 4 GiB disk image")

        video_args = ["-device", video_model] if video_model != "std" else ["-vga", "std"]
        print(f"Using QEMU display device: {video_model}")

        args = [
            qemu,
            "-M", "q35,i8042=off",
            "-m", "2G",
            *video_args,
            "-display", "none",
            "-monitor", f"unix:{monitor_path},server,nowait",
            "-no-reboot",
            "-no-shutdown",
            "-serial", "stdio",
            "-cdrom", str(iso),
            "-boot", "d",
            "-device", "qemu-xhci,id=xhci",
            "-device", "usb-kbd,bus=xhci.0",
            "-device", "usb-mouse,bus=xhci.0",
            "-drive", f"file={disk},format=raw,if=none,id=eonos_nvme",
            "-device", "nvme,drive=eonos_nvme,serial=EONOSNVME0001",
        ]

        def boot() -> SmokeTest:
            print("Booting headless QEMU...")
            monitor_path.unlink(missing_ok=True)
            process = subprocess.Popen(
                args,
                cwd=ROOT,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                bufsize=0,
            )
            smoke = SmokeTest(process, monitor_path)
            smoke.expect(b"Mapped, wrote, translated and unmapped", "VMM self-test")
            smoke.expect(b"/init found", "initramfs self-test")
            smoke.expect(b"Dropped to ring 3", "syscall self-test")
            smoke.expect(PROMPT, "initial shell prompt")
            print("PASS boot: VMM, initramfs, syscall self-tests and shell prompt")
            return smoke

        def stop(smoke: SmokeTest) -> None:
            process = smoke.process
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            smoke.selector.close()
            if process.stdin is not None:
                process.stdin.close()
            if process.stdout is not None:
                process.stdout.close()

        print("Booting headless QEMU with the persistent NVMe image...")
        first = boot()
        try:
            first.command("", ())
            first.command("help", (b"commands:", b"lsblk", b"mount", b"panic", b"exit"))
            first.command("he\t", (b"head", b"help"))
            first.command("cat /etc/mo\t", (b"Welcome to EonOS.",))
            initial_lsblk = first.command("lsblk", (b"nvme0n1", b"vfat"))
            if b"/disk" in initial_lsblk:
                first.fail("the persistent disk was mounted automatically")
            first.command("lsblk -f", (b"usage: lsblk",))
            first.command("ls", (b"init", b"asm-shell", b"rust-shell", b"hello"))
            first.command("cat /etc/motd", (b"Welcome to EonOS.",))
            first.command("echo history-check", (b"history-check",))
            first.command_bytes(b"\x1b[A\n", "up-arrow history", (b"history-check",))
            first.command("exec /hello", (b"hello from /hello",))
            first.command(
                "exec /rust-hello smoke",
                (b"hello from Rust", b"arg: smoke", b"heap: 2 values; dynamic Rust memory works"),
            )
            era_start = first.command("era &", (b"started era &",))
            match = re.search(rb"\[(\d+)\] started era &", era_start)
            if match is None:
                first.fail("Era did not start as a background process")
            era_pid = match.group(1).decode("ascii")
            first.command("sleep 500")
            doom_output_start = len(first.output)
            first.mouse_click_to(670, 750, 640, 400)  
            first.mouse_click_to(640, 308, 670, 750)  
            first.expect(
                b"doom: engine initialized; rendering game frames",
                "DOOM engine initialization and first frame",
                doom_output_start,
            )
            first.monitor_command("sendkey ctrl-q")
            first.expect(
                b"doom: received Ctrl+Q; returning to Era",
                "DOOM keyboard exit event",
                doom_output_start,
            )
            first.expect(
                b"doom: game loop stopped; releasing resources",
                "DOOM game loop shutdown",
                doom_output_start,
            )
            time.sleep(0.5)
            if f"[{era_pid}] running".encode() not in first.command_bytes(
                b"\x7fjobs\n", "Era status after first app return"
            ):
                first.fail("Era did not remain active after DOOM exited")
            second_doom_start = len(first.output)
            first.mouse_click_to(670, 750, 640, 308)  
            first.mouse_click_to(640, 308, 670, 750)  
            first.expect(
                b"doom: engine initialized; rendering game frames",
                "DOOM relaunch after returning to Era",
                second_doom_start,
            )
            first.monitor_command("sendkey ctrl-q")
            first.expect(
                b"doom: received Ctrl+Q; returning to Era",
                "DOOM keyboard exit on relaunch",
                second_doom_start,
            )
            first.expect(
                b"doom: game loop stopped; releasing resources",
                "DOOM relaunch cleanup",
                second_doom_start,
            )
            time.sleep(0.5)
            if f"[{era_pid}] running".encode() not in first.command_bytes(
                b"\x7fjobs\n", "Era jobs after app return"
            ):
                first.fail("Era did not resume after DOOM exited")
            print("PASS era: mouse launch, first frame, keyboard exit, relaunch, and clean return")
            first.command(f"kill {era_pid}", (b"SIGTERM",))
            first.command("jobs", (f"[{era_pid}] done (143) era &".encode(),))
            first.command(f"wait {era_pid}")
            print("PASS era: graphics session opened, presented and restored the terminal on exit")
            doom_start = first.command("exec /bin/doom &", (b"started /bin/doom &",))
            match = re.search(rb"\[(\d+)\] started /bin/doom &", doom_start)
            if match is None:
                first.fail("DOOM did not start as a background user process")
            doom_pid = match.group(1).decode("ascii")
            first.command("sleep 1500")
            doom_jobs = first.command("jobs")
            if f"[{doom_pid}] running".encode() not in doom_jobs:
                first.fail("DOOM exited before the WAD and game loop smoke check completed")
            first.command(f"kill {doom_pid}", (b"SIGTERM",))
            first.command("jobs", (f"[{doom_pid}] done (143)".encode(),))
            first.command(f"wait {doom_pid}")
            print("PASS doom: bundled IWAD loaded in a user process and process cleanup completed")
            first.command(
                "exec /rust-process-demo",
                (b"process demo: spawned pid", b"hello from /hello", b"process demo: pid", b"exited 0"),
            )
            first.interactive_command(
                "exec /rust-input-demo",
                "typed from smoke test",
                b"input demo> ",
                (b"read: typed from smoke test",),
            )
            first.command("write /disk/early.txt should-fail", (b"error:",))
            first.command("echo \"hello from rust shell\"", (b"hello from rust shell",))
            first.command("cat /missing-file", (b"no such file or directory",))
            first.command("uptime", (b" ms",))
            first.command("pid")
            first.command("ps", (b"PID  STATE",))
            first.command("mount /device/nvme0n1 /disk", (b"mount: mounted",))
            first.command("exec /bin/doom -iwad /disk/missing.wad", (b"could not open IWAD", b"exited 2"))
            first.command("write /disk/not-a-wad.wad not-an-iwad")
            first.command(
                "exec /bin/doom -iwad /disk/not-a-wad.wad",
                (b"invalid IWAD header", b"exited 2"),
            )
            first.command("rm /disk/not-a-wad.wad")
            first.command("mkdir /disk/rust-shell-smoke")
            first.command('write /disk/rust-shell-smoke/one.txt "hello rust shell"', (b"write: saved",))
            first.command("cat /disk/rust-shell-smoke/on\t", (b"hello rust shell",))
            first.command("cat /disk/rust-shell-smoke/one.txt", (b"hello rust shell",))
            first.command("cp /disk/rust-shell-smoke/one.txt /disk/rust-shell-smoke/two.txt", (b"cp: copied",))
            first.command("touch /disk/rust-shell-smoke/empty.txt", (b"touch: created",))
            first.start_program("edit /disk/rust-shell-smoke/edit.txt", b"edit> ")
            first.interactive_line("appended line", b"edit> ")
            first.interactive_line(".save", PROMPT, (b"[ saved ]",))
            first.command("cat /disk/rust-shell-smoke/edit.txt", (b"appended line",))
            first.command("rm /disk/rust-shell-smoke/empty.txt")
            first.command("rm /disk/rust-shell-smoke/one.txt")
            first.command("rm /disk/rust-shell-smoke/two.txt")
            first.command("rm /disk/rust-shell-smoke/edit.txt")
            first.command("rm /disk/rust-shell-smoke")
            first.command("sync", (b"sync: storage reports durable writes",))
            first.command("exec /bin/echo standalone echo", (b"standalone echo",))
            first.command("exec /bin/cat /etc/motd", (b"Welcome to EonOS.",))
            first.command("exec /bin/ls /bin", (b"echo", b"era"))
            job_start = first.command("exec /bin/sleep 60000 &", (b"started",))
            match = re.search(rb"\[(\d+)\] started", job_start)
            if match is None:
                first.fail("background sleep did not report its pid")
            job_pid = match.group(1).decode("ascii")
            first.command("jobs", (f"[{job_pid}] running".encode(),))
            first.command(f"kill {job_pid}", (b"SIGTERM",))
            first.command("jobs", (f"[{job_pid}] done (143)".encode(),))
            first.command(f"wait {job_pid}")
            first.command("exec /rust-file-demo", (b"file demo: Rust file API works",))
            first.command("lsblk", (b"nvme0n1", b"/disk"))
            first.command("exec /disk/HELLO.ELF", (b"hello from /hello",))
            first.command("mkdir /disk/smoke")
            first.command("write /disk/smoke/persist.txt survives-reboot", (b"write: saved",))
            first.command("cat /disk/smoke/persist.txt", (b"survives-reboot",))
            first.command("sync", (b"sync: storage reports durable writes",))
        finally:
            stop(first)

        print("Rebuilding while preserving the data image...")
        subprocess.run([str(ROOT / "build.sh")], cwd=ROOT, env=env, check=True)
        print("Rebooting QEMU with the same disk image...")
        second = boot()
        try:
            second.command("mount /device/nvme0n1 /disk", (b"mount: mounted",))
            second.command("lsblk", (b"nvme0n1", b"/disk"))
            second.command("cat /disk/smoke/persist.txt", (b"survives-reboot",))
            second.command("rm /disk/smoke/persist.txt")
            second.command("rm /disk/smoke")
            second.command("sync", (b"sync: storage reports durable writes",))
            second.command("smoke-unknown", (b"smoke-unknown: command not found",))

            assert second.process.stdin is not None
            panic_start = len(second.output)
            second.process.stdin.write(b"panic\n")
            second.process.stdin.flush()
            second.expect(b"kernel panic requested by process", "panic command kernel log", panic_start)
            second.expect(b"KERNEL PANIC -- system halted", "panic halt log", panic_start)
            print("PASS shell: panic enters the kernel panic handler")
        finally:
            stop(second)

    print("QEMU smoke test passed")

if __name__ == "__main__":
    try:
        run()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"QEMU smoke test failed: {error}", file=sys.stderr)
        sys.exit(1)
